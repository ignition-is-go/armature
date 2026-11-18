/// Keeps something rendered while it animates out. Without it, closing would remove
/// the element before its exit animation could play.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Presence {
    #[default]
    Absent,
    /// Rendered and playing its entrance.
    Entering,
    Present,
    /// Closed, but still rendered while its exit plays.
    Exiting,
}

impl Presence {
    /// Follows the open state. Returns whether anything changed.
    pub fn set(&mut self, open: bool) -> bool {
        let next = match (*self, open) {
            (Self::Absent | Self::Exiting, true) => Self::Entering,
            (Self::Entering | Self::Present, false) => Self::Exiting,
            (unchanged, _) => unchanged,
        };
        std::mem::replace(self, next) != next
    }

    /// The running animation finished.
    pub const fn animation_ended(&mut self) {
        *self = match *self {
            Self::Entering => Self::Present,
            Self::Exiting => Self::Absent,
            settled => settled,
        };
    }

    /// Whether to render it at all.
    #[must_use]
    pub fn is_rendered(self) -> bool {
        self != Self::Absent
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
    fn closing_keeps_it_rendered_until_the_exit_finishes() {
        let mut presence = Presence::default();
        assert!(presence.set(true));
        presence.animation_ended();
        assert_eq!(presence, Presence::Present);
        assert!(presence.set(false));
        assert!(presence.is_rendered());
        presence.animation_ended();
        assert!(!presence.is_rendered());
    }

    #[test]
    fn reopening_mid_exit_plays_the_entrance_again() {
        let mut presence = Presence::Exiting;
        presence.set(true);
        assert_eq!(presence, Presence::Entering);
    }

    #[test]
    fn repeating_the_same_state_changes_nothing() {
        let mut presence = Presence::Present;
        assert!(!presence.set(true));
        let mut absent = Presence::Absent;
        assert!(!absent.set(false));
        absent.animation_ended();
        assert_eq!(absent, Presence::Absent);
    }
}
