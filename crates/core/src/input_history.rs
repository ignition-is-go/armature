/// Earlier entries of a text field, recalled the way a shell recalls commands.
#[derive(Clone, Debug, Default)]
pub struct InputHistory {
    entries: Vec<String>,
    /// Index of the entry being shown, when recalling.
    cursor: Option<usize>,
    /// What was typed before recalling began, restored on the way back down.
    draft: String,
}

impl InputHistory {
    /// Remembers a submitted entry. Blank entries and immediate repeats are skipped.
    pub fn push(&mut self, entry: &str) {
        self.cursor = None;
        if !entry.trim().is_empty() && self.entries.last().map(String::as_str) != Some(entry) {
            self.entries.push(entry.to_string());
        }
    }

    /// The entry before the one shown. `typed` is the field's text right now, kept so
    /// `newer` can return to it. `None` at the oldest entry.
    pub fn older(&mut self, typed: &str) -> Option<&str> {
        let target = match self.cursor {
            None => self.entries.len().checked_sub(1)?,
            Some(at) => at.checked_sub(1)?,
        };
        if self.cursor.is_none() {
            self.draft = typed.to_string();
        }
        self.cursor = Some(target);
        Some(&self.entries[target])
    }

    /// The entry after the one shown, ending with the text typed before recalling.
    /// `None` when not recalling.
    pub fn newer(&mut self) -> Option<&str> {
        let at = self.cursor?;
        if at.saturating_add(1) < self.entries.len() {
            self.cursor = Some(at.saturating_add(1));
            Some(&self.entries[at.saturating_add(1)])
        } else {
            self.cursor = None;
            Some(&self.draft)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history() -> InputHistory {
        let mut history = InputHistory::default();
        history.push("first");
        history.push("second");
        history
    }

    #[test]
    fn older_walks_back_and_stops_at_the_oldest() {
        let mut history = history();
        assert_eq!(history.older(""), Some("second"));
        assert_eq!(history.older("second"), Some("first"));
        assert_eq!(history.older("first"), None);
    }

    #[test]
    fn newer_returns_to_what_was_being_typed() {
        let mut history = history();
        history.older("half a thou");
        history.older("second");
        assert_eq!(history.newer(), Some("second"));
        assert_eq!(history.newer(), Some("half a thou"));
        assert_eq!(history.newer(), None);
    }

    #[test]
    fn blanks_and_immediate_repeats_are_not_remembered() {
        let mut history = history();
        history.push("second");
        history.push("   ");
        assert_eq!(history.older(""), Some("second"));
        assert_eq!(history.older(""), Some("first"));
    }
}
