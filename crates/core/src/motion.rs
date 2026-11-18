//! Moving and resizing freely positioned boxes, by pointer delta or by keyboard.

use crate::geometry::{Point, Rect, Size};
use crate::keys::{Key, Modifiers};

fn snap(value: f64, grid: Option<f64>) -> f64 {
    match grid {
        Some(grid) if grid > 0.0 => (value / grid).round() * grid,
        _ => value,
    }
}

/// The movement an arrow key asks for. Shift takes the larger step.
#[must_use]
pub fn key_delta(key: Key, mods: Modifiers, step: f64, large_step: f64) -> Option<Point> {
    let by = if mods.shift { large_step } else { step };
    Some(match key {
        Key::ArrowLeft => Point::new(-by, 0.0),
        Key::ArrowRight => Point::new(by, 0.0),
        Key::ArrowUp => Point::new(0.0, -by),
        Key::ArrowDown => Point::new(0.0, by),
        _ => return None,
    })
}

/// `rect` moved by `delta`, snapped to `grid` and kept inside `bounds`.
#[must_use]
pub fn moved(rect: Rect, delta: Point, bounds: Option<Rect>, grid: Option<f64>) -> Rect {
    let mut x = snap(rect.x + delta.x, grid);
    let mut y = snap(rect.y + delta.y, grid);
    if let Some(bounds) = bounds {
        x = x.min(bounds.right() - rect.width).max(bounds.x);
        y = y.min(bounds.bottom() - rect.height).max(bounds.y);
    }
    Rect { x, y, ..rect }
}

/// The edge or corner being dragged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handle {
    /// -1 drags the left edge, 1 the right, 0 neither.
    pub horizontal: i8,
    /// -1 drags the top edge, 1 the bottom, 0 neither.
    pub vertical: i8,
}

impl Handle {
    pub const LEFT: Self = Self {
        horizontal: -1,
        vertical: 0,
    };
    pub const RIGHT: Self = Self {
        horizontal: 1,
        vertical: 0,
    };
    pub const TOP: Self = Self {
        horizontal: 0,
        vertical: -1,
    };
    pub const BOTTOM: Self = Self {
        horizontal: 0,
        vertical: 1,
    };
    pub const TOP_LEFT: Self = Self {
        horizontal: -1,
        vertical: -1,
    };
    pub const BOTTOM_RIGHT: Self = Self {
        horizontal: 1,
        vertical: 1,
    };
}

#[derive(Clone, Copy, Debug)]
pub struct ResizeLimits {
    pub min: Size,
    pub max: Option<Size>,
    pub bounds: Option<Rect>,
}

