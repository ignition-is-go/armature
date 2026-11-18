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
        if self == Self::Log { value.ln() } else { value }
    }

    fn from(self, position: f64) -> f64 {
        if self == Self::Log {
            position.exp()
        } else {
            position
        }
    }
}

impl NumberRange {
    #[must_use]
    pub const fn new(min: f64, max: f64, step: f64) -> Self {
        Self {
            min,
            max,
            step,
            scale: Scale::Linear,
        }
    }

    #[must_use]
    pub const fn log(mut self) -> Self {
        self.scale = Scale::Log;
        self
    }

    #[must_use]
    pub const fn clamp(&self, value: f64) -> f64 {
        value.clamp(self.min, self.max)
    }

    /// Where the steps are counted from: the least value, or zero without one.
    const fn base(&self) -> f64 {
        if self.min.is_finite() { self.min } else { 0.0 }
    }

    /// How many steps `value` is from the base, with float noise rounded away.
    fn steps_from_base(&self, value: f64) -> f64 {
        let count = (value - self.base()) / self.step;
        format!("{count:.15e}")
            .parse::<f64>()
            .map_or(count, |rounded| {
                // Fifteen significant digits, as far as a float can be trusted.
                let scale =
                    10f64.powi(14 - rounded.abs().log10().floor().clamp(-300.0, 300.0) as i32);
                if scale.is_finite() && scale > 0.0 {
                    (count * scale).round() / scale
                } else {
                    count
                }
            })
    }

    /// The last step at or below the most allowed.
    #[must_use]
    pub fn max_on_grid(&self) -> f64 {
        if !self.max.is_finite() || self.step <= 0.0 {
            return self.max;
        }
        ((self.max - self.base()) / self.step + 1e-9)
            .floor()
            .mul_add(self.step, self.base())
    }

    /// A value brought within the range and written with only the decimals its
    /// step and base have.
    fn on_grid(&self, value: f64) -> f64 {
        let decimals = decimals_of(self.step)
            .max(decimals_of(self.base()))
            .min(100);
        let kept = value.clamp(self.min, self.max_on_grid().max(self.min));
        format!("{kept:.decimals$}").parse().unwrap_or(kept)
    }

    /// The nearest step to `value`, counted from `min` and kept in range.
    #[must_use]
    pub fn snap(&self, value: f64) -> f64 {
        if self.step <= 0.0 {
            return self.clamp(value);
        }
        let steps = self.steps_from_base(value).round();
        self.on_grid(steps.mul_add(self.step, self.base()))
    }

    /// Where `value` sits between `min` and `max`, from 0 to 1.
    #[must_use]
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
    #[must_use]
    pub fn at_fraction(&self, fraction: f64) -> f64 {
        let low = self.scale.to(self.min);
        self.snap(
            self.scale.from(
                fraction
                    .clamp(0.0, 1.0)
                    .mul_add(self.scale.to(self.max) - low, low),
            ),
        )
    }

    /// `value` moved by a number of steps, which may be negative.
    #[must_use]
    pub fn nudge(&self, value: f64, steps: f64) -> f64 {
        if self.step <= 0.0 {
            return self.clamp(value + steps);
        }
        // A value between steps goes onto the grid first, so one step up from 1.5
        // is 2 and not 2.5.
        let position = self.steps_from_base(value);
        let from = if position.fract() == 0.0 {
            position
        } else if steps > 0.0 {
            position.floor()
        } else {
            position.ceil()
        };
        self.on_grid((from + steps).mul_add(self.step, self.base()))
    }
}

/// Reads a number the way people type one: surrounding spaces, thousands separators
/// and a trailing percent sign are accepted.
#[must_use]
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
#[must_use]
pub fn format_number(value: f64, decimals: usize) -> String {
    let fixed = format!("{:.*}", decimals, value.abs());
    let (whole, fraction) = fixed.split_once('.').unwrap_or((&fixed, ""));
    let mut grouped = String::new();
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len().saturating_sub(index)) % 3 == 0 {
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
#[must_use]
pub fn decimals_of(step: f64) -> usize {
    if !step.is_finite() || step.fract() == 0.0 {
        return 0;
    }
    let text = format!("{step}");
    text.split_once('.')
        .map_or(0, |(_, decimals)| decimals.trim_end_matches('0').len())
}

/// `value` brought round into the span from `min` up to, but not including, `max`,
/// as an angle is brought back within a turn.
#[must_use]
pub fn wrap(value: f64, min: f64, max: f64) -> f64 {
    let span = max - min;
    if span <= 0.0 {
        return min;
    }
    (value - min).rem_euclid(span) + min
}

/// A number written for reading, with thousands separated: `1,234.50`.
#[must_use]
pub fn format_grouped(value: f64, decimals: usize) -> String {
    let text = format!("{:.*}", decimals, value.abs());
    let (whole, fraction) = text.split_once('.').unwrap_or((&text, ""));
    let mut grouped = String::new();
    for (at, digit) in whole.chars().enumerate() {
        if at > 0 && (whole.len().saturating_sub(at)) % 3 == 0 {
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
#[must_use]
pub fn format_file_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len().saturating_sub(1) {
        size /= 1024.0;
        unit = unit.saturating_add(1);
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!(
            "{} {}",
            format_number(size, usize::from(size < 10.0)),
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
#[must_use]
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
#[must_use]
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
        assert_eq!(
            range.snap(100.0),
            9.0,
            "the last step at or below the most allowed"
        );
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
