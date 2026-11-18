/// Recorded states to step back and forward through.
#[derive(Clone, Debug)]
pub struct UndoStack<T> {
    states: Vec<T>,
    current: usize,
    limit: usize,
}

impl<T> UndoStack<T> {
    /// Keeps at most `limit` states, dropping the oldest.
    pub fn new(initial: T, limit: usize) -> Self {
        Self {
            states: vec![initial],
            current: 0,
            limit: limit.max(1),
        }
    }

    #[must_use]
    pub fn current(&self) -> &T {
        &self.states[self.current]
    }

    /// Records a new state. Anything that had been undone can no longer be redone.
    pub fn record(&mut self, state: T) {
        self.states.truncate(self.current + 1);
        self.states.push(state);
        if self.states.len() > self.limit {
            self.states.remove(0);
        }
        self.current = self.states.len() - 1;
    }

    #[must_use]
    pub const fn can_undo(&self) -> bool {
        self.current > 0
    }

    #[must_use]
    pub const fn can_redo(&self) -> bool {
        self.current + 1 < self.states.len()
    }

    /// Steps back and returns the state to restore.
    pub fn undo(&mut self) -> Option<&T> {
        self.can_undo().then(|| {
            self.current -= 1;
            &self.states[self.current]
        })
    }

    pub fn redo(&mut self) -> Option<&T> {
        self.can_redo().then(|| {
            self.current += 1;
            &self.states[self.current]
        })
    }
}

#[cfg(test)]
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