/// `rect` with `handle` dragged by `delta`. The opposite edge stays put, and the size
/// stays within the limits.
#[must_use]
pub fn resized(rect: Rect, handle: Handle, delta: Point, limits: ResizeLimits) -> Rect {
    let axis = |start: f64,
                length: f64,
                direction: i8,
                by: f64,
                min: f64,
                max: Option<f64>,
                low: Option<f64>,
                high: Option<f64>| {
        let max = max.unwrap_or(f64::INFINITY).max(min);
        match direction {
            1 => {
                let room = high.map_or(f64::INFINITY, |high| high - start);
                (start, (length + by).clamp(min, max.min(room.max(min))))
            }
            -1 => {
                let end = start + length;
                let room = low.map_or(f64::INFINITY, |low| end - low);
                let length = (length - by).clamp(min, max.min(room.max(min)));
                (end - length, length)
            }
            _ => (start, length),
        }
    };
    let bounds = limits.bounds;
    let (x, width) = axis(
        rect.x,
        rect.width,
        handle.horizontal,
        delta.x,
        limits.min.width,
        limits.max.map(|m| m.width),
        bounds.map(|b| b.x),
        bounds.map(|b| b.right()),
    );
    let (y, height) = axis(
        rect.y,
        rect.height,
        handle.vertical,
        delta.y,
        limits.min.height,
        limits.max.map(|m| m.height),
        bounds.map(|b| b.y),
        bounds.map(|b| b.bottom()),
    );
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// What to tell a screen reader after a move.
#[must_use]
pub fn move_message(rect: Rect) -> String {
    format!("Moved to {}, {}.", rect.x.round(), rect.y.round())
}

/// What to tell a screen reader after a resize.
#[must_use]
pub fn resize_message(rect: Rect) -> String {
    format!(
        "Resized to {} by {}.",
        rect.width.round(),
        rect.height.round()
    )
}

/// The cursor that shows which way an edge or corner can be dragged, by its
/// conventional name: `ew-resize`, `ns-resize`, `nwse-resize` or `nesw-resize`.
#[must_use]
pub const fn resize_cursor(handle: Handle) -> &'static str {
    match (handle.horizontal, handle.vertical) {
        (0, _) => "ns-resize",
        (_, 0) => "ew-resize",
        (h, v) if h == v => "nwse-resize",
        _ => "nesw-resize",
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

    const BOX: Rect = Rect {
        x: 50.,
        y: 50.,
        width: 100.,
        height: 80.,
    };
    const ROOM: Rect = Rect {
        x: 0.,
        y: 0.,
        width: 400.,
        height: 300.,
    };
    const LIMITS: ResizeLimits = ResizeLimits {
        min: Size {
            width: 40.,
            height: 40.,
        },
        max: None,
        bounds: Some(ROOM),
    };

    #[test]
    fn a_box_stops_at_the_edges_of_its_bounds() {
        let far = moved(BOX, Point::new(1000., -1000.), Some(ROOM), None);
        assert_eq!((far.x, far.y), (300., 0.));
    }

    #[test]
    fn movement_snaps_to_the_grid() {
        let snapped = moved(BOX, Point::new(13., 4.), None, Some(10.));
        assert_eq!((snapped.x, snapped.y), (60., 50.));
    }

    #[test]
    fn shift_takes_the_larger_keyboard_step() {
        let shift = Modifiers {
            shift: true,
            ctrl: false,
            alt: false,
        };
        assert_eq!(
            key_delta(Key::ArrowUp, Modifiers::default(), 1., 10.),
            Some(Point::new(0., -1.))
        );
        assert_eq!(
            key_delta(Key::ArrowRight, shift, 1., 10.),
            Some(Point::new(10., 0.))
        );
        assert_eq!(key_delta(Key::Enter, shift, 1., 10.), None);
    }

    #[test]
    fn dragging_the_left_edge_keeps_the_right_edge_still() {
        let wider = resized(BOX, Handle::LEFT, Point::new(-30., 0.), LIMITS);
        assert_eq!((wider.x, wider.width, wider.right()), (20., 130., 150.));
    }

    #[test]
    fn a_box_cannot_shrink_below_its_minimum_or_grow_past_its_bounds() {
        let tiny = resized(BOX, Handle::TOP_LEFT, Point::new(500., 500.), LIMITS);
        assert_eq!(
            (tiny.width, tiny.height, tiny.right(), tiny.bottom()),
            (40., 40., 150., 130.)
        );
        let huge = resized(BOX, Handle::BOTTOM_RIGHT, Point::new(900., 900.), LIMITS);
        assert_eq!((huge.right(), huge.bottom()), (400., 300.));
    }

    #[test]
    fn a_maximum_size_caps_growth() {
        let capped = ResizeLimits {
            max: Some(Size {
                width: 120.,
                height: 90.,
            }),
            ..LIMITS
        };
        let grown = resized(BOX, Handle::BOTTOM_RIGHT, Point::new(500., 500.), capped);
        assert_eq!((grown.width, grown.height), (120., 90.));
    }

    #[test]
    fn each_grip_has_the_cursor_for_the_way_it_drags() {
        assert_eq!(resize_cursor(Handle::LEFT), "ew-resize");
        assert_eq!(resize_cursor(Handle::TOP), "ns-resize");
        assert_eq!(resize_cursor(Handle::BOTTOM_RIGHT), "nwse-resize");
        assert_eq!(
            resize_cursor(Handle {
                horizontal: 1,
                vertical: -1
            }),
            "nesw-resize"
        );
    }
}
