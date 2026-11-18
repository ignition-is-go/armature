//! Placing a popup beside its anchor on any of four sides, keeping it on screen.

use crate::geometry::{Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceSide {
    Top,
    Bottom,
    Left,
    Right,
}

impl PlaceSide {
    const fn opposite(self) -> Self {
        match self {
            Self::Top => Self::Bottom,
            Self::Bottom => Self::Top,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }

    const fn is_vertical(self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceAlign {
    Start,
    Center,
    End,
}

/// How a popup wants to be placed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaceOptions {
    pub side: PlaceSide,
    pub align: PlaceAlign,
    /// The gap between the anchor and the popup.
    pub side_offset: f64,
    /// A shift along the anchor's edge.
    pub align_offset: f64,
    /// How far the popup keeps from the edges of the view.
    pub collision_padding: f64,
    /// In a right-to-left layout `Start` is the right edge.
    pub rtl: bool,
}

/// Where a popup was placed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub x: f64,
    pub y: f64,
    /// The side used, which is the other one after a flip.
    pub side: PlaceSide,
    /// The room on that side, for limiting the popup's popup.
    pub available: Size,
}

fn point(
    anchor: Rect,
    popup: Size,
    side: PlaceSide,
    align: PlaceAlign,
    options: &PlaceOptions,
) -> (f64, f64) {
    let along = |start: f64, length: f64, popup: f64| match align {
        PlaceAlign::Start => start + options.align_offset,
        PlaceAlign::Center => start + length / 2.0 - popup / 2.0 + options.align_offset,
        PlaceAlign::End => start + length - popup - options.align_offset,
    };
    match side {
        PlaceSide::Bottom => (
            along(anchor.x, anchor.width, popup.width),
            anchor.y + anchor.height + options.side_offset,
        ),
        PlaceSide::Top => (
            along(anchor.x, anchor.width, popup.width),
            anchor.y - popup.height - options.side_offset,
        ),
        PlaceSide::Right => (
            anchor.x + anchor.width + options.side_offset,
            along(anchor.y, anchor.height, popup.height),
        ),
        PlaceSide::Left => (
            anchor.x - popup.width - options.side_offset,
            along(anchor.y, anchor.height, popup.height),
        ),
    }
}

/// How far a popup at `at` runs past the view on `side`. Negative when it fits.
fn overflow(side: PlaceSide, at: (f64, f64), popup: Size, viewport: Rect, padding: f64) -> f64 {
    let (x, y) = at;
    match side {
        PlaceSide::Bottom => y + popup.height - (viewport.y + viewport.height - padding),
        PlaceSide::Top => viewport.y + padding - y,
        PlaceSide::Right => x + popup.width - (viewport.x + viewport.width - padding),
        PlaceSide::Left => viewport.x + padding - x,
    }
}

fn available(side: PlaceSide, anchor: Rect, viewport: Rect, options: &PlaceOptions) -> Size {
    let (gap, padding) = (options.side_offset, options.collision_padding);
    let height = match side {
        PlaceSide::Bottom => {
            viewport.y + viewport.height - padding - (anchor.y + anchor.height + gap)
        }
        PlaceSide::Top => anchor.y - gap - padding - viewport.y,
        _ => f64::mul_add(padding, -2.0, viewport.height),
    };
    let width = match side {
        PlaceSide::Right => viewport.x + viewport.width - padding - (anchor.x + anchor.width + gap),
        PlaceSide::Left => anchor.x - gap - padding - viewport.x,
        _ => f64::mul_add(padding, -2.0, viewport.width),
    };
    Size {
        width: width.max(0.0),
        height: height.max(0.0),
    }
}

/// `clamp` that does not panic when the range is empty: the lower bound wins.
const fn held(value: f64, least: f64, most: f64) -> f64 {
    value.min(most).max(least)
}

/// One axis of a box: where it starts and how long it is.
#[derive(Clone, Copy)]
struct Span {
    start: f64,
    length: f64,
}

impl Span {
    fn end(self) -> f64 {
        self.start + self.length
    }
}

/// A box as its span across and its span down.
const fn spans(rect: Rect) -> (Span, Span) {
    let across = Span {
        start: rect.x,
        length: rect.width,
    };
    let down = Span {
        start: rect.y,
        length: rect.height,
    };
    (across, down)
}

/// `position` moved as little as it takes to keep `size` on screen, clear of the
/// edges by `padding`.
fn kept_on_screen(position: f64, size: f64, view: Span, padding: f64) -> f64 {
    held(position, view.start + padding, view.end() - size - padding)
}

/// `position` moved as little as it takes to keep `size` inside the anchor.
fn kept_inside(position: f64, size: f64, anchor: Span) -> f64 {
    held(position, anchor.start, anchor.end() - size)
}

/// Slides along the anchor's edge to stay on screen, but never so far that the
/// popup leaves the anchor behind.
fn slid_along(position: f64, size: f64, view: Span, anchor: Span, padding: f64) -> f64 {
    let on_screen = kept_on_screen(position, size, view, padding);
    if (on_screen - position).abs() < f64::EPSILON {
        return position;
    }
    held(on_screen, anchor.start - size, anchor.end())
}

/// Places a popup of size `popup` beside `anchor` within `viewport`, the part of
/// the page on show.
///
/// It flips to the other side when that overflows less, slides along the anchor to
/// stay on screen without ever leaving the anchor behind, and goes inside an anchor
/// bigger than it when neither side has room.
#[must_use]
pub fn compute_placement(
    anchor: Rect,
    popup: Size,
    viewport: Rect,
    options: PlaceOptions,
) -> Placement {
    // In a right-to-left layout, start and end swap along a horizontal edge.
    let align = match (options.rtl && options.side.is_vertical(), options.align) {
        (true, PlaceAlign::Start) => PlaceAlign::End,
        (true, PlaceAlign::End) => PlaceAlign::Start,
        (_, align) => align,
    };
    let padding = options.collision_padding;
    let (anchor_across, anchor_down) = spans(anchor);
    let (view_across, view_down) = spans(viewport);

    // The side asked for, unless the opposite one overflows less.
    let mut side = options.side;
    let (mut x, mut y) = point(anchor, popup, side, align, &options);
    let over = overflow(side, (x, y), popup, viewport, padding);
    if over > 0.0 {
        let other = side.opposite();
        let there = point(anchor, popup, other, align, &options);
        if overflow(other, there, popup, viewport, padding) < over {
            side = other;
            (x, y) = there;
        }
    }

    // With no room on either side, a popup smaller than its anchor goes inside it.
    let mut room = available(side, anchor, viewport, &options);
    if overflow(side, (x, y), popup, viewport, padding) > 0.0 {
        let clear = |length: f64| padding.mul_add(-2.0, length).max(0.0);
        if side.is_vertical() && popup.height > 0.0 && anchor.height >= popup.height {
            let on_screen = kept_on_screen(y, popup.height, view_down, padding);
            y = kept_inside(on_screen, popup.height, anchor_down);
            room.height = clear(viewport.height);
        } else if !side.is_vertical() && popup.width > 0.0 && anchor.width >= popup.width {
            let on_screen = kept_on_screen(x, popup.width, view_across, padding);
            x = kept_inside(on_screen, popup.width, anchor_across);
            room.width = clear(viewport.width);
        }
    }

    if side.is_vertical() {
        x = slid_along(x, popup.width, view_across, anchor_across, padding);
    } else {
        y = slid_along(y, popup.height, view_down, anchor_down, padding);
    }
    Placement {
        x,
        y,
        side,
        available: room,
    }
}

/// Where the anchor's middle falls along the popup edge that faces it, kept
/// `padding` clear of the corners, for pointing an arrow or a transform origin.
#[must_use]
pub fn arrow_offset(anchor: Rect, popup: Rect, side: PlaceSide, padding: f64) -> f64 {
    let (center, length) = if side.is_vertical() {
        (anchor.x + anchor.width / 2.0 - popup.x, popup.width)
    } else {
        (anchor.y + anchor.height / 2.0 - popup.y, popup.height)
    };
    center
        .max(padding)
        .min(padding.max(length - padding))
        .round()
}
