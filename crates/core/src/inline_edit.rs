/// Editing a value in place: a label that becomes a field, then saves or cancels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InlineEdit {
    value: String,
    draft: Option<String>,
}

impl InlineEdit {
    pub fn new(value: &str) -> Self {
        InlineEdit {
            value: value.to_string(),
            draft: None,
        }
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn is_editing(&self) -> bool {
        self.draft.is_some()
    }

    /// The text to show in the field while editing.
    pub fn draft(&self) -> Option<&str> {
        self.draft.as_deref()
    }

    pub fn start(&mut self) {
        self.draft.get_or_insert_with(|| self.value.clone());
    }

    pub fn input(&mut self, text: &str) {
        if let Some(draft) = self.draft.as_mut() {
            *draft = text.to_string();
        }
    }

    /// Saves the draft if `accept` allows it, and returns the new value if it changed.
    /// A rejected draft stays open so it can be corrected.
    pub fn submit(&mut self, accept: impl Fn(&str) -> bool) -> Option<String> {
        let draft = self.draft.as_ref()?;
        if !accept(draft) {
            return None;
        }
        let draft = self.draft.take()?;
        if draft == self.value {
            return None;
        }
        self.value = draft;
        Some(self.value.clone())
    }

    pub fn cancel(&mut self) {
        self.draft = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_starts_from_the_current_value_and_saves_on_submit() {
        let mut title = InlineEdit::new("Untitled");
        title.start();
        assert_eq!(title.draft(), Some("Untitled"));
        title.input("Roadmap");
        assert_eq!(title.submit(|_| true), Some("Roadmap".to_string()));
        assert!(!title.is_editing());
        assert_eq!(title.value(), "Roadmap");
    }

    #[test]
    fn cancelling_keeps_the_old_value() {
        let mut title = InlineEdit::new("Untitled");
        title.start();
        title.input("Scrapped");
        title.cancel();
        assert_eq!(title.value(), "Untitled");
    }

    #[test]
    fn a_rejected_draft_stays_open_for_correction() {
        let mut title = InlineEdit::new("Untitled");
        title.start();
        title.input("");
        assert_eq!(title.submit(|text| !text.is_empty()), None);
        assert!(title.is_editing());
    }

    #[test]
    fn submitting_an_unchanged_value_reports_nothing() {
        let mut title = InlineEdit::new("Untitled");
        title.start();
        assert_eq!(title.submit(|_| true), None);
        assert!(!title.is_editing());
        title.input("ignored while not editing");
        assert_eq!(title.value(), "Untitled");
    }
}
