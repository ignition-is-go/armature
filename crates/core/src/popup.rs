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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

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
}
