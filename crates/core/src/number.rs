//! A numeric range with a step, for sliders, steppers and anything else that picks a
//! number.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumberRange {
    pub min: f64,
    pub max: f64,
    pub step: f64,
    /// How positions along a track map to values.
    pub scale: Scale,
}

/// How evenly a range is spread along a track.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scale {
    #[default]
    Linear,
    /// Each step along the track multiplies the value, as for a frequency or a zoom
    /// level. The range must be above zero.
    Log,
}

impl Scale {
    fn to(self, value: f64) -> f64 {
        if self == Scale::Log {
            value.ln()
        } else {
            value
        }
    }

    fn from(self, position: f64) -> f64 {
        if self == Scale::Log {
            position.exp()
        } else {
            position
        }
    }
}

impl NumberRange {
    pub fn new(min: f64, max: f64, step: f64) -> Self {
        NumberRange {
            min,
            max,
            step,
            scale: Scale::Linear,
        }
    }

    pub fn log(mut self) -> Self {
        self.scale = Scale::Log;
        self
    }

    pub fn clamp(&self, value: f64) -> f64 {
        value.clamp(self.min, self.max)
    }

    /// The nearest step to `value`, counted from `min` and kept in range.
    pub fn snap(&self, value: f64) -> f64 {
        if self.step <= 0.0 {
            return self.clamp(value);
        }
        let steps = ((value - self.min) / self.step).round();
        // Rounding away the float error that multiplying by a decimal step leaves.
        let snapped = self.min + steps * self.step;
        self.clamp((snapped * 1e9).round() / 1e9)
    }

    /// Where `value` sits between `min` and `max`, from 0 to 1.
    pub fn fraction(&self, value: f64) -> f64 {
        if self.max <= self.min {
            return 0.0;
        }
        let low = self.scale.to(self.min);
        let fraction = (self.scale.to(self.clamp(value)) - low) / (self.scale.to(self.max) - low);
        if fraction.is_finite() {
            fraction.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// The stepped value at `fraction` of the way from `min` to `max`.
    pub fn at_fraction(&self, fraction: f64) -> f64 {
        let low = self.scale.to(self.min);
        self.snap(
            self.scale
                .from(low + fraction.clamp(0.0, 1.0) * (self.scale.to(self.max) - low)),
        )
    }

    /// `value` moved by a number of steps, which may be negative.
    pub fn nudge(&self, value: f64, steps: f64) -> f64 {
        self.snap(value + steps * self.step)
    }
}

/// Reads a number the way people type one: surrounding spaces, thousands separators
/// and a trailing percent sign are accepted.
pub fn parse_number(text: &str) -> Option<f64> {
    let cleaned: String = text
        .trim()
        .trim_end_matches('%')
        .chars()
        .filter(|c| *c != ',' && *c != '_')
        .collect();
    cleaned
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

/// Formats with a fixed number of decimals and thousands separators.
pub fn format_number(value: f64, decimals: usize) -> String {
    let fixed = format!("{:.*}", decimals, value.abs());
    let (whole, fraction) = fixed.split_once('.').unwrap_or((&fixed, ""));
    let mut grouped = String::new();
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    let sign = if value < 0.0 && fixed.chars().any(|c| c.is_ascii_digit() && c != '0') {
        "-"
    } else {
        ""
    };
    if fraction.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped}.{fraction}")
    }
}

/// How many decimal places a step calls for: none for 1, two for 0.25.
pub fn decimals_of(step: f64) -> usize {
    let text = format!("{step}");
    text.split_once('.')
        .map_or(0, |(_, decimals)| decimals.trim_end_matches('0').len())
}

/// A number written for reading, with thousands separated: `1,234.50`.
pub fn format_grouped(value: f64, decimals: usize) -> String {
    let text = format!("{:.*}", decimals, value.abs());
    let (whole, fraction) = text.split_once('.').unwrap_or((&text, ""));
    let mut grouped = String::new();
    for (at, digit) in whole.chars().enumerate() {
        if at > 0 && (whole.len() - at) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    let sign = if value < 0.0 && text.chars().any(|ch| ch.is_ascii_digit() && ch != '0') {
        "-"
    } else {
        ""
    };
    if fraction.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped}.{fraction}")
    }
}

/// A size in bytes written for reading: `512 B`, `1.5 KB`, `3.2 MB`.
pub fn format_file_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!(
            "{} {}",
            format_number(size, if size < 10.0 { 1 } else { 0 }),
            UNITS[unit]
        )
    }
}

