use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::{Dir, Key};
use crate::primitive::{Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The one real text field. Draw the code's cells from `cells`.
    Input,
    /// One cell of the code, keyed by its place from 0. Pressing it puts the caret
    /// on that character, or after the code if the cell is still empty.
    Cell,
}

/// Which characters a code is made of.
#[derive(Clone, Copy, Debug, Default)]
pub enum Accept {
    #[default]
    Digits,
    /// Letters and digits.
    Alphanumeric,
    /// Whatever this says yes to, one character at a time.
    Custom(fn(char) -> bool),
}

impl Accept {
    fn takes(self, ch: char) -> bool {
        match self {
            Self::Digits => ch.is_ascii_digit(),
            Self::Alphanumeric => ch.is_ascii_alphanumeric(),
            Self::Custom(test) => test(ch),
        }
    }
}

const ACTIONS: &[&str] = &["set", "clear"];

/// A one-time code of a fixed number of characters, digits unless `accepting` says
/// otherwise. Anything else is dropped, so a pasted "123 456" or "123-456" works.
#[derive(Clone, Debug)]
pub struct OtpInput {
    length: usize,
    code: String,
    accept: Accept,
    dir: Dir,
    /// The cell the next key acts on: a character to replace, or the empty cell
    /// after the code.
    slot: usize,
    focused: bool,
}

impl Default for OtpInput {
    fn default() -> Self {
        Self::new(6)
    }
}

impl OtpInput {
    #[must_use]
    pub const fn new(length: usize) -> Self {
        Self {
            length,
            code: String::new(),
            accept: Accept::Digits,
            dir: Dir::Ltr,
            slot: 0,
            focused: false,
        }
    }

    #[must_use]
    pub const fn accepting(mut self, accept: Accept) -> Self {
        self.accept = accept;
        self
    }

    #[must_use]
    pub const fn direction(mut self, dir: Dir) -> Self {
        self.dir = dir;
        self
    }

    /// The cell that has the caret or the selection, while the field has focus.
    #[must_use]
    pub fn active_cell(&self) -> Option<usize> {
        self.focused.then_some(self.slot)
    }

    /// The furthest the caret goes: the empty cell after the code, or the last
    /// character once the code is full.
    fn end(&self) -> usize {
        self.entered().min(self.length.saturating_sub(1))
    }

    fn commit(&mut self, code: String, source: Source, reason: Reason, gate: Gate) -> bool {
        if code == self.code {
            return true;
        }
        let allowed = gate(&ValueChange {
            value: vec![code.clone()],
            source,
            reason,
        });
        if allowed {
            self.code = code;
        }
        allowed
    }

    /// How many characters have been entered.
    fn entered(&self) -> usize {
        self.code.chars().count()
    }

    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.entered() == self.length
    }

    /// One entry per cell: the character in it, if any.
    #[must_use]
    pub fn cells(&self) -> Vec<Option<char>> {
        let mut digits = self.code.chars();
        (0..self.length).map(|_| digits.next()).collect()
    }

    /// The cell the next character goes into, for showing a caret.
    #[must_use]
    pub fn next_cell(&self) -> Option<usize> {
        (!self.is_complete()).then_some(self.entered())
    }

    fn cleaned(&self, text: &str) -> String {
        text.chars()
            .filter(|ch| self.accept.takes(*ch))
            .take(self.length)
            .collect()
    }

    /// Takes text as the code, keeping only the characters a code is made of.
    fn set(&mut self, text: &str, source: Source, reason: Reason, gate: Gate) -> bool {
        let code = self.cleaned(text);
        self.commit(code, source, reason, gate)
    }
}

impl OtpInput {
    /// A key at the caret: typing over a cell, deleting one, or moving between them.
    fn on_key(&mut self, key: Key, gate: Gate) -> Reaction<Part> {
        let mut cells: Vec<char> = self.code.chars().collect();
        let (back, forward) = if self.dir == Dir::Rtl {
            (Key::ArrowRight, Key::ArrowLeft)
        } else {
            (Key::ArrowLeft, Key::ArrowRight)
        };
        match key {
            Key::Char(ch) if self.accept.takes(ch) => {
                // Typing replaces the character under the caret, or adds one.
                if let Some(cell) = cells.get_mut(self.slot) {
                    *cell = ch;
                } else if cells.len() < self.length {
                    cells.push(ch);
                }
                let at = self.slot;
                if self.commit(
                    cells.into_iter().collect(),
                    Source::User,
                    Reason::Text,
                    gate,
                ) {
                    self.slot = (at.saturating_add(1)).min(self.end());
                }
                Reaction::handled()
            }
            Key::Backspace => {
                let at = if self.slot < cells.len() {
                    self.slot
                } else {
                    cells.len().saturating_sub(1)
                };
                if !cells.is_empty() {
                    cells.remove(at);
                    if self.commit(
                        cells.into_iter().collect(),
                        Source::User,
                        Reason::Text,
                        gate,
                    ) {
                        self.slot = at.min(self.end());
                    }
                }
                Reaction::handled()
            }
            k if k == back => {
                self.slot = self.slot.saturating_sub(1);
                Reaction::handled()
            }
            k if k == forward => {
                self.slot = (self.slot.saturating_add(1)).min(self.end());
                Reaction::handled()
            }
            Key::Home => {
                self.slot = 0;
                Reaction::handled()
            }
            Key::End => {
                self.slot = self.end();
                Reaction::handled()
            }
            // Other keys are left to the platform.
            _ => Reaction::default(),
        }
    }
}

