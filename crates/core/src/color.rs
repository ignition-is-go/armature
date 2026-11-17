//! Colours as values: conversion between the models a picker works in, hex text, and
//! contrast.

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
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Rgb { r, g, b }
    }

    /// Parses `#rgb` or `#rrggbb`, with or without the `#`.
    pub fn from_hex(text: &str) -> Option<Rgb> {
        let hex = text.trim().trim_start_matches('#');
        if !hex.is_ascii() {
            return None;
        }
        let channel = |digits: &str| u8::from_str_radix(digits, 16).ok();
        match hex.len() {
            3 => {
                let double = |i: usize| channel(&hex[i..i + 1].repeat(2));
                Some(Rgb::new(double(0)?, double(1)?, double(2)?))
            }
            6 => Some(Rgb::new(
                channel(&hex[0..2])?,
                channel(&hex[2..4])?,
                channel(&hex[4..6])?,
            )),
            _ => None,
        }
    }

    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    fn unit(self) -> (f64, f64, f64) {
        (
            self.r as f64 / 255.0,
            self.g as f64 / 255.0,
            self.b as f64 / 255.0,
        )
    }

    fn from_unit(r: f64, g: f64, b: f64) -> Rgb {
        let byte = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        Rgb::new(byte(r), byte(g), byte(b))
    }

    fn hue_and_range(self) -> (f64, f64, f64) {
        let (r, g, b) = self.unit();
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let range = max - min;
        let hue = if range == 0.0 {
            0.0
        } else if max == r {
            60.0 * ((g - b) / range).rem_euclid(6.0)
        } else if max == g {
            60.0 * ((b - r) / range + 2.0)
        } else {
            60.0 * ((r - g) / range + 4.0)
        };
        (hue, max, min)
    }

    pub fn to_hsv(self) -> Hsv {
        let (h, max, min) = self.hue_and_range();
        Hsv {
            h,
            s: if max == 0.0 { 0.0 } else { (max - min) / max },
            v: max,
        }
    }

    pub fn to_hsl(self) -> Hsl {
        let (h, max, min) = self.hue_and_range();
        let l = (max + min) / 2.0;
        let s = if max == min {
            0.0
        } else {
            (max - min) / (1.0 - (2.0 * l - 1.0).abs())
        };
        Hsl { h, s, l }
    }

    /// Relative luminance, from 0 for black to 1 for white.
    pub fn luminance(self) -> f64 {
        let linear = |c: f64| {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let (r, g, b) = self.unit();
        0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
    }

    /// Contrast ratio against another colour, from 1 to 21. Body text needs 4.5.
    pub fn contrast(self, other: Rgb) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
}

