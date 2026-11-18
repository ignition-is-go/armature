//! Telling a drag from a click: a press only becomes a drag once the pointer has
//! moved past a small threshold.

use crate::geometry::Point;

/// How far a pressed pointer may wander and still count as a click.
pub const DRAG_THRESHOLD: f64 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DragEvent {
    /// The press crossed the threshold. `total` is the movement so far.
    Start {
        origin: Point,
        total: Point,
    },
    Move {
        delta: Point,
        total: Point,
    },
    End {
        total: Point,
    },
    /// Released without ever becoming a drag.
    Click,
    Cancel,
}

#[derive(Clone, Copy, Debug)]
struct Press {
    origin: Point,
    last: Point,
    dragging: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Drag {
    threshold: f64,
    press: Option<Press>,
}

impl Default for Drag {
    fn default() -> Self {
        Self {
            threshold: DRAG_THRESHOLD,
            press: None,
        }
    }
}

impl Drag {
    #[must_use]
    pub const fn with_threshold(threshold: f64) -> Self {
        Self {
            threshold,
            press: None,
        }
    }

    #[must_use]
    pub fn is_dragging(&self) -> bool {
        self.press.is_some_and(|press| press.dragging)
    }

    pub const fn down(&mut self, at: Point) {
        self.press = Some(Press {
            origin: at,
            last: at,
            dragging: false,
        });
    }

    pub fn moved(&mut self, at: Point) -> Option<DragEvent> {
        let press = self.press.as_mut()?;
        let total = at.minus(press.origin);
        let delta = at.minus(press.last);
        press.last = at;
        if press.dragging {
            return Some(DragEvent::Move { delta, total });
        }
        if at.distance(press.origin) < self.threshold {
            return None;
        }
        press.dragging = true;
        Some(DragEvent::Start {
            origin: press.origin,
            total,
        })
    }

    pub fn up(&mut self, at: Point) -> Option<DragEvent> {
        let press = self.press.take()?;
        Some(if press.dragging {
            DragEvent::End {
                total: at.minus(press.origin),
            }
        } else {
            DragEvent::Click
        })
    }

    /// Ends the press without a result, as when Escape is pressed or the pointer is
    /// taken away by the platform.
    pub fn cancel(&mut self) -> Option<DragEvent> {
        self.press.take().map(|_| DragEvent::Cancel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn a_press_that_barely_moves_is_a_click() {
        let mut drag = Drag::default();
        drag.down(at(10., 10.));
        assert_eq!(drag.moved(at(11., 11.)), None);
        assert_eq!(drag.up(at(11., 11.)), Some(DragEvent::Click));
    }

    #[test]
    fn crossing_the_threshold_starts_a_drag_that_reports_each_step() {
        let mut drag = Drag::default();
        drag.down(at(10., 10.));
        assert_eq!(
            drag.moved(at(14., 10.)),
            Some(DragEvent::Start {
                origin: at(10., 10.),
                total: at(4., 0.)
            })
        );
        assert_eq!(
            drag.moved(at(20., 15.)),
            Some(DragEvent::Move {
                delta: at(6., 5.),
                total: at(10., 5.)
            })
        );
        assert_eq!(
            drag.up(at(20., 15.)),
            Some(DragEvent::End { total: at(10., 5.) })
        );
        assert!(!drag.is_dragging());
    }

    #[test]
    fn a_drag_that_returns_to_its_origin_is_still_a_drag() {
        let mut drag = Drag::default();
        drag.down(at(0., 0.));
        drag.moved(at(10., 0.));
        drag.moved(at(0., 0.));
        assert_eq!(
            drag.up(at(0., 0.)),
            Some(DragEvent::End { total: at(0., 0.) })
        );
    }

    #[test]
    fn cancelling_ends_the_press_and_movement_without_a_press_is_ignored() {
        let mut drag = Drag::default();
        assert_eq!(drag.moved(at(50., 50.)), None);
        drag.down(at(0., 0.));
        assert_eq!(drag.cancel(), Some(DragEvent::Cancel));
        assert_eq!(drag.up(at(0., 0.)), None);
    }
}
