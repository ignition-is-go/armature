//! Colours as values: conversion between the models a picker works in, hex text, and
//! contrast.

use crate::convert::Cast;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Hue in degrees, saturation and value from 0 to 1. A picker's square is saturation
/// across and value up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsv {
    pub h: f64,
    pub s: f64,
    pub v: f64,
}

/// Hue in degrees, saturation and lightness from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsl {
    pub h: f64,
    pub s: f64,
    pub l: f64,
}

impl Rgb {
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Parses `#rgb` or `#rrggbb`, with or without the `#`.
    #[must_use]
    pub fn from_hex(text: &str) -> Option<Self> {
        let hex = text.trim().trim_start_matches('#');
        if !hex.is_ascii() {
            return None;
        }
        let channel = |digits: &str| u8::from_str_radix(digits, 16).ok();
        match hex.len() {
            3 => {
                let double = |i: usize| channel(&hex.get(i..=i)?.repeat(2));
                Some(Self::new(double(0)?, double(1)?, double(2)?))
            }
            6 => Some(Self::new(
                channel(hex.get(0..2)?)?,
                channel(hex.get(2..4)?)?,
                channel(hex.get(4..6)?)?,
            )),
            _ => None,
        }
    }

    #[must_use]
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    fn unit(self) -> (f64, f64, f64) {
        (
            f64::from(self.r) / 255.0,
            f64::from(self.g) / 255.0,
            f64::from(self.b) / 255.0,
        )
    }

    fn from_unit(r: f64, g: f64, b: f64) -> Self {
        let byte = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round().cast::<u8>();
        Self::new(byte(r), byte(g), byte(b))
    }

    fn hue_and_range(self) -> (f64, f64, f64) {
        let (r, g, b) = self.unit();
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let range = max - min;
        let hue = if range == 0.0 {
            0.0
        } else if (max - r).abs() < f64::EPSILON {
            60.0 * ((g - b) / range).rem_euclid(6.0)
        } else if (max - g).abs() < f64::EPSILON {
            60.0 * ((b - r) / range + 2.0)
        } else {
            60.0 * ((r - g) / range + 4.0)
        };
        (hue, max, min)
    }

    #[must_use]
    pub fn to_hsv(self) -> Hsv {
        let (h, max, min) = self.hue_and_range();
        Hsv {
            h,
            s: if max == 0.0 { 0.0 } else { (max - min) / max },
            v: max,
        }
    }

    #[must_use]
    pub fn to_hsl(self) -> Hsl {
        let (h, max, min) = self.hue_and_range();
        let l = f64::midpoint(max, min);
        let s = if (max - min).abs() < f64::EPSILON {
            0.0
        } else {
            (max - min) / (1.0 - 2.0f64.mul_add(l, -1.0).abs())
        };
        Hsl { h, s, l }
    }

    /// Relative luminance, from 0 for black to 1 for white.
    #[must_use]
    pub fn luminance(self) -> f64 {
        let linear = |c: f64| {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let (r, g, b) = self.unit();
        weighted([0.2126, 0.7152, 0.0722], [linear(r), linear(g), linear(b)])
    }

    /// Contrast ratio against another colour, from 1 to 21. Body text needs 4.5.
    #[must_use]
    pub fn contrast(self, other: Self) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
}

/// Each value times its weight, added up: one row of a colour matrix.
fn weighted(weights: [f64; 3], values: [f64; 3]) -> f64 {
    let [first, second, third] = weights;
    let [x, y, z] = values;
    third.mul_add(z, second.mul_add(y, first * x))
}

#[expect(
    clippy::many_single_char_names,
    reason = "colour channels go by their letters"
)]
fn from_hue(h: f64, chroma: f64, floor: f64) -> Rgb {
    let sector = h.rem_euclid(360.0) / 60.0;
    let x = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (r, g, b) = match sector.cast::<u8>() {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    Rgb::from_unit(r + floor, g + floor, b + floor)
}

impl Hsv {
    #[must_use]
    pub fn to_rgb(self) -> Rgb {
        let chroma = self.v * self.s;
        from_hue(self.h, chroma, self.v - chroma)
    }
}

impl Hsl {
    #[must_use]
    pub fn to_rgb(self) -> Rgb {
        let chroma = (1.0 - 2.0f64.mul_add(self.l, -1.0).abs()) * self.s;
        from_hue(self.h, chroma, self.l - chroma / 2.0)
    }
}

impl Rgb {
    /// This colour with `amount` of `other` mixed in, from 0 for none to 1 for all.
    #[must_use]
    pub fn mix(self, other: Self, amount: f64) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        let blend = |a: u8, b: u8| {
            (f64::mul_add(f64::from(b) - f64::from(a), amount, f64::from(a)).round()).cast::<u8>()
        };
        Self::new(
            blend(self.r, other.r),
            blend(self.g, other.g),
            blend(self.b, other.b),
        )
    }

    /// The colour a basic colour name stands for, in any case.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        let hex = match name.trim().to_lowercase().as_str() {
            "black" => "#000000",
            "silver" => "#c0c0c0",
            "gray" | "grey" => "#808080",
            "white" => "#ffffff",
            "maroon" => "#800000",
            "red" => "#ff0000",
            "purple" => "#800080",
            "fuchsia" | "magenta" => "#ff00ff",
            "green" => "#008000",
            "lime" => "#00ff00",
            "olive" => "#808000",
            "yellow" => "#ffff00",
            "navy" => "#000080",
            "blue" => "#0000ff",
            "teal" => "#008080",
            "aqua" | "cyan" => "#00ffff",
            "orange" => "#ffa500",
            "pink" => "#ffc0cb",
            "brown" => "#a52a2a",
            _ => return None,
        };
        Self::from_hex(hex)
    }

    /// Text read as a colour: a hex value or a basic colour name.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::from_hex(text).or_else(|| Self::named(text))
    }
}

