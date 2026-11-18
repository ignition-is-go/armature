//! Choosing a value along two axes at once, such as saturation and brightness on a
//! colour pad.

use crate::geometry::{Point, Rect};
use crate::keys::{Dir, Key, Modifiers};
use crate::number::NumberRange;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    /// Grows to the right, or to the left in a right-to-left layout.
    pub x: NumberRange,
    /// Grows upwards.
    pub y: NumberRange,
}

impl Area {
    /// The value under the pointer on a pad occupying `pad`. Positions outside it are
    /// held at the nearest edge.
    #[must_use]
    pub fn value_at(&self, pad: Rect, at: Point, dir: Dir) -> (f64, f64) {
        let across = (at.x - pad.x) / pad.width.max(f64::MIN_POSITIVE);
        let down = (at.y - pad.y) / pad.height.max(f64::MIN_POSITIVE);
        let across = if dir == Dir::Rtl {
            1.0 - across
        } else {
            across
        };
        (self.x.at_fraction(across), self.y.at_fraction(1.0 - down))
    }

    /// Where on the pad a value sits, as fractions from its left and top edges.
    #[must_use]
    pub fn position_of(&self, value: (f64, f64), dir: Dir) -> (f64, f64) {
        let across = self.x.fraction(value.0);
        (
            if dir == Dir::Rtl {
                1.0 - across
            } else {
                across
            },
            1.0 - self.y.fraction(value.1),
        )
    }

    /// The value after an arrow key. Shift moves ten steps.
    #[must_use]
    pub fn nudge(
        &self,
        value: (f64, f64),
        key: Key,
        mods: Modifiers,
        dir: Dir,
    ) -> Option<(f64, f64)> {
        let steps = if mods.shift { 10.0 } else { 1.0 };
        let forward = if dir == Dir::Rtl { -steps } else { steps };
        Some(match key {
            Key::ArrowRight => (self.x.nudge(value.0, forward), value.1),
            Key::ArrowLeft => (self.x.nudge(value.0, -forward), value.1),
            Key::ArrowUp => (value.0, self.y.nudge(value.1, steps)),
            Key::ArrowDown => (value.0, self.y.nudge(value.1, -steps)),
            _ => return None,
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::too_many_lines,
    reason = "a test fails by panicking, and its arithmetic is on values it wrote itself"
)]
mod tests {
    use super::*;

    const PAD: Rect = Rect {
        x: 100.0,
        y: 50.0,
        width: 200.0,
        height: 100.0,
    };

    fn area() -> Area {
        Area {
            x: NumberRange::new(0.0, 100.0, 1.0),
            y: NumberRange::new(0.0, 100.0, 1.0),
        }
    }

    #[test]
    fn the_top_right_corner_is_the_maximum_of_both_axes() {
        assert_eq!(
            area().value_at(PAD, Point::new(300.0, 50.0), Dir::Ltr),
            (100.0, 100.0)
        );
        assert_eq!(
            area().value_at(PAD, Point::new(150.0, 125.0), Dir::Ltr),
            (25.0, 25.0)
        );
    }

    #[test]
    fn positions_off_the_pad_are_held_at_its_edges() {
        assert_eq!(
            area().value_at(PAD, Point::new(-500.0, 900.0), Dir::Ltr),
            (0.0, 0.0)
        );
    }

    #[test]
    fn right_to_left_mirrors_the_horizontal_axis() {
        assert_eq!(
            area().value_at(PAD, Point::new(150.0, 100.0), Dir::Rtl).0,
            75.0
        );
        assert_eq!(area().position_of((75.0, 100.0), Dir::Rtl), (0.25, 0.0));
    }

    #[test]
    fn a_value_and_its_position_agree() {
        let value = area().value_at(PAD, Point::new(180.0, 80.0), Dir::Ltr);
        let (across, down) = area().position_of(value, Dir::Ltr);
        assert!((across - 0.4).abs() < 1e-9 && (down - 0.3).abs() < 1e-9);
    }

    #[test]
    fn arrows_step_one_axis_and_shift_steps_ten() {
        let shift = Modifiers {
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            area().nudge((50.0, 50.0), Key::ArrowUp, Modifiers::default(), Dir::Ltr),
            Some((50.0, 51.0))
        );
        assert_eq!(
            area().nudge((50.0, 50.0), Key::ArrowLeft, shift, Dir::Ltr),
            Some((40.0, 50.0))
        );
        assert_eq!(
            area().nudge((50.0, 50.0), Key::Enter, shift, Dir::Ltr),
            None
        );
    }
}
