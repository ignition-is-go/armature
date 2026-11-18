//! The path a pointer takes from a trigger to the popup it opened.
//!
//! While the pointer stays inside the triangle from where it left the trigger to the
//! popup's near edge, and keeps getting closer, the popup stays open, even though the
//! pointer is over neither.

use crate::geometry::{Point, Rect};

/// Whether the popup lies sideways from the trigger, or above or below it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

/// How long the pointer may sit still inside the triangle before it stops counting.
pub const REST_MS: u64 = 300;
/// How far past the popup's corners the triangle reaches.
const MARGIN: f64 = 8.0;

#[derive(Clone, Copy, Debug)]
pub struct SafeArea {
    apex: Point,
    popup: Rect,
    axis: Axis,
    last: Point,
    /// When the pointer last moved towards the popup.
    moved_ms: u64,
}

impl SafeArea {
    /// Starts when the pointer leaves the trigger at `point`, heading for `popup`.
    #[must_use]
    pub const fn start(point: Point, popup: Rect, axis: Axis, now_ms: u64) -> Self {
        Self {
            apex: point,
            popup,
            axis,
            last: point,
            moved_ms: now_ms,
        }
    }

    fn near(&self) -> f64 {
        match self.axis {
            Axis::X if self.popup.x >= self.apex.x => self.popup.x,
            Axis::X => self.popup.x + self.popup.width,
            Axis::Y if self.popup.y >= self.apex.y => self.popup.y,
            Axis::Y => self.popup.y + self.popup.height,
        }
    }

    /// Whether the pointer at `point` is still on its way to the popup.
    pub fn contains(&mut self, point: Point, now_ms: u64) -> bool {
        let (near, popup) = (self.near(), self.popup);
        let (b, c, along, was) = match self.axis {
            Axis::X => (
                Point::new(near, popup.y - MARGIN),
                Point::new(near, popup.y + popup.height + MARGIN),
                point.x,
                self.last.x,
            ),
            Axis::Y => (
                Point::new(popup.x - MARGIN, near),
                Point::new(popup.x + popup.width + MARGIN, near),
                point.y,
                self.last.y,
            ),
        };
        let progressing = (near - along).abs() <= (near - was).abs() + 1.0;
        let moved = point != self.last;
        self.last = point;
        if moved && progressing {
            self.moved_ms = now_ms;
        }
        in_triangle(point, self.apex, b, c)
            && progressing
            && now_ms.saturating_sub(self.moved_ms) < REST_MS
    }
}

fn in_triangle(p: Point, a: Point, b: Point, c: Point) -> bool {
    let sign = |p1: Point, p2: Point, p3: Point| {
        (p2.x - p3.x).mul_add(-(p1.y - p3.y), (p1.x - p3.x) * (p2.y - p3.y))
    };
    let (d1, d2, d3) = (sign(p, a, b), sign(p, b, c), sign(p, c, a));
    let negative = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let positive = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(negative && positive)
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

    fn area() -> SafeArea {
        // A submenu to the right of the pointer.
        SafeArea::start(
            Point::new(100.0, 100.0),
            Rect::new(200.0, 50.0, 150.0, 200.0),
            Axis::X,
            0,
        )
    }

    #[test]
    fn a_pointer_heading_for_the_popup_is_inside_even_off_the_straight_line() {
        let mut area = area();
        assert!(area.contains(Point::new(140.0, 120.0), 10));
        assert!(
            area.contains(Point::new(180.0, 200.0), 20),
            "cutting the corner towards the bottom"
        );
    }

    #[test]
    fn turning_back_or_leaving_the_triangle_is_outside() {
        let mut back = area();
        assert!(back.contains(Point::new(150.0, 100.0), 10));
        assert!(
            !back.contains(Point::new(120.0, 100.0), 20),
            "moving away from the popup"
        );
        assert!(
            !area().contains(Point::new(110.0, 300.0), 10),
            "well below the triangle"
        );
    }

    #[test]
    fn resting_inside_the_triangle_stops_counting() {
        let mut area = area();
        assert!(area.contains(Point::new(150.0, 100.0), 10));
        assert!(area.contains(Point::new(150.0, 100.0), 200));
        assert!(
            !area.contains(Point::new(150.0, 100.0), 400),
            "it has sat still too long"
        );
    }
}
