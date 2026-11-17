use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::Key;
use crate::primitive::{
    Focusable, Input, Item, PartOf, Primitive, Reaction, Relations, Role, Semantics,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Text,
    /// Hidden as typed, with a button to show it.
    Password,
    /// Several lines. Enter adds a line and Ctrl+Enter submits.
    Multiline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    Label,
    Input,
    /// Help text that explains the field.
    Description,
    /// What is wrong with the value. Fill it from `error`.
    Error,
    /// Empties the field. Hidden while the field is empty.
    Clear,
    /// Shows or hides a password.
    Reveal,
}

const ACTIONS: &[&str] = &["set", "clear", "submit", "reveal", "conceal"];

/// A labelled text field with help text and an error. It covers a plain input, a
/// password input and a multi-line one. The value is the text. Each edit is offered
/// to the gate with `Reason::Text`, and a submit with `Reason::Keyboard`.
#[derive(Clone, Debug, Default)]
pub struct TextField {
    kind: Kind,
    text: String,
    required: bool,
    disabled: bool,
    /// Returns what is wrong with a value, if anything.
    validate: Option<fn(&str) -> Option<String>>,
    /// Whether to show a problem yet: after the field has been left or submitted.
    judged: bool,
    revealed: bool,
    focused: Option<Part>,
}