/// The marks a language groups thousands and sets off decimals with.
fn separators(locale: &str) -> (&'static str, &'static str) {
    let tag = locale.to_lowercase();
    let language = tag.split(['-', '_']).next().unwrap_or_default();
    match (language, tag.as_str()) {
        (_, "de-ch" | "fr-ch" | "it-ch") => ("\u{2019}", "."),
        ("de" | "es" | "it" | "nl" | "pt" | "tr" | "id" | "da" | "el", _) => (".", ","),
        ("fr" | "ru" | "pl" | "sv" | "nb" | "fi" | "cs" | "uk", _) => ("\u{a0}", ","),
        _ => (",", "."),
    }
}

/// A number written the way a language writes it: `1,234.5` in English, `1.234,5`
/// in German, with a space in French.
pub fn format_localized(value: f64, decimals: usize, locale: &str) -> String {
    let (group, point) = separators(locale);
    format_grouped(value, decimals)
        .chars()
        .map(|ch| match ch {
            ',' => group.to_string(),
            '.' => point.to_string(),
            other => other.to_string(),
        })
        .collect()
}

/// Reads a number written the way a language writes it.
pub fn parse_localized(text: &str, locale: &str) -> Option<f64> {
    let (group, point) = separators(locale);
    let plain: String = text
        .trim()
        .replace(group, "")
        .replace(' ', "")
        .replace(point, ".");
    plain.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_snap_to_the_nearest_step_from_the_minimum() {
        let range = NumberRange::new(1.0, 10.0, 2.0);
        assert_eq!(range.snap(4.2), 5.0);
        assert_eq!(range.snap(100.0), 10.0);
        assert_eq!(range.snap(-3.0), 1.0);
    }

    #[test]
    fn decimal_steps_do_not_accumulate_float_error() {
        let range = NumberRange::new(0.0, 1.0, 0.1);
        assert_eq!(range.nudge(0.2, 1.0), 0.3);
        assert_eq!(range.at_fraction(0.7), 0.7);
    }

    #[test]
    fn fractions_and_values_convert_both_ways() {
        let range = NumberRange::new(0.0, 200.0, 10.0);
        assert_eq!(range.fraction(50.0), 0.25);
        assert_eq!(range.at_fraction(0.26), 50.0);
        assert_eq!(range.at_fraction(7.0), 200.0);
    }

    #[test]
    fn typed_numbers_tolerate_separators_and_a_percent_sign() {
        assert_eq!(parse_number(" 1,234.5 "), Some(1234.5));
        assert_eq!(parse_number("45%"), Some(45.0));
        assert_eq!(parse_number("abc"), None);
        assert_eq!(parse_number("inf"), None);
    }

    #[test]
    fn formatting_groups_thousands_and_fixes_decimals() {
        assert_eq!(format_number(1234567.891, 2), "1,234,567.89");
        assert_eq!(format_number(-999.0, 0), "-999");
        assert_eq!(format_number(-0.001, 2), "0.00");
    }

    #[test]
    fn a_log_scale_puts_each_tenfold_step_the_same_distance_apart() {
        let range = NumberRange::new(1.0, 1000.0, 1.0).log();
        assert!((range.fraction(10.0) - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(range.at_fraction(2.0 / 3.0), 100.0);
        assert_eq!(NumberRange::new(0.0, 100.0, 1.0).at_fraction(0.5), 50.0);
    }

    #[test]
    fn numbers_and_sizes_are_written_for_reading() {
        assert_eq!(
            (decimals_of(1.0), decimals_of(0.25), decimals_of(0.5)),
            (0, 2, 1)
        );
        assert_eq!(format_grouped(1234567.891, 2), "1,234,567.89");
        assert_eq!(format_grouped(-999.0, 0), "-999");
        assert_eq!(format_grouped(1000.0, 0), "1,000");
        assert_eq!(format_file_size(512), "512 B");
        assert_eq!(format_file_size(1536), "1.5 KB");
        assert_eq!(format_file_size(34 * 1024 * 1024), "34 MB");
    }

    #[test]
    fn a_number_is_written_and_read_the_way_a_language_does() {
        assert_eq!(format_localized(1234.5, 1, "en-US"), "1,234.5");
        assert_eq!(format_localized(1234.5, 1, "de-DE"), "1.234,5");
        assert_eq!(format_localized(1234.5, 1, "fr-FR"), "1\u{a0}234,5");
        assert_eq!(parse_localized("1.234,5", "de"), Some(1234.5));
        assert_eq!(parse_localized("1 234,5", "fr"), Some(1234.5));
        assert_eq!(parse_localized("1,234.5", "en"), Some(1234.5));
    }
}
