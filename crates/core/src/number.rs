//! A numeric range with a step, for sliders, steppers and anything else that picks a
//! number.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumberRange {
    pub min: f64,
    pub max: f64,
    pub step: f64,
}

impl NumberRange {
    pub fn new(min: f64, max: f64, step: f64) -> Self {
        NumberRange { min, max, step }
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
        ((value - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
    }

    /// The stepped value at `fraction` of the way from `min` to `max`.
    pub fn at_fraction(&self, fraction: f64) -> f64 {
        self.snap(self.min + fraction.clamp(0.0, 1.0) * (self.max - self.min))
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
}
