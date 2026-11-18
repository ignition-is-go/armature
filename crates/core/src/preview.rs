/// A value being tried before it is committed, such as a slider mid-drag or a colour
/// under the pointer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview<T> {
    committed: T,
    draft: Option<T>,
}

impl<T: Clone + PartialEq> Preview<T> {
    pub const fn new(committed: T) -> Self {
        Self {
            committed,
            draft: None,
        }
    }

    /// What to show: the value being tried, or else the committed one.
    pub fn value(&self) -> &T {
        self.draft.as_ref().unwrap_or(&self.committed)
    }

    pub const fn is_trying(&self) -> bool {
        self.draft.is_some()
    }

    pub fn try_value(&mut self, value: T) {
        self.draft = Some(value);
    }

    /// Ends the interaction and keeps the tried value. Returns it if that changed the
    /// committed value, so the owner is told once, at the end.
    pub fn commit(&mut self) -> Option<T> {
        let draft = self.draft.take()?;
        if draft == self.committed {
            return None;
        }
        self.committed = draft;
        Some(self.committed.clone())
    }

    /// Ends the interaction and goes back to the committed value.
    pub fn cancel(&mut self) {
        self.draft = None;
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
    fn a_tried_value_shows_but_is_not_kept_until_committed() {
        let mut volume = Preview::new(3);
        volume.try_value(7);
        assert_eq!(volume.value(), &7);
        volume.cancel();
        assert_eq!(volume.value(), &3);
    }

    #[test]
    fn commit_reports_the_final_value_once() {
        let mut volume = Preview::new(3);
        volume.try_value(5);
        volume.try_value(8);
        assert_eq!(volume.commit(), Some(8));
        assert_eq!(volume.commit(), None);
        assert_eq!(volume.value(), &8);
    }

    #[test]
    fn ending_where_it_started_reports_nothing() {
        let mut volume = Preview::new(3);
        volume.try_value(9);
        volume.try_value(3);
        assert_eq!(volume.commit(), None);
    }
}
