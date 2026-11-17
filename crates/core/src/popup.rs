//! Where a popup goes. A primitive states the placement it wants; `place` is the
//! geometry for adapters whose platform does not position popups itself.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Bottom,
    Top,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement<Part> {
    /// The part the popup is positioned against.
    pub anchor: Part,
    pub side: Side,
    pub align: Align,
    pub gap: f64,
}

use crate::geometry::Point;
pub use crate::geometry::{Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub x: f64,
    pub y: f64,
    /// The side actually used, which differs from the request after a flip.
    pub side: Side,
}

/// Positions a popup beside its anchor. It flips to the other side when the preferred
/// one overflows and the other fits, and slides along the anchor to stay on screen.
pub fn place(
    anchor: Rect,
    popup: Size,
    viewport: Size,
    side: Side,
    align: Align,
    gap: f64,
) -> Placed {
    let below = anchor.y + anchor.height + gap;
    let above = anchor.y - gap - popup.height;
    let fits_below = below + popup.height <= viewport.height;
    let fits_above = above >= 0.0;
    let side = match side {
        Side::Bottom if !fits_below && fits_above => Side::Top,
        Side::Top if !fits_above && fits_below => Side::Bottom,
        preferred => preferred,
    };
    let x = match align {
        Align::Start => anchor.x,
        Align::End => anchor.x + anchor.width - popup.width,
    };
    Placed {
        x: x.min(viewport.width - popup.width).max(0.0),
        y: if side == Side::Bottom { below } else { above },
        side,
    }
}

/// Where along a popup's edge its arrow sits so that it points at the middle of the
/// anchor, measured from the popup's left. It stays `inset` clear of the corners.
pub fn arrow_offset(anchor: Rect, placed: Placed, popup: Size, inset: f64) -> f64 {
    let target = anchor.x + anchor.width / 2.0 - placed.x;
    target.clamp(
        inset.min(popup.width / 2.0),
        (popup.width - inset).max(popup.width / 2.0),
    )
}

/// Where a popup's arrow starts along the edge facing the anchor, so that it
/// points at the middle of the anchor, and whether it had to stop short of a
/// corner to stay `padding` clear of it. Along the width for a popup above or
/// below, and along the height for one beside.
pub fn arrow_along(
    anchor: Rect,
    popup: Rect,
    arrow: Size,
    beside: bool,
    padding: f64,
) -> (f64, bool) {
    let (middle, start, length, size) = if beside {
        (
            anchor.y + anchor.height / 2.0,
            popup.y,
            popup.height,
            arrow.height,
        )
    } else {
        (
            anchor.x + anchor.width / 2.0,
            popup.x,
            popup.width,
            arrow.width,
        )
    };
    let wanted = middle - start - size / 2.0;
    let offset = wanted.clamp(padding, (length - size - padding).max(padding));
    (offset, offset != wanted)
}

/// Whether a point lies on the way between a trigger and its popup: inside the
/// smallest shape with no dents that holds both, each grown by `padding`. A pointer
/// there has not left, so the popup stays while it crosses the gap.
pub fn bridges(trigger: Rect, popup: Rect, padding: f64, point: Point) -> bool {
    let mut corners: Vec<Point> = [trigger, popup]
        .iter()
        .flat_map(|rect| {
            let (left, right) = (rect.x - padding, rect.right() + padding);
            let (top, bottom) = (rect.y - padding, rect.bottom() + padding);
            [
                Point::new(left, top),
                Point::new(right, top),
                Point::new(right, bottom),
                Point::new(left, bottom),
            ]
        })
        .collect();
    corners.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    let cross =
        |a: Point, b: Point, c: Point| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let half = |points: &mut dyn Iterator<Item = Point>| {
        let mut chain: Vec<Point> = Vec::new();
        for point in points {
            while chain.len() >= 2
                && cross(chain[chain.len() - 2], chain[chain.len() - 1], point) <= 0.0
            {
                chain.pop();
            }
            chain.push(point);
        }
        chain.pop();
        chain
    };
    let mut hull = half(&mut corners.iter().copied());
    hull.extend(half(&mut corners.iter().rev().copied()));
    (0..hull.len()).all(|at| cross(hull[at], hull[(at + 1) % hull.len()], point) >= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWPORT: Size = Size {
        width: 800.0,
        height: 600.0,
    };
    const POPUP: Size = Size {
        width: 200.0,
        height: 150.0,
    };

    fn anchor(x: f64, y: f64) -> Rect {
        Rect {
            x,
            y,
            width: 100.0,
            height: 30.0,
        }
    }

    #[test]
    fn opens_below_the_anchor_with_a_gap() {
        let placed = place(
            anchor(50.0, 50.0),
            POPUP,
            VIEWPORT,
            Side::Bottom,
            Align::Start,
            4.0,
        );
        assert_eq!(
            placed,
            Placed {
                x: 50.0,
                y: 84.0,
                side: Side::Bottom
            }
        );
    }

    #[test]
    fn flips_above_when_there_is_no_room_below() {
        let placed = place(
            anchor(50.0, 500.0),
            POPUP,
            VIEWPORT,
            Side::Bottom,
            Align::Start,
            4.0,
        );
        assert_eq!(
            placed,
            Placed {
                x: 50.0,
                y: 346.0,
                side: Side::Top
            }
        );
    }

    #[test]
    fn keeps_the_preferred_side_when_neither_fits() {
        let tall = Size {
            width: 200.0,
            height: 580.0,
        };
        let placed = place(
            anchor(50.0, 300.0),
            tall,
            VIEWPORT,
            Side::Bottom,
            Align::Start,
            4.0,
        );
        assert_eq!(placed.side, Side::Bottom);
    }

    #[test]
    fn slides_back_on_screen_at_the_right_edge() {
        let placed = place(
            anchor(750.0, 50.0),
            POPUP,
            VIEWPORT,
            Side::Bottom,
            Align::Start,
            4.0,
        );
        assert_eq!(placed.x, 600.0);
    }

    #[test]
    fn end_alignment_lines_up_the_far_edges() {
        let placed = place(
            anchor(400.0, 50.0),
            POPUP,
            VIEWPORT,
            Side::Bottom,
            Align::End,
            4.0,
        );
        assert_eq!(placed.x, 300.0);
    }

    #[test]
    fn the_arrow_points_at_the_anchors_middle_and_stays_off_the_corners() {
        let placed = place(
            anchor(50.0, 50.0),
            POPUP,
            VIEWPORT,
            Side::Bottom,
            Align::Start,
            4.0,
        );
        assert_eq!(arrow_offset(anchor(50.0, 50.0), placed, POPUP, 12.0), 50.0);
        let shifted = place(
            anchor(750.0, 50.0),
            POPUP,
            VIEWPORT,
            Side::Bottom,
            Align::Start,
            4.0,
        );
        assert_eq!(
            arrow_offset(anchor(750.0, 50.0), shifted, POPUP, 12.0),
            188.0,
            "held clear of the corner"
        );
    }
}
