/// Recorded states to step back and forward through.
#[derive(Clone, Debug)]
pub struct UndoStack<T> {
    /// What undo steps back to, oldest first.
    past: Vec<T>,
    present: T,
    /// What redo steps forward to, the next one last.
    future: Vec<T>,
    limit: usize,
}

impl<T> UndoStack<T> {
    /// Keeps at most `limit` states, dropping the oldest.
    pub fn new(initial: T, limit: usize) -> Self {
        Self {
            past: Vec::new(),
            present: initial,
            future: Vec::new(),
            limit: limit.max(1),
        }
    }

    #[must_use]
    pub const fn current(&self) -> &T {
        &self.present
    }

    /// Records a new state. Anything that had been undone can no longer be redone.
    pub fn record(&mut self, state: T) {
        self.future.clear();
        self.past.push(std::mem::replace(&mut self.present, state));
        // The present counts toward the limit.
        while self.past.len() >= self.limit {
            self.past.remove(0);
        }
    }

    #[must_use]
    pub const fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    #[must_use]
    pub const fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    /// Steps back and returns the state to restore.
    pub fn undo(&mut self) -> Option<&T> {
        let previous = self.past.pop()?;
        self.future
            .push(std::mem::replace(&mut self.present, previous));
        Some(&self.present)
    }

    /// Steps forward again and returns the state to restore.
    pub fn redo(&mut self) -> Option<&T> {
        let next = self.future.pop()?;
        self.past.push(std::mem::replace(&mut self.present, next));
        Some(&self.present)
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

    #[test]
    fn undo_and_redo_walk_the_recorded_states() {
        let mut stack = UndoStack::new("a", 10);
        stack.record("b");
        stack.record("c");
        assert_eq!(stack.undo(), Some(&"b"));
        assert_eq!(stack.undo(), Some(&"a"));
        assert_eq!(stack.undo(), None);
        assert_eq!(stack.redo(), Some(&"b"));
        assert_eq!(stack.current(), &"b");
    }

    #[test]
    fn recording_after_an_undo_discards_the_redo_branch() {
        let mut stack = UndoStack::new(1, 10);
        stack.record(2);
        stack.undo();
        stack.record(3);
        assert!(!stack.can_redo());
        assert_eq!(stack.undo(), Some(&1));
    }

    #[test]
    fn the_oldest_state_is_dropped_past_the_limit() {
        let mut stack = UndoStack::new(1, 2);
        stack.record(2);
        stack.record(3);
        assert_eq!(stack.undo(), Some(&2));
        assert_eq!(stack.undo(), None);
    }
}
