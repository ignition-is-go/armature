//! Actions revealed by swiping a row sideways, with an optional full swipe that
//! triggers the first one.

use crate::gesture::rubber_band;

/// How far a press must travel sideways before it is a swipe and not a tap.
pub const SWIPE_SLOP: f64 = 8.0;
/// The release speed, in units per second, that counts as a flick.
pub const FLICK_SPEED: f64 = 300.0;

/// Whether a press that has moved by `dx` and `dy` has become a swipe. A move that is
/// mostly vertical is left to scrolling.
#[must_use]
pub fn is_swipe(dx: f64, dy: f64) -> bool {
    dx.abs() >= SWIPE_SLOP && dx.abs() > dy.abs()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwipeActions {
    /// Width of the actions behind the row.
    pub reveal: f64,
    /// The share of the row's width a swipe must cover to trigger the first action
    /// outright, if that is allowed.
    pub full: Option<f64>,
    pub row_width: f64,
}

impl SwipeActions {
    /// A row whose full swipe needs 60% of its width.
    #[must_use]
    pub const fn with_full_swipe(reveal: f64, row_width: f64) -> Self {
        Self {
            reveal,
            full: Some(0.6),
            row_width,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Swipe {
    Closed,
    Revealed,
    /// Swiped all the way: run the action.
    Full,
}

impl SwipeActions {
    fn full_distance(&self) -> Option<f64> {
        self.full.map(|fraction| self.row_width * fraction)
    }

    /// How far to show the row moved while the finger is down. Past the furthest stop
    /// it resists.
    #[must_use]
    pub fn offset(&self, dragged: f64) -> f64 {
        let dragged = dragged.max(0.0);
        let limit = self.full_distance().unwrap_or(self.reveal);
        if dragged <= limit {
            dragged
        } else {
            limit + rubber_band(dragged - limit, self.reveal.max(1.0), 0.55)
        }
    }

    /// Where the row comes to rest when released at `dragged` moving at `velocity`
    /// (units per second, positive in the swipe direction). Far enough is a full
    /// swipe. Otherwise a flick decides by its direction, and without one the row
    /// stays open only if it is at least half revealed.
    #[must_use]
    pub fn settle(&self, dragged: f64, velocity: f64) -> Swipe {
        if self.full_distance().is_some_and(|full| dragged >= full) {
            Swipe::Full
        } else if velocity.abs() >= FLICK_SPEED {
            if velocity > 0.0 {
                Swipe::Revealed
            } else {
                Swipe::Closed
            }
        } else if dragged >= self.reveal / 2.0 {
            Swipe::Revealed
        } else {
            Swipe::Closed
        }
    }

    /// The offset a settled row rests at. A full swipe runs its action and closes.
    #[must_use]
    pub const fn rest(&self, swipe: Swipe) -> f64 {
        match swipe {
            Swipe::Revealed => self.reveal,
            Swipe::Closed | Swipe::Full => 0.0,
        }
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

    const ROW: SwipeActions = SwipeActions {
        reveal: 80.0,
        full: Some(0.6),
        row_width: 400.0,
    };

    #[test]
    fn a_press_becomes_a_swipe_only_once_it_has_moved_sideways() {
        assert!(!is_swipe(5.0, 0.0));
        assert!(is_swipe(9.0, 2.0));
        assert!(!is_swipe(9.0, 30.0), "mostly vertical is a scroll");
    }

    #[test]
    fn releasing_past_halfway_reveals_the_actions() {
        assert_eq!(ROW.settle(30.0, 0.0), Swipe::Closed);
        assert_eq!(ROW.settle(45.0, 0.0), Swipe::Revealed);
    }

    #[test]
    fn a_full_swipe_needs_most_of_the_rows_width() {
        assert_eq!(ROW.settle(239.0, 0.0), Swipe::Revealed);
        assert_eq!(ROW.settle(240.0, 0.0), Swipe::Full);
        assert_eq!(
            ROW.rest(Swipe::Full),
            0.0,
            "the action ran, so the row closes"
        );
    }

    #[test]
    fn a_flick_decides_by_its_direction_whatever_the_position() {
        assert_eq!(ROW.settle(10.0, 350.0), Swipe::Revealed);
        assert_eq!(ROW.settle(70.0, -350.0), Swipe::Closed);
        assert_eq!(
            ROW.settle(10.0, 200.0),
            Swipe::Closed,
            "too slow to be a flick"
        );
    }

    #[test]
    fn a_row_without_full_swipe_only_reveals() {
        let row = SwipeActions {
            reveal: 80.0,
            full: None,
            row_width: 400.0,
        };
        assert_eq!(row.settle(900.0, 0.0), Swipe::Revealed);
    }

    #[test]
    fn the_row_follows_the_finger_then_resists_past_the_last_stop() {
        assert_eq!(ROW.offset(100.0), 100.0);
        assert_eq!(ROW.offset(-50.0), 0.0);
        let past = ROW.offset(340.0);
        assert!(past > 240.0 && past < 340.0);
    }
}