impl PartOf for Part {
    type Primitive = OtpInput;
}

impl Primitive for OtpInput {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Input, Part::Cell]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match part {
            Part::Input => Semantics {
                role: Some(Role::TextBox),
                focusable: Focusable::TabStop,
                ..Default::default()
            },
            Part::Cell => Semantics {
                active: item.and_then(|at| at.parse().ok()) == self.active_cell(),
                ..Default::default()
            },
        }
    }
    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Text { value, .. } => {
                self.set(&value, Source::User, Reason::Text, gate);
                self.slot = self.end();
                Reaction::handled()
            }
            // Focus goes after the code, or onto its last character when full.
            Input::Focus {
                part: Part::Input, ..
            } => {
                self.focused = true;
                self.slot = self.end();
                Reaction::default()
            }
            Input::Blur { .. } => {
                self.focused = false;
                Reaction::default()
            }
            Input::Press {
                part: Part::Cell,
                item: Some(at),
                ..
            } => {
                self.slot = at.parse::<usize>().unwrap_or(0).min(self.end());
                Reaction::focusing(Part::Input)
            }
            // A whole code replaces the value wherever the caret is. Spaces and
            // dashes in it are dropped, and a paste with anything else is ignored.
            Input::Paste { text } => {
                let stripped: String = text
                    .chars()
                    .filter(|ch| !ch.is_whitespace() && *ch != '-')
                    .collect();
                if stripped.is_empty() || !stripped.chars().all(|ch| self.accept.takes(ch)) {
                    return Reaction::handled();
                }
                let code: String = stripped.chars().take(self.length).collect();
                if self.commit(code, Source::User, Reason::Text, gate) {
                    self.slot = self.end();
                }
                Reaction::handled()
            }
            Input::Key { key, mods, .. } if !mods.ctrl && !mods.alt => self.on_key(key, gate),
            _ => Reaction::default(),
        }
    }

    fn text(&self, _: Part, _: Option<&str>) -> Option<String> {
        Some(self.code.clone())
    }

    fn value(&self) -> Vec<String> {
        vec![self.code.clone()]
    }

    fn set_value(&mut self, value: Vec<String>) {
        self.code = self.cleaned(value.first().map(String::as_str).unwrap_or_default());
        self.slot = self.end();
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "otp",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let text = match action {
            "clear" => "",
            "set" => item.ok_or(ActError::MissingItem { action: "set" })?,
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        if self.set(text, Source::Agent, Reason::Action, gate) {
            Ok(())
        } else {
            Err(ActError::Refused)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(otp: &mut OtpInput, text: &str) {
        otp.send(
            Input::Text {
                part: Part::Input,
                item: None,
                value: text.into(),
            },
            &mut |_| true,
        );
    }

    #[test]
    fn digits_fill_the_cells_in_order() {
        let mut otp = OtpInput::new(4);
        typed(&mut otp, "12");
        assert_eq!(otp.cells(), [Some('1'), Some('2'), None, None]);
        assert_eq!(otp.next_cell(), Some(2));
        assert!(!otp.is_complete());
    }

    #[test]
    fn a_pasted_code_loses_its_separators_and_anything_past_the_length() {
        let mut otp = OtpInput::default();
        typed(&mut otp, "123-456 789");
        assert_eq!(otp.value(), ["123456"]);
        assert!(otp.is_complete());
        assert_eq!(otp.next_cell(), None);
    }

    #[test]
    fn deleting_text_empties_the_last_cells() {
        let mut otp = OtpInput::new(4);
        typed(&mut otp, "1234");
        typed(&mut otp, "123");
        assert_eq!(otp.cells()[3], None);
        assert_eq!(otp.text(Part::Input, None).as_deref(), Some("123"));
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(OtpInput::default, &[]);
    }

    #[test]
    fn a_code_of_wider_characters_is_counted_in_characters() {
        let mut otp = OtpInput::new(2).accepting(Accept::Custom(|ch| ch == 'é'));
        otp.set_value(vec!["é".into()]);
        assert!(!otp.is_complete());
        assert_eq!(otp.next_cell(), Some(1));
        otp.set_value(vec!["éé".into()]);
        assert!(otp.is_complete());
    }
}
