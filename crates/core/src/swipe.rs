//! Actions revealed by swiping a row sideways, with an optional full swipe that
//! triggers the first one.

use crate::gesture::{DECELERATION, project_distance, rubber_band};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwipeActions {
    /// Width of the actions behind the row.
    pub reveal: f64,
    /// How far the row must travel to trigger the action outright, if that is allowed.
    pub full: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Swipe {
    Closed,
    Revealed,
    /// Swiped all the way: run the action.
    Full,
}

impl SwipeActions {
    /// How far to show the row moved while the finger is down. Past the furthest stop
    /// it resists.
    pub fn offset(&self, dragged: f64) -> f64 {
        let dragged = dragged.max(0.0);
        let limit = self.full.unwrap_or(self.reveal);
        if dragged <= limit {
            dragged
        } else {
            limit + rubber_band(dragged - limit, self.reveal.max(1.0), 0.55)
        }
    }

    /// Where the row comes to rest when released at `dragged` moving at `velocity`
    /// (units per second, positive in the swipe direction). A flick counts for where
    /// it would have coasted to.
    pub fn settle(&self, dragged: f64, velocity: f64) -> Swipe {
        let landing = dragged + project_distance(velocity, DECELERATION);
        match self.full {
            Some(full) if landing >= full => Swipe::Full,
            _ if landing >= self.reveal / 2.0 => Swipe::Revealed,
            _ => Swipe::Closed,
        }
    }

    /// The offset a settled row rests at.
    pub fn rest(&self, swipe: Swipe) -> f64 {
        match swipe {
            Swipe::Closed => 0.0,
            Swipe::Revealed => self.reveal,
            Swipe::Full => self.full.unwrap_or(self.reveal),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW: SwipeActions = SwipeActions {
        reveal: 80.0,
        full: Some(240.0),
    };

    #[test]
    fn releasing_past_halfway_reveals_the_actions() {
        assert_eq!(ROW.settle(30.0, 0.0), Swipe::Closed);
        assert_eq!(ROW.settle(45.0, 0.0), Swipe::Revealed);
        assert_eq!(ROW.settle(250.0, 0.0), Swipe::Full);
    }

    #[test]
    fn a_flick_counts_for_where_it_would_coast_to() {
        assert_eq!(ROW.settle(10.0, 200.0), Swipe::Revealed);
        assert_eq!(ROW.settle(60.0, -200.0), Swipe::Closed);
    }

    #[test]
    fn a_row_without_full_swipe_only_reveals() {
        let row = SwipeActions {
            reveal: 80.0,
            full: None,
        };
        assert_eq!(row.settle(900.0, 0.0), Swipe::Revealed);
        assert_eq!(row.rest(Swipe::Full), 80.0);
    }

    #[test]
    fn the_row_follows_the_finger_then_resists_past_the_last_stop() {
        assert_eq!(ROW.offset(100.0), 100.0);
        assert_eq!(ROW.offset(-50.0), 0.0);
        let past = ROW.offset(340.0);
        assert!(past > 240.0 && past < 340.0);
    }
}
