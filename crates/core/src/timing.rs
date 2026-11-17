//! Presses that mean something over time. Both are driven by the adapter's clock: start
//! them, then call `tick` when the time they ask for arrives.

/// A press that fires once it has been held long enough, as a long-press does.
#[derive(Clone, Copy, Debug)]
pub struct Hold {
    delay_ms: u64,
    fires_at: Option<u64>,
}

impl Hold {
    pub fn new(delay_ms: u64) -> Self {
        Hold {
            delay_ms,
            fires_at: None,
        }
    }

    /// Begins the press. Returns when to call `tick`.
    pub fn start(&mut self, now_ms: u64) -> u64 {
        let at = now_ms + self.delay_ms;
        self.fires_at = Some(at);
        at
    }

    /// Whether the hold completes now. It fires at most once per press.
    pub fn tick(&mut self, now_ms: u64) -> bool {
        match self.fires_at {
            Some(at) if now_ms >= at => {
                self.fires_at = None;
                true
            }
            _ => false,
        }
    }

    /// The press ended early.
    pub fn release(&mut self) {
        self.fires_at = None;
    }
}

/// A press that steps once at once, then keeps stepping while held, as a stepper
/// button or a scroll arrow does.
#[derive(Clone, Copy, Debug)]
pub struct Repeat {
    delay_ms: u64,
    interval_ms: u64,
    next_at: Option<u64>,
}

impl Default for Repeat {
    fn default() -> Self {
        Repeat {
            delay_ms: 400,
            interval_ms: 60,
            next_at: None,
        }
    }
}

impl Repeat {
    pub fn new(delay_ms: u64, interval_ms: u64) -> Self {
        Repeat {
            delay_ms,
            interval_ms: interval_ms.max(1),
            next_at: None,
        }
    }

    /// Begins the press, which counts as the first step. Returns when to call `tick`.
    pub fn start(&mut self, now_ms: u64) -> u64 {
        let at = now_ms + self.delay_ms;
        self.next_at = Some(at);
        at
    }

    /// How many steps are due now, and when to call `tick` next. A late tick catches
    /// up on the steps it missed.
    pub fn tick(&mut self, now_ms: u64) -> (u32, Option<u64>) {
        let Some(next_at) = self.next_at else {
            return (0, None);
        };
        if now_ms < next_at {
            return (0, Some(next_at));
        }
        let steps = (now_ms - next_at) / self.interval_ms + 1;
        let following = next_at + steps * self.interval_ms;
        self.next_at = Some(following);
        (steps as u32, Some(following))
    }

    pub fn release(&mut self) {
        self.next_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hold_fires_once_after_its_delay() {
        let mut hold = Hold::new(500);
        assert_eq!(hold.start(1000), 1500);
        assert!(!hold.tick(1499));
        assert!(hold.tick(1500));
        assert!(!hold.tick(2000));
    }

    #[test]
    fn releasing_early_cancels_the_hold() {
        let mut hold = Hold::new(500);
        hold.start(0);
        hold.release();
        assert!(!hold.tick(9999));
    }

    #[test]
    fn a_repeat_waits_then_steps_at_a_steady_interval() {
        let mut repeat = Repeat::new(400, 50);
        assert_eq!(repeat.start(0), 400);
        assert_eq!(repeat.tick(399), (0, Some(400)));
        assert_eq!(repeat.tick(400), (1, Some(450)));
        assert_eq!(repeat.tick(450), (1, Some(500)));
    }

    #[test]
    fn a_late_tick_catches_up_and_release_stops_it() {
        let mut repeat = Repeat::new(400, 50);
        repeat.start(0);
        assert_eq!(repeat.tick(560), (4, Some(600)));
        repeat.release();
        assert_eq!(repeat.tick(5000), (0, None));
    }
}