/// How a colour is written out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notation {
    Hex,
    /// `rgb(59 111 224)`
    Rgb,
    /// `hsl(221 73% 55%)`
    Hsl,
    /// `oklch(0.628 0.2577 29.23)`: lightness, colourfulness and hue as the eye
    /// judges them.
    Oklch,
    /// `lab(54.29 80.8 69.89)`: CIE Lab, measured against daylight at D50.
    Lab,
}

impl Rgb {
    /// How different two colours look, from 0 for the same to about 765 for black
    /// against white. Red, green and blue are weighted by how strongly the eye
    /// answers to each.
    #[must_use]
    pub fn distance(self, other: Self) -> f64 {
        let mean = f64::midpoint(f64::from(self.r), f64::from(other.r));
        let (dr, dg, db) = (
            f64::from(self.r) - f64::from(other.r),
            f64::from(self.g) - f64::from(other.g),
            f64::from(self.b) - f64::from(other.b),
        );
        f64::mul_add(
            (2.0 + (255.0 - mean) / 256.0) * db,
            db,
            f64::mul_add(4.0 * dg, dg, (2.0 + mean / 256.0) * dr * dr),
        )
        .sqrt()
    }

    #[must_use]
    pub fn write(self, notation: Notation) -> String {
        match notation {
            Notation::Hex => self.to_hex(),
            Notation::Rgb => format!("rgb({} {} {})", self.r, self.g, self.b),
            Notation::Hsl => {
                let hsl = self.to_hsl();
                format!(
                    "hsl({} {}% {}%)",
                    hsl.h.round(),
                    (hsl.s * 100.0).round(),
                    (hsl.l * 100.0).round()
                )
            }
            Notation::Lab => {
                let (lightness, a, b) = self.to_lab();
                let round = |value: f64| (value * 100.0).round() / 100.0;
                format!("lab({} {} {})", round(lightness), round(a), round(b))
            }
            Notation::Oklch => {
                let (lightness, chroma, hue) = self.to_oklch();
                let round = |value: f64, places: i32| {
                    (value * 10f64.powi(places)).round() / 10f64.powi(places)
                };
                // A grey has no hue to speak of.
                let (chroma, hue) = if chroma < 0.00005 {
                    (0.0, 0.0)
                } else {
                    (round(chroma, 4), round(hue, 2))
                };
                format!("oklch({} {} {})", round(lightness, 3), chroma, hue)
            }
        }
    }

