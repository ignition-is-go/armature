//! A single line of text with a caret, for renderers that have no text field of their
//! own. The adapter feeds it keys and draws `text` with the caret at `caret`.

use crate::keys::{Key, Modifiers};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextEdit {
    text: String,
    /// The caret's position, counted in characters.
    caret: usize,
}

/// A key that edits text but is not one of the keys a primitive hears.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    Insert(char),
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
}

impl TextEdit {
    /// Starts with the caret at the end, where typing continues.
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            caret: text.chars().count(),
        }
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub const fn caret(&self) -> usize {
        self.caret
    }

    /// Replaces the text from outside, keeping the caret where it was if it still fits.
    pub fn set_text(&mut self, text: &str) {
        if text != self.text {
            self.text = text.to_string();
            self.caret = self.caret.min(self.text.chars().count());
        }
    }

    fn byte_at(&self, position: usize) -> usize {
        self.text
            .char_indices()
            .nth(position)
            .map_or(self.text.len(), |(byte, _)| byte)
    }

    /// The edit a key means, if any. Keys held with Ctrl or Alt are shortcuts, not
    /// text.
    #[must_use]
    pub const fn edit_for(key: Key, mods: Modifiers) -> Option<Edit> {
        if mods.ctrl || mods.alt {
            return None;
        }
        Some(match key {
            Key::Char(ch) => Edit::Insert(ch),
            Key::Space => Edit::Insert(' '),
            Key::Backspace => Edit::Backspace,
            Key::ArrowLeft => Edit::Left,
            Key::ArrowRight => Edit::Right,
            Key::Home => Edit::Home,
            Key::End => Edit::End,
            _ => return None,
        })
    }

    /// Applies an edit. Returns whether the text itself changed, as opposed to only
    /// the caret moving.
    pub fn apply(&mut self, edit: Edit) -> bool {
        let length = self.text.chars().count();
        match edit {
            Edit::Insert(ch) => {
                self.text.insert(self.byte_at(self.caret), ch);
                self.caret = self.caret.saturating_add(1);
                true
            }
            Edit::Backspace if self.caret > 0 => {
                self.caret = self.caret.saturating_sub(1);
                self.text.remove(self.byte_at(self.caret));
                true
            }
            Edit::Delete if self.caret < length => {
                self.text.remove(self.byte_at(self.caret));
                true
            }
            Edit::Left => {
                self.caret = self.caret.saturating_sub(1);
                false
            }
            Edit::Right => {
                self.caret = (self.caret.saturating_add(1)).min(length);
                false
            }
            Edit::Home => {
                self.caret = 0;
                false
            }
            Edit::End => {
                self.caret = length;
                false
            }
            Edit::Backspace | Edit::Delete => false,
        }
    }
}

/// What changed between two texts, as one replaced stretch: where it starts, what
/// was taken out and what was put in. Enough to say what a single edit did.
#[must_use]
pub fn text_diff(before: &str, after: &str) -> (usize, String, String) {
    let (old, new): (Vec<char>, Vec<char>) = (before.chars().collect(), after.chars().collect());
    let start = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let room = (old.len().min(new.len())).saturating_sub(start);
    let end = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    (
        start,
        old.get(start..old.len().saturating_sub(end))
            .unwrap_or_default()
            .iter()
            .collect(),
        new.get(start..new.len().saturating_sub(end))
            .unwrap_or_default()
            .iter()
            .collect(),
    )
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
    fn typing_inserts_at_the_caret() {
        let mut field = TextEdit::new("cat");
        field.apply(Edit::Left);
        assert!(field.apply(Edit::Insert('r')));
        assert_eq!((field.text(), field.caret()), ("cart", 3));
    }

    #[test]
    fn backspace_and_delete_remove_either_side_of_the_caret() {
        let mut field = TextEdit::new("héllo");
        field.apply(Edit::Home);
        field.apply(Edit::Right);
        field.apply(Edit::Right);
        assert!(field.apply(Edit::Backspace));
        assert_eq!(field.text(), "hllo");
        assert!(field.apply(Edit::Delete));
        assert_eq!((field.text(), field.caret()), ("hlo", 1));
    }

    #[test]
    fn the_caret_stops_at_both_ends_and_nothing_is_removed_there() {
        let mut field = TextEdit::new("ab");
        assert!(!field.apply(Edit::Delete));
        field.apply(Edit::Right);
        assert_eq!(field.caret(), 2);
        field.apply(Edit::Home);
        assert!(!field.apply(Edit::Backspace));
        field.apply(Edit::Left);
        assert_eq!((field.text(), field.caret()), ("ab", 0));
    }

    #[test]
    fn keys_held_with_ctrl_are_shortcuts_and_not_text() {
        let ctrl = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        assert_eq!(TextEdit::edit_for(Key::Char('a'), ctrl), None);
        assert_eq!(
            TextEdit::edit_for(Key::Char('a'), Modifiers::default()),
            Some(Edit::Insert('a'))
        );
        assert_eq!(TextEdit::edit_for(Key::Enter, Modifiers::default()), None);
    }

    #[test]
    fn text_set_from_outside_keeps_the_caret_in_range() {
        let mut field = TextEdit::new("hello");
        field.set_text("hi");
        assert_eq!(field.caret(), 2);
        field.apply(Edit::Home);
        field.set_text("hi there");
        assert_eq!(field.caret(), 0);
    }

    #[test]
    fn a_change_between_two_texts_is_one_replaced_stretch() {
        assert_eq!(
            text_diff("hello world", "hello brave world"),
            (6, String::new(), "brave ".to_string())
        );
        assert_eq!(
            text_diff("colour", "color"),
            (4, "u".to_string(), String::new())
        );
        assert_eq!(
            text_diff("cat", "cut"),
            (1, "a".to_string(), "u".to_string())
        );
        assert_eq!(text_diff("same", "same"), (4, String::new(), String::new()));
    }
}