impl TextField {
    pub fn new(kind: Kind) -> Self {
        TextField {
            kind,
            ..Default::default()
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    pub fn validate(mut self, check: fn(&str) -> Option<String>) -> Self {
        self.validate = Some(check);
        self
    }

    /// What is wrong with the value, once the person has left the field or tried to
    /// submit it. Nobody is told off while still typing.
    pub fn error(&self) -> Option<String> {
        if !self.judged {
            return None;
        }
        if self.required && self.text.trim().is_empty() {
            return Some("This field is required.".to_string());
        }
        self.validate.and_then(|check| check(&self.text))
    }

    pub fn is_revealed(&self) -> bool {
        self.revealed
    }

    fn write(&mut self, text: String, source: Source, reason: Reason, gate: Gate) -> bool {
        if text == self.text {
            return false;
        }
        let allowed = gate(&ValueChange {
            value: vec![text.clone()],
            source,
            reason,
        });
        if allowed {
            self.text = text;
        }
        allowed
    }

    /// Offers the value as finished. A value with something wrong is not offered.
    fn submit(&mut self, source: Source, gate: Gate) -> bool {
        self.judged = true;
        self.error().is_none()
            && gate(&ValueChange {
                value: vec![self.text.clone()],
                source,
                reason: Reason::Keyboard,
            })
    }
}

impl PartOf for Part {
    type Primitive = TextField;
}

impl Primitive for TextField {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[
            Part::Root,
            Part::Label,
            Part::Input,
            Part::Description,
            Part::Error,
            Part::Clear,
            Part::Reveal,
        ]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn named_part(&self) -> Option<Part> {
        Some(Part::Input)
    }

    fn relations(part: Part) -> Relations<Part> {
        match part {
            Part::Input => Relations {
                labelled_by: Some(Part::Label),
                described_by: Some(Part::Description),
                controls: None,
            },
            _ => Relations::default(),
        }
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, _: Option<&str>) -> Semantics {
        let button = |hidden: bool| Semantics {
            role: Some(Role::Button),
            hidden,
            focusable: if hidden || self.disabled {
                Focusable::No
            } else {
                Focusable::TabStop
            },
            ..Default::default()
        };
        match part {
            Part::Root => Semantics {
                role: Some(Role::Group),
                disabled: self.disabled,
                ..Default::default()
            },
            Part::Label | Part::Description => Semantics::default(),
            Part::Input => Semantics {
                role: Some(Role::TextBox),
                disabled: self.disabled,
                invalid: self.error().is_some(),
                obscured: self.kind == Kind::Password && !self.revealed,
                focusable: if self.disabled {
                    Focusable::No
                } else {
                    Focusable::TabStop
                },
                ..Default::default()
            },
            Part::Error => Semantics {
                role: Some(Role::Alert),
                hidden: self.error().is_none(),
                ..Default::default()
            },
            Part::Clear => button(self.text.is_empty()),
            Part::Reveal => Semantics {
                pressed: Some(self.revealed),
                ..button(self.kind != Kind::Password)
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        if self.disabled {
            return Reaction::default();
        }
        match input {
            Input::Focus { part, .. } => {
                self.focused = Some(part);
                Reaction::default()
            }
            // Pressing the label puts the caret in the field, as a real label does.
            Input::Press {
                part: Part::Label, ..
            } => Reaction::focusing(Part::Input),
            Input::Press {
                part: Part::Clear, ..
            } => {
                self.write(String::new(), Source::User, Reason::Press, gate);
                Reaction::focusing(Part::Input)
            }
            Input::Press {
                part: Part::Reveal, ..
            } => {
                self.revealed = !self.revealed;
                Reaction::handled()
            }
            Input::Text {
                part: Part::Input,
                value,
                ..
            } => {
                self.write(value, Source::User, Reason::Text, gate);
                Reaction::handled()
            }
            Input::Blur {
                part: Part::Input, ..
            } => {
                self.judged = true;
                // A password goes back into hiding when the field is left.
                self.revealed = false;
                Reaction::default()
            }
            Input::Key {
                key: Key::Enter | Key::Space,
                mods,
                ..
            } if matches!(self.focused, Some(Part::Clear | Part::Reveal)) => {
                let part = self.focused.unwrap_or(Part::Clear);
                self.send(
                    Input::Press {
                        part,
                        item: None,
                        mods,
                    },
                    gate,
                )
            }
            Input::Key {
                key: Key::Enter,
                mods,
                ..
            } if self.focused == Some(Part::Input) => {
                if self.kind == Kind::Multiline && !mods.ctrl {
                    return Reaction::default();
                }
                self.submit(Source::User, gate);
                Reaction::handled()
            }
            _ => Reaction::default(),
        }
    }

    fn text(&self, part: Part, _: Option<&str>) -> Option<String> {
        (part == Part::Input).then(|| self.text.clone())
    }

    fn value(&self) -> Vec<String> {
        vec![self.text.clone()]
    }

    fn set_value(&mut self, value: Vec<String>) {
        if let Some(text) = value.into_iter().next() {
            self.text = text;
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "text-field",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let text = match action {
            "set" => item
                .ok_or(ActError::MissingItem { action: "set" })?
                .to_string(),
            "clear" => String::new(),
            "submit" => {
                return if !self.disabled && self.submit(Source::Agent, gate) {
                    Ok(())
                } else {
                    Err(ActError::Refused)
                };
            }
            "reveal" | "conceal" => {
                self.revealed = action == "reveal" && self.kind == Kind::Password;
                return Ok(());
            }
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        if self.disabled {
            return Err(ActError::Refused);
        }
        if text == self.text || self.write(text, Source::Agent, Reason::Action, gate) {
            Ok(())
        } else {
            Err(ActError::Refused)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Modifiers;

    const NONE: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
    };

    fn typed(field: &mut TextField, text: &str) {
        field.send(
            Input::Text {
                part: Part::Input,
                item: None,
                value: text.into(),
            },
            &mut |_| true,
        );
    }

    fn enter(field: &mut TextField, mods: Modifiers) -> (bool, Vec<Reason>) {
        let mut seen = Vec::new();
        field.send(
            Input::Focus {
                part: Part::Input,
                item: None,
            },
            &mut |_| true,
        );
        let reaction = field.send(
            Input::Key {
                key: Key::Enter,
                mods,
                now_ms: 0,
            },
            &mut |change| {
                seen.push(change.reason);
                true
            },
        );
        (reaction.handled, seen)
    }

    #[test]
    fn nobody_is_told_off_until_they_leave_the_field() {
        let mut email = TextField::new(Kind::Text)
            .required()
            .validate(|text| (!text.contains('@')).then(|| "Enter an email address.".to_string()));
        typed(&mut email, "ada");
        assert_eq!(email.error(), None);
        assert!(email.semantics(Part::Error, None).hidden);
        email.send(
            Input::Blur {
                part: Part::Input,
                item: None,
            },
            &mut |_| true,
        );
        assert_eq!(email.error().as_deref(), Some("Enter an email address."));
        assert!(email.semantics(Part::Input, None).invalid);
        typed(&mut email, "ada@example.com");
        assert_eq!(email.error(), None);
    }

    #[test]
    fn enter_submits_a_good_value_and_holds_back_a_bad_one() {
        let mut name = TextField::new(Kind::Text).required();
        assert_eq!(
            enter(&mut name, NONE),
            (true, vec![]),
            "empty and required, so nothing is offered"
        );
        assert_eq!(name.error().as_deref(), Some("This field is required."));
        typed(&mut name, "Ada");
        assert_eq!(enter(&mut name, NONE).1, [Reason::Keyboard]);
    }

    #[test]
    fn enter_adds_a_line_in_a_multi_line_field_and_ctrl_enter_submits() {
        let mut note = TextField::new(Kind::Multiline);
        typed(&mut note, "hello");
        assert_eq!(enter(&mut note, NONE), (false, vec![]));
        assert_eq!(
            enter(&mut note, Modifiers { ctrl: true, ..NONE }),
            (true, vec![Reason::Keyboard])
        );
    }

    #[test]
    fn a_password_is_hidden_until_revealed_and_hides_again_when_left() {
        let mut password = TextField::new(Kind::Password);
        assert!(password.semantics(Part::Input, None).obscured);
        password.send(
            Input::Press {
                part: Part::Reveal,
                item: None,
                mods: NONE,
            },
            &mut |_| true,
        );
        assert!(!password.semantics(Part::Input, None).obscured);
        assert_eq!(password.semantics(Part::Reveal, None).pressed, Some(true));
        password.send(
            Input::Blur {
                part: Part::Input,
                item: None,
            },
            &mut |_| true,
        );
        assert!(password.semantics(Part::Input, None).obscured);
        assert!(
            TextField::new(Kind::Text)
                .semantics(Part::Reveal, None)
                .hidden
        );
    }

    #[test]
    fn the_clear_button_empties_the_field_and_returns_focus_to_it() {
        let mut search = TextField::new(Kind::Text);
        assert!(search.semantics(Part::Clear, None).hidden);
        typed(&mut search, "rust");
        let cleared = search.send(
            Input::Press {
                part: Part::Clear,
                item: None,
                mods: NONE,
            },
            &mut |_| true,
        );
        assert_eq!(search.value(), [""]);
        assert_eq!(cleared.focus.unwrap().part, Part::Input);
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(|| TextField::new(Kind::Text), &[]);
        crate::testing::assert_conforms(|| TextField::new(Kind::Password), &[]);
    }
}
