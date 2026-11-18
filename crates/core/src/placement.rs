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
    /// The room on that side, for limiting the popup's size.
    pub available: Size,
}

fn point(
    anchor: Rect,
    size: Size,
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
            along(anchor.x, anchor.width, size.width),
            anchor.y + anchor.height + options.side_offset,
        ),
        PlaceSide::Top => (
            along(anchor.x, anchor.width, size.width),
            anchor.y - size.height - options.side_offset,
        ),
        PlaceSide::Right => (
            anchor.x + anchor.width + options.side_offset,
            along(anchor.y, anchor.height, size.height),
        ),
        PlaceSide::Left => (
            anchor.x - size.width - options.side_offset,
            along(anchor.y, anchor.height, size.height),
        ),
    }
}

/// How far a popup at `at` runs past the view on `side`. Negative when it fits.
fn overflow(side: PlaceSide, at: (f64, f64), size: Size, viewport: Rect, padding: f64) -> f64 {
    match side {
        PlaceSide::Bottom => at.1 + size.height - (viewport.y + viewport.height - padding),
        PlaceSide::Top => viewport.y + padding - at.1,
        PlaceSide::Right => at.0 + size.width - (viewport.x + viewport.width - padding),
        PlaceSide::Left => viewport.x + padding - at.0,
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

/// Places a popup of `size` beside `anchor` within `viewport`, the part of the
/// page on show. It flips to the other side when that overflows less, slides
/// along the anchor to stay on screen without ever leaving the anchor behind, and
/// goes inside an anchor bigger than it when neither side has room.
#[must_use]
pub fn compute_placement(
    anchor: Rect,
    size: Size,
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
    let mut side = options.side;
    let mut at = point(anchor, size, side, align, &options);
    let over = overflow(side, at, size, viewport, padding);
    if over > 0.0 {
        let other = side.opposite();
        let there = point(anchor, size, other, align, &options);
        if overflow(other, there, size, viewport, padding) < over {
            (side, at) = (other, there);
        }
    }
    let mut room = available(side, anchor, viewport, &options);
    if overflow(side, at, size, viewport, padding) > 0.0 {
        if side.is_vertical() && size.height > 0.0 && anchor.height >= size.height {
            let on_screen = held(
                at.1,
                viewport.y + padding,
                viewport.y + viewport.height - size.height - padding,
            );
            at.1 = held(on_screen, anchor.y, anchor.y + anchor.height - size.height);
            room.height = padding.mul_add(-2.0, viewport.height).max(0.0);
        } else if !side.is_vertical() && size.width > 0.0 && anchor.width >= size.width {
            let on_screen = held(
                at.0,
                viewport.x + padding,
                viewport.x + viewport.width - size.width - padding,
            );
            at.0 = held(on_screen, anchor.x, anchor.x + anchor.width - size.width);
            room.width = padding.mul_add(-2.0, viewport.width).max(0.0);
        }
    }
    if side.is_vertical() {
        let x = held(
            at.0,
            viewport.x + padding,
            viewport.x + viewport.width - size.width - padding,
        );
        at.0 = if x == at.0 {
            x
        } else {
            held(x, anchor.x - size.width, anchor.x + anchor.width)
        };
    } else {
        let y = held(
            at.1,
            viewport.y + padding,
            viewport.y + viewport.height - size.height - padding,
        );
        at.1 = if y == at.1 {
            y
        } else {
            held(y, anchor.y - size.height, anchor.y + anchor.height)
        };
    }
    Placement {
        x: at.0,
        y: at.1,
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