fn from_hue(h: f64, chroma: f64, floor: f64) -> Rgb {
    let sector = h.rem_euclid(360.0) / 60.0;
    let x = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (r, g, b) = match sector as u8 {
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
    pub fn to_rgb(self) -> Rgb {
        let chroma = self.v * self.s;
        from_hue(self.h, chroma, self.v - chroma)
    }
}

impl Hsl {
    pub fn to_rgb(self) -> Rgb {
        let chroma = (1.0 - (2.0 * self.l - 1.0).abs()) * self.s;
        from_hue(self.h, chroma, self.l - chroma / 2.0)
    }
}

impl Rgb {
    /// This colour with `amount` of `other` mixed in, from 0 for none to 1 for all.
    pub fn mix(self, other: Rgb, amount: f64) -> Rgb {
        let amount = amount.clamp(0.0, 1.0);
        let blend = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * amount).round() as u8;
        Rgb::new(
            blend(self.r, other.r),
            blend(self.g, other.g),
            blend(self.b, other.b),
        )
    }

    /// The colour a basic colour name stands for, in any case.
    pub fn named(name: &str) -> Option<Rgb> {
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
        Rgb::from_hex(hex)
    }

    /// Text read as a colour: a hex value or a basic colour name.
    pub fn parse(text: &str) -> Option<Rgb> {
        Rgb::from_hex(text).or_else(|| Rgb::named(text))
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
}

impl Rgb {
    /// How different two colours look, from 0 for the same to about 765 for black
    /// against white. Red, green and blue are weighted by how strongly the eye
    /// answers to each.
    pub fn distance(self, other: Rgb) -> f64 {
        let mean = (self.r as f64 + other.r as f64) / 2.0;
        let (dr, dg, db) = (
            self.r as f64 - other.r as f64,
            self.g as f64 - other.g as f64,
            self.b as f64 - other.b as f64,
        );
        ((2.0 + mean / 256.0) * dr * dr + 4.0 * dg * dg + (2.0 + (255.0 - mean) / 256.0) * db * db)
            .sqrt()
    }

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

    /// Lightness from 0 to 1, colourfulness, and hue in degrees, in OKLCH.
    pub fn to_oklch(self) -> (f64, f64, f64) {
        let [r, g, b] = [self.r, self.g, self.b].map(|part| to_linear(part as f64 / 255.0));
        let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
        let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
        let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
        let lightness = 0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s;
        let a = 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s;
        let b = 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s;
        (
            lightness,
            a.hypot(b),
            b.atan2(a).to_degrees().rem_euclid(360.0),
        )
    }

    /// The colour written with how see-through it is, from 0 to 1. A colour that
    /// is not see-through at all is written without it.
    pub fn write_alpha(self, notation: Notation, alpha: f64) -> String {
        let alpha = alpha.clamp(0.0, 1.0);
        let plain = self.write(notation);
        if alpha >= 1.0 {
            return plain;
        }
        match notation {
            Notation::Hex => format!("{plain}{:02x}", (alpha * 255.0).round() as u8),
            _ => format!(
                "{} / {})",
                plain.trim_end_matches(')'),
                (alpha * 1000.0).round() / 1000.0
            ),
        }
    }

    /// Text read as a colour and how see-through it is: `rgb(255 0 0 / 0.5)`,
    /// `#00000080`. Without one, it is not see-through.
    pub fn read_alpha(text: &str) -> Option<(Rgb, f64)> {
        let text = text.trim();
        if let Some(hex) = text.strip_prefix('#').filter(|hex| hex.len() == 8) {
            let alpha = u8::from_str_radix(&hex[6..], 16).ok()? as f64 / 255.0;
            return Some((Rgb::from_hex(&text[..7])?, alpha));
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
        Some((Rgb::read(&colour)?, alpha.clamp(0.0, 1.0)))
    }

    /// Text read as a colour in any notation: hex, `rgb(..)`, `hsl(..)` or a name.
    pub fn read(text: &str) -> Option<Rgb> {
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
            let n = numbers(body);
            return (n.len() >= 3).then(|| {
                Rgb::new(
                    n[0].clamp(0.0, 255.0) as u8,
                    n[1].clamp(0.0, 255.0) as u8,
                    n[2].clamp(0.0, 255.0) as u8,
                )
            });
        }
        if let Some(body) = inside("hsl").or_else(|| inside("hsla")) {
            let n = numbers(body);
            return (n.len() >= 3).then(|| {
                Hsl {
                    h: n[0].rem_euclid(360.0),
                    s: (n[1] / 100.0).clamp(0.0, 1.0),
                    l: (n[2] / 100.0).clamp(0.0, 1.0),
                }
                .to_rgb()
            });
        }
        Rgb::parse(text)
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
    if part <= 0.0031308 {
        part * 12.92
    } else {
        1.055 * part.powf(1.0 / 2.4) - 0.055
    }
}

impl P3 {
    /// The same colour as an ordinary one, however far outside the ordinary range.
    fn in_srgb(self) -> [f64; 3] {
        let (r, g, b) = (to_linear(self.r), to_linear(self.g), to_linear(self.b));
        [
            1.2249401 * r - 0.2249404 * g,
            -0.0420569 * r + 1.0420571 * g,
            -0.0196376 * r - 0.0786361 * g + 1.0982735 * b,
        ]
    }

    pub fn from_rgb(color: Rgb) -> P3 {
        let (r, g, b) = (
            to_linear(color.r as f64 / 255.0),
            to_linear(color.g as f64 / 255.0),
            to_linear(color.b as f64 / 255.0),
        );
        P3 {
            r: from_linear(0.8224621 * r + 0.1775380 * g),
            g: from_linear(0.0331941 * r + 0.9668058 * g),
            b: from_linear(0.0170827 * r + 0.0723974 * g + 0.9105199 * b),
        }
    }

    /// Whether an ordinary screen can show this colour as it is.
    pub fn is_in_srgb(self) -> bool {
        self.in_srgb()
            .iter()
            .all(|part| (-0.0005..=1.0005).contains(part))
    }

    /// The nearest colour an ordinary screen can show.
    pub fn to_rgb(self) -> Rgb {
        let [r, g, b] = self
            .in_srgb()
            .map(|part| (from_linear(part.clamp(0.0, 1.0)) * 255.0).round() as u8);
        Rgb::new(r, g, b)
    }

    /// Reads `color(display-p3 1 0.5 0)`.
    pub fn read(text: &str) -> Option<P3> {
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
        (parts.len() >= 3).then(|| P3 {
            r: parts[0],
            g: parts[1],
            b: parts[2],
        })
    }

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