    /// Lightness from 0 to 100 and the two colour axes of CIE Lab.
    #[must_use]
    #[expect(
        clippy::many_single_char_names,
        reason = "colour channels go by their letters"
    )]
    pub fn to_lab(self) -> (f64, f64, f64) {
        let [r, g, b] = [self.r, self.g, self.b].map(|part| to_linear(f64::from(part) / 255.0));
        // To XYZ under D65, then adapted to the D50 white Lab is measured against,
        // with the matrices CSS Color gives.
        let rgb = [r, g, b];
        let xyz = [
            weighted(
                [
                    506_752.0 / 1_228_815.0,
                    87881.0 / 245_763.0,
                    12673.0 / 70218.0,
                ],
                rgb,
            ),
            weighted(
                [
                    87098.0 / 409_605.0,
                    175_762.0 / 245_763.0,
                    12673.0 / 175_545.0,
                ],
                rgb,
            ),
            weighted(
                [
                    7918.0 / 409_605.0,
                    87881.0 / 737_289.0,
                    1_001_167.0 / 1_053_270.0,
                ],
                rgb,
            ),
        ];
        let adapted = [
            weighted(
                [
                    1.047_929_792_544_996_9,
                    0.022_946_870_601_609_652,
                    -0.050_192_266_289_205_24,
                ],
                xyz,
            ),
            weighted(
                [
                    0.029_627_808_770_055_99,
                    0.990_434_426_753_879_9,
                    -0.017_073_799_063_418_826,
                ],
                xyz,
            ),
            weighted(
                [
                    -0.009_243_040_646_204_504,
                    0.015_055_191_490_298_152,
                    0.751_874_281_428_137_1,
                ],
                xyz,
            ),
        ];
        let white = [0.3457 / 0.3585, 1.0, (1.0 - 0.3457 - 0.3585) / 0.3585];
        let curve = |t: f64| {
            if t > 216.0 / 24389.0 {
                t.cbrt()
            } else {
                f64::mul_add(24389.0 / 27.0, t, 16.0) / 116.0
            }
        };
        let [white_x, white_y, white_z] = white;
        let [x, y, z] = adapted;
        let (fx, fy, fz) = (curve(x / white_x), curve(y / white_y), curve(z / white_z));
        (
            116.0f64.mul_add(fy, -16.0),
            500.0 * (fx - fy),
            200.0 * (fy - fz),
        )
    }

    /// Lightness from 0 to 1, colourfulness, and hue in degrees, in OKLCH.
    #[must_use]
    pub fn to_oklch(self) -> (f64, f64, f64) {
        let [r, g, b] = [self.r, self.g, self.b].map(|part| to_linear(f64::from(part) / 255.0));
        // To the cone responses, their cube roots, and from those to Oklab.
        let rgb = [r, g, b];
        let cones = [
            weighted([0.412_221_470_8, 0.536_332_536_3, 0.051_445_992_9], rgb).cbrt(),
            weighted([0.211_903_498_2, 0.680_699_545_1, 0.107_396_956_6], rgb).cbrt(),
            weighted([0.088_302_461_9, 0.281_718_837_6, 0.629_978_700_5], rgb).cbrt(),
        ];
        let lightness = weighted([0.210_454_255_3, 0.793_617_785_0, -0.004_072_046_8], cones);
        let a = weighted([1.977_998_495_1, -2.428_592_205_0, 0.450_593_709_9], cones);
        let b = weighted([0.025_904_037_1, 0.782_771_766_2, -0.808_675_766_0], cones);
        (
            lightness,
            a.hypot(b),
            b.atan2(a).to_degrees().rem_euclid(360.0),
        )
    }

    /// The colour written with how see-through it is, from 0 to 1. A colour that
    /// is not see-through at all is written without it.
    #[must_use]
    pub fn write_alpha(self, notation: Notation, alpha: f64) -> String {
        let alpha = alpha.clamp(0.0, 1.0);
        let plain = self.write(notation);
        if alpha >= 1.0 {
            return plain;
        }
        match notation {
            Notation::Hex => format!("{plain}{:02x}", (alpha * 255.0).round().cast::<u8>()),
            _ => format!(
                "{} / {})",
                plain.trim_end_matches(')'),
                (alpha * 1000.0).round() / 1000.0
            ),
        }
    }

    /// Text read as a colour and how see-through it is: `rgb(255 0 0 / 0.5)`,
    /// `#00000080`. Without one, it is not see-through.
    #[must_use]
    pub fn read_alpha(text: &str) -> Option<(Self, f64)> {
        let text = text.trim();
        if let Some(hex) = text.strip_prefix('#').filter(|hex| hex.len() == 8) {
            let (colour, alpha) = hex.split_at_checked(6)?;
            let alpha = f64::from(u8::from_str_radix(alpha, 16).ok()?) / 255.0;
            return Some((Self::from_hex(colour)?, alpha));
        }
        let alpha = match text.rsplit_once('/') {
            Some((_, alpha)) => {
                let alpha = alpha.trim().trim_end_matches(')').trim();
                match alpha.strip_suffix('%') {
                    Some(percent) => percent.parse::<f64>().ok()? / 100.0,
                    None => alpha.parse().ok()?,
                }
            }
            None => 1.0,
        };
        let colour = match text.split_once('/') {
            Some((colour, _)) => format!("{})", colour.trim()),
            None => text.to_string(),
        };
        Some((Self::read(&colour)?, alpha.clamp(0.0, 1.0)))
    }

    /// Text read as a colour in any notation: hex, `rgb(..)`, `hsl(..)` or a name.
    #[must_use]
    pub fn read(text: &str) -> Option<Self> {
        let text = text.trim();
        let numbers = |body: &str| -> Vec<f64> {
            body.split(|ch: char| ch == ',' || ch == '/' || ch.is_whitespace())
                .filter(|piece| !piece.is_empty())
                .filter_map(|piece| piece.trim_end_matches('%').parse().ok())
                .collect()
        };
        let inside = |name: &str| {
            text.strip_prefix(name)
                .and_then(|rest| rest.trim().strip_prefix('('))
                .and_then(|rest| rest.strip_suffix(')'))
        };
        if let Some(body) = inside("rgb").or_else(|| inside("rgba")) {
            let [r, g, b, ..] = numbers(body)[..] else {
                return None;
            };
            let channel = |value: f64| value.clamp(0.0, 255.0).cast::<u8>();
            return Some(Self::new(channel(r), channel(g), channel(b)));
        }
        if let Some(body) = inside("hsl").or_else(|| inside("hsla")) {
            let [hue, saturation, lightness, ..] = numbers(body)[..] else {
                return None;
            };
            let hsl = Hsl {
                h: hue.rem_euclid(360.0),
                s: (saturation / 100.0).clamp(0.0, 1.0),
                l: (lightness / 100.0).clamp(0.0, 1.0),
            };
            return Some(hsl.to_rgb());
        }
        Self::parse(text)
    }
}

