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
}
