//! Reordering a list by pointer or keyboard. One session tracks where the grabbed item
//! came from and where it would land, so either input can drive it and cancel it.

use crate::geometry::{Point, Rect};
use crate::keys::Orientation;

/// The index `from` ends up at when dropped at `at`: it passes every item whose
/// centre the pointer has crossed. `rects` are the items' boxes in their original
/// order.
pub fn drop_index(rects: &[Rect], from: usize, at: Point, axis: Orientation) -> usize {
    let along = |point: Point| {
        if axis == Orientation::Vertical {
            point.y
        } else {
            point.x
        }
    };
    let before = rects
        .iter()
        .enumerate()
        .filter(|(index, rect)| *index != from && along(rect.center()) < along(at))
        .count();
    before.min(rects.len().saturating_sub(1))
}

/// Moves the item at `from` so that it ends up at index `to`.
pub fn move_item<T>(items: &mut Vec<T>, from: usize, to: usize) {
    if from < items.len() && to < items.len() && from != to {
        let item = items.remove(from);
        items.insert(to, item);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reorder {
    pub from: usize,
    pub to: usize,
}

/// One grab, from pick-up to drop or cancel.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReorderSession {
    active: Option<Reorder>,
}

impl ReorderSession {
    pub fn current(&self) -> Option<Reorder> {
        self.active
    }

    pub fn grab(&mut self, index: usize) {
        self.active = Some(Reorder {
            from: index,
            to: index,
        });
    }

    /// Follows the pointer.
    pub fn over(&mut self, rects: &[Rect], at: Point, axis: Orientation) {
        if let Some(active) = self.active.as_mut() {
            active.to = drop_index(rects, active.from, at, axis);
        }
    }

    /// Moves the landing spot by `steps` places, as arrow keys do.
    pub fn step(&mut self, steps: isize, count: usize) {
        if let Some(active) = self.active.as_mut() {
            active.to = active
                .to
                .saturating_add_signed(steps)
                .min(count.saturating_sub(1));
        }
    }

    /// Ends the grab. Returns the move to make, or `None` if nothing changed.
    pub fn drop(&mut self) -> Option<Reorder> {
        self.active
            .take()
            .filter(|reorder| reorder.from != reorder.to)
    }

    pub fn cancel(&mut self) {
        self.active = None;
    }

    /// The order to show while the grab is in progress, as original indices.
    pub fn preview(&self, count: usize) -> Vec<usize> {
        let mut order: Vec<usize> = (0..count).collect();
        if let Some(active) = self.active {
            move_item(&mut order, active.from, active.to);
        }
        order
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReorderStep {
    Grabbed,
    Moved,
    Dropped,
    Cancelled,
}

/// What to tell a screen reader at each step. Positions are spoken from 1.
pub fn reorder_message(step: ReorderStep, label: &str, reorder: Reorder, count: usize) -> String {
    let (from, to) = (reorder.from + 1, reorder.to + 1);
    match step {
        ReorderStep::Grabbed => format!("Picked up {label}, position {from} of {count}."),
        ReorderStep::Moved => format!("{label}, position {to} of {count}."),
        ReorderStep::Dropped => format!("Dropped {label} at position {to} of {count}."),
        ReorderStep::Cancelled => format!("Moved {label} back to position {from} of {count}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column() -> Vec<Rect> {
        (0..4)
            .map(|i| Rect::new(0., i as f64 * 20., 100., 20.))
            .collect()
    }

    #[test]
    fn an_item_lands_past_every_centre_the_pointer_has_crossed() {
        let rects = column();
        let at = |y| Point::new(50., y);
        assert_eq!(drop_index(&rects, 0, at(5.), Orientation::Vertical), 0);
        assert_eq!(drop_index(&rects, 0, at(35.), Orientation::Vertical), 1);
        assert_eq!(drop_index(&rects, 0, at(500.), Orientation::Vertical), 3);
        assert_eq!(drop_index(&rects, 3, at(-50.), Orientation::Vertical), 0);
    }

    #[test]
    fn moving_an_item_shifts_the_ones_between() {
        let mut items = vec!["a", "b", "c", "d"];
        move_item(&mut items, 0, 2);
        assert_eq!(items, ["b", "c", "a", "d"]);
        move_item(&mut items, 3, 0);
        assert_eq!(items, ["d", "b", "c", "a"]);
    }

    #[test]
    fn the_pointer_and_the_keyboard_drive_the_same_session() {
        let mut session = ReorderSession::default();
        session.grab(0);
        session.over(&column(), Point::new(50., 55.), Orientation::Vertical);
        assert_eq!(session.preview(4), [1, 2, 0, 3]);
        session.step(-1, 4);
        assert_eq!(session.drop(), Some(Reorder { from: 0, to: 1 }));
        assert_eq!(session.current(), None);
    }

    #[test]
    fn keyboard_steps_stop_at_the_ends() {
        let mut session = ReorderSession::default();
        session.grab(2);
        session.step(10, 4);
        assert_eq!(session.current().unwrap().to, 3);
        session.step(-10, 4);
        assert_eq!(session.current().unwrap().to, 0);
    }

    #[test]
    fn dropping_where_it_started_or_cancelling_changes_nothing() {
        let mut session = ReorderSession::default();
        session.grab(1);
        assert_eq!(session.drop(), None);
        session.grab(1);
        session.step(2, 4);
        session.cancel();
        assert_eq!(session.preview(4), [0, 1, 2, 3]);
    }

    #[test]
    fn messages_speak_positions_from_one() {
        let moved = reorder_message(ReorderStep::Moved, "Milk", Reorder { from: 0, to: 2 }, 4);
        assert_eq!(moved, "Milk, position 3 of 4.");
    }
}