/// A colour in Display P3, the wider range of colour newer screens show. Each part
/// runs from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct P3 {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

fn to_linear(part: f64) -> f64 {
    if part <= 0.04045 {
        part / 12.92
    } else {
        ((part + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(part: f64) -> f64 {
    if part <= 0.003_130_8 {
        part * 12.92
    } else {
        1.055f64.mul_add(part.powf(1.0 / 2.4), -0.055)
    }
}

impl P3 {
    /// The same colour as an ordinary one, however far outside the ordinary range.
    fn in_srgb(self) -> [f64; 3] {
        let (r, g, b) = (to_linear(self.r), to_linear(self.g), to_linear(self.b));
        [
            0.224_940_4_f64.mul_add(-g, 1.224_940_1 * r),
            1.042_057_1_f64.mul_add(g, -0.042_056_9 * r),
            1.098_273_5_f64.mul_add(b, 0.078_636_1_f64.mul_add(-g, -0.019_637_6 * r)),
        ]
    }

    #[must_use]
    pub fn from_rgb(color: Rgb) -> Self {
        let rgb = [color.r, color.g, color.b].map(|part| to_linear(f64::from(part) / 255.0));
        Self {
            r: from_linear(weighted([0.822_462_1, 0.177_538_0, 0.0], rgb)),
            g: from_linear(weighted([0.033_194_1, 0.966_805_8, 0.0], rgb)),
            b: from_linear(weighted([0.017_082_7, 0.072_397_4, 0.910_519_9], rgb)),
        }
    }

    /// Whether an ordinary screen can show this colour as it is.
    #[must_use]
    pub fn is_in_srgb(self) -> bool {
        self.in_srgb()
            .iter()
            .all(|part| (-0.0005..=1.0005).contains(part))
    }

    /// The nearest colour an ordinary screen can show.
    #[must_use]
    pub fn to_rgb(self) -> Rgb {
        let [r, g, b] = self.in_srgb().map(|part| {
            (from_linear(part.clamp(0.0, 1.0)) * 255.0)
                .round()
                .cast::<u8>()
        });
        Rgb::new(r, g, b)
    }

    /// Reads `color(display-p3 1 0.5 0)`.
    #[must_use]
    pub fn read(text: &str) -> Option<Self> {
        let body = text
            .trim()
            .strip_prefix("color(")?
            .strip_suffix(')')?
            .trim()
            .strip_prefix("display-p3")?;
        let parts: Vec<f64> = body
            .split_whitespace()
            .filter_map(|part| part.parse().ok())
            .collect();
        match parts[..] {
            [r, g, b, ..] => Some(Self { r, g, b }),
            _ => None,
        }
    }

    #[must_use]
    pub fn write(self) -> String {
        let round = |part: f64| (part * 10_000.0).round() / 10_000.0;
        format!(
            "color(display-p3 {} {} {})",
            round(self.r),
            round(self.g),
            round(self.b)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_text_parses_in_short_and_long_form() {
        assert_eq!(Rgb::from_hex("#ff8800"), Some(Rgb::new(255, 136, 0)));
        assert_eq!(Rgb::from_hex("f80"), Some(Rgb::new(255, 136, 0)));
        assert_eq!(Rgb::from_hex("#ff88"), None);
        assert_eq!(Rgb::from_hex("#gggggg"), None);
        assert_eq!(Rgb::new(255, 136, 0).to_hex(), "#ff8800");
    }

    #[test]
    fn primary_colours_have_their_textbook_hues() {
        assert_eq!(
            Rgb::new(255, 0, 0).to_hsv(),
            Hsv {
                h: 0.0,
                s: 1.0,
                v: 1.0
            }
        );
        assert_eq!(Rgb::new(0, 255, 0).to_hsv().h, 120.0);
        assert_eq!(
            Rgb::new(0, 0, 255).to_hsl(),
            Hsl {
                h: 240.0,
                s: 1.0,
                l: 0.5
            }
        );
        assert_eq!(Rgb::new(128, 128, 128).to_hsv().s, 0.0);
    }

    #[test]
    fn every_model_converts_back_to_the_same_colour() {
        for rgb in [
            Rgb::new(12, 200, 99),
            Rgb::new(250, 3, 180),
            Rgb::new(0, 0, 0),
            Rgb::new(77, 77, 200),
        ] {
            assert_eq!(rgb.to_hsv().to_rgb(), rgb);
            assert_eq!(rgb.to_hsl().to_rgb(), rgb);
        }
    }

    #[test]
    fn contrast_runs_from_one_to_twenty_one() {
        let (black, white) = (Rgb::new(0, 0, 0), Rgb::new(255, 255, 255));
        assert!((black.contrast(white) - 21.0).abs() < 1e-9);
        assert_eq!(white.contrast(white), 1.0);
        assert_eq!(black.contrast(white), white.contrast(black));
        assert!(Rgb::new(118, 118, 118).contrast(white) >= 4.5);
    }

    #[test]
    fn colours_mix_and_basic_names_are_understood() {
        assert_eq!(
            Rgb::new(0, 0, 0).mix(Rgb::new(255, 255, 255), 0.5),
            Rgb::new(128, 128, 128)
        );
        assert_eq!(
            Rgb::new(10, 20, 30).mix(Rgb::new(200, 0, 0), 0.0),
            Rgb::new(10, 20, 30)
        );
        assert_eq!(Rgb::parse("Teal"), Some(Rgb::new(0, 128, 128)));
        assert_eq!(Rgb::parse("#fff"), Some(Rgb::new(255, 255, 255)));
        assert_eq!(Rgb::parse("blurple"), None);
    }

    #[test]
    fn colours_are_read_and_written_in_each_notation() {
        let blue = Rgb::new(59, 111, 224);
        assert_eq!(blue.write(Notation::Rgb), "rgb(59 111 224)");
        assert_eq!(Rgb::read("rgb(59 111 224)"), Some(blue));
        assert_eq!(Rgb::read(" hsl(0, 100%, 50%) "), Some(Rgb::new(255, 0, 0)));
        assert_eq!(
            Rgb::read(&Rgb::new(255, 0, 0).write(Notation::Hsl)),
            Some(Rgb::new(255, 0, 0))
        );
        assert_eq!(Rgb::read("teal"), Some(Rgb::new(0, 128, 128)));
        assert_eq!(Rgb::read("rgb(1, 2)"), None);
    }

    #[test]
    fn distance_is_nothing_for_the_same_colour_and_most_for_black_and_white() {
        let (black, white) = (Rgb::new(0, 0, 0), Rgb::new(255, 255, 255));
        assert_eq!(black.distance(black), 0.0);
        assert!(black.distance(white) > black.distance(Rgb::new(128, 128, 128)));
        assert!(
            Rgb::new(0, 200, 0).distance(black) > Rgb::new(0, 0, 200).distance(black),
            "green counts for more than blue"
        );
    }

    #[test]
    fn a_wide_colour_knows_whether_an_ordinary_screen_can_show_it() {
        let vivid = P3::read("color(display-p3 1 0 0)").unwrap();
        assert!(!vivid.is_in_srgb(), "redder than an ordinary screen goes");
        assert_eq!(
            vivid.to_rgb(),
            Rgb::new(255, 0, 0),
            "brought back to the nearest it can show"
        );
        let grey = P3::from_rgb(Rgb::new(128, 128, 128));
        assert!(grey.is_in_srgb());
        assert_eq!(grey.to_rgb(), Rgb::new(128, 128, 128));
        assert_eq!(
            P3::from_rgb(Rgb::new(59, 111, 224)).to_rgb(),
            Rgb::new(59, 111, 224),
            "there and back"
        );
        assert_eq!(vivid.write(), "color(display-p3 1 0 0)");
    }
}
