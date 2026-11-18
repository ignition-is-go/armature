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
use crate::keys::{Dir, Orientation};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub x: f64,
    pub y: f64,
    /// The side actually used, which differs from the request after a flip.
    pub side: Side,
}

/// Positions a popup beside its anchor. It flips to the other side when the preferred
/// one overflows and the other fits, and slides along the anchor to stay on screen.
#[must_use]
pub fn place(
    anchor: Rect,
    popup: Size,
    viewport: Size,
    side: Side,
    align: Align,
    gap: f64,
) -> Placed {
    use crate::placement::{PlaceAlign, PlaceOptions, PlaceSide, compute_placement};
    let options = PlaceOptions {
        side: if side == Side::Bottom {
            PlaceSide::Bottom
        } else {
            PlaceSide::Top
        },
        align: if align == Align::Start {
            PlaceAlign::Start
        } else {
            PlaceAlign::End
        },
        side_offset: gap,
        align_offset: 0.0,
        collision_padding: 0.0,
        dir: Dir::Ltr,
    };
    let placed = compute_placement(
        anchor,
        popup,
        Rect::new(0.0, 0.0, viewport.width, viewport.height),
        options,
    );
    Placed {
        x: placed.x,
        y: placed.y,
        side: if placed.side == PlaceSide::Top {
            Side::Top
        } else {
            Side::Bottom
        },
    }
}

/// Where along a popup's edge its arrow sits so that it points at the middle of the
/// anchor, measured from the popup's left. It stays `inset` clear of the corners.
#[must_use]
pub fn arrow_offset(anchor: Rect, placed: Placed, popup: Size, inset: f64) -> f64 {
    let target = anchor.x + anchor.width / 2.0 - placed.x;
    target.clamp(
        inset.min(popup.width / 2.0),
        (popup.width - inset).max(popup.width / 2.0),
    )
}

/// Where a popup's arrow goes along the edge that faces the anchor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArrowSpot {
    /// How far along the edge the arrow starts.
    pub offset: f64,
    /// Whether it stopped short of a corner, and so no longer points at the middle
    /// of the anchor.
    pub clamped: bool,
}

/// Where a popup's arrow goes so that it points at the middle of the anchor,
/// staying `padding` clear of the corners.
///
/// `along` is the edge's direction: horizontal for a popup above or below, and
/// vertical for one beside.
#[must_use]
pub fn arrow_along(
    anchor: Rect,
    popup: Rect,
    arrow: Size,
    along: Orientation,
    padding: f64,
) -> ArrowSpot {
    let (middle, start, length, size) = if along == Orientation::Vertical {
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
    ArrowSpot {
        offset,
        clamped: (offset - wanted).abs() > f64::EPSILON,
    }
}

/// Whether a point lies on the way between a trigger and its popup:
///
/// inside the smallest shape with no dents that holds both, each grown by `padding`. A
/// pointer there has not left, so the popup stays while it crosses the gap.
#[must_use]
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
        |a: Point, b: Point, c: Point| (b.y - a.y).mul_add(-(c.x - a.x), (b.x - a.x) * (c.y - a.y));
    let half = |points: &mut dyn Iterator<Item = Point>| {
        let mut chain: Vec<Point> = Vec::new();
        for point in points {
            while chain.len() >= 2
                && cross(
                    chain
                        .get(chain.len().saturating_sub(2))
                        .copied()
                        .unwrap_or_default(),
                    chain
                        .get(chain.len().saturating_sub(1))
                        .copied()
                        .unwrap_or_default(),
                    point,
                ) <= 0.0
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
    // The point is inside when it lies on the same side of every edge.
    let edges = hull.iter().zip(hull.iter().cycle().skip(1));
    edges
        .take(hull.len())
        .all(|(from, to)| cross(*from, *to, point) >= 0.0)
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
    fn when_neither_side_fits_it_takes_the_one_with_more_room() {
        let tall = Size {
            width: 200.0,
            height: 580.0,
        };
        // Near the top of the view there is more room below, so it stays below.
        let placed = place(
            anchor(50.0, 100.0),
            tall,
            VIEWPORT,
            Side::Bottom,
            Align::Start,
            4.0,
        );
        assert_eq!(placed.side, Side::Bottom);
        // Lower down there is more room above, so it goes there.
        let placed = place(
            anchor(50.0, 400.0),
            tall,
            VIEWPORT,
            Side::Bottom,
            Align::Start,
            4.0,
        );
        assert_eq!(placed.side, Side::Top);
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
