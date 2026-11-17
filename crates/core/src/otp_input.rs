use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::primitive::{Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The one real text field. Draw the code's cells from `cells`.
    Input,
}

const ACTIONS: &[&str] = &["set", "clear"];

/// A one-time code of a fixed number of digits. Anything that is not a digit is
/// dropped, so a pasted "123 456" or "123-456" works.
#[derive(Clone, Debug)]
pub struct OtpInput {
    length: usize,
    code: String,
}

impl Default for OtpInput {
    fn default() -> Self {
        OtpInput::new(6)
    }
}

impl OtpInput {
    pub fn new(length: usize) -> Self {
        OtpInput {
            length,
            code: String::new(),
        }
    }

    pub fn is_complete(&self) -> bool {
        self.code.len() == self.length
    }

    /// One entry per cell: the digit in it, if any.
    pub fn cells(&self) -> Vec<Option<char>> {
        let mut digits = self.code.chars();
        (0..self.length).map(|_| digits.next()).collect()
    }

    /// The cell the next digit goes into, for showing a caret.
    pub fn next_cell(&self) -> Option<usize> {
        (!self.is_complete()).then_some(self.code.len())
    }

    fn cleaned(&self, text: &str) -> String {
        text.chars()
            .filter(char::is_ascii_digit)
            .take(self.length)
            .collect()
    }

    fn set(&mut self, text: &str, source: Source, reason: Reason, gate: Gate) -> bool {
        let code = self.cleaned(text);
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
}

impl PartOf for Part {
    type Primitive = OtpInput;
}

impl Primitive for OtpInput {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Input]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, _: Part, _: Option<&str>) -> Semantics {
        Semantics {
            role: Some(Role::TextBox),
            focusable: Focusable::TabStop,
            ..Default::default()
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Text { value, .. } => {
                self.set(&value, Source::User, Reason::Text, gate);
                Reaction::handled()
            }
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
}
