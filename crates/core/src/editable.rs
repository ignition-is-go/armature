use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::inline_edit::InlineEdit;
use crate::keys::Key;
use crate::primitive::{Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The value as read. Pressing it, or Enter or F2 on it, starts editing. Typing a
    /// character starts editing with that character.
    Preview,
    /// The field shown while editing.
    Input,
}

const ACTIONS: &[&str] = &["set"];

/// Text edited in place. Enter, Tab or leaving the field saves; Escape puts the old
/// text back. The value is the text.
#[derive(Clone, Debug, Default)]
pub struct Editable {
    edit: InlineEdit,
}

impl Editable {
    #[must_use]
    pub fn new(value: &str) -> Self {
        Self {
            edit: InlineEdit::new(value),
        }
    }

    #[must_use]
    pub const fn is_editing(&self) -> bool {
        self.edit.is_editing()
    }

    /// Saves what was typed if the owner accepts it. A refusal keeps the field open.
    fn save(&mut self, gate: Gate) -> bool {
        let Some(draft) = self.edit.draft().map(str::to_string) else {
            return true;
        };
        if draft == self.edit.value() {
            self.edit.cancel();
            return true;
        }
        let allowed = gate(&ValueChange {
            value: vec![draft],
            source: Source::User,
            reason: Reason::Text,
        });
        if allowed {
            self.edit.submit(|_| true);
        }
        allowed
    }
}

impl PartOf for Part {
    type Primitive = Editable;
}

impl Primitive for Editable {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Preview, Part::Input]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    /// The field is what a person types into, so it carries the name while editing;
    /// the preview carries it otherwise.
    fn named_part(&self) -> Option<Part> {
        Some(if self.is_editing() {
            Part::Input
        } else {
            Part::Preview
        })
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, _: Option<&str>) -> Semantics {
        let editing = self.is_editing();
        match part {
            Part::Preview => Semantics {
                role: Some(Role::Button),
                hidden: editing,
                focusable: if editing {
                    Focusable::No
                } else {
                    Focusable::TabStop
                },
                ..Default::default()
            },
            Part::Input => Semantics {
                role: Some(Role::TextBox),
                hidden: !editing,
                focusable: if editing {
                    Focusable::TabStop
                } else {
                    Focusable::No
                },
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match (input, self.is_editing()) {
            (
                Input::Press {
                    part: Part::Preview,
                    ..
                }
                | Input::Key {
                    key: Key::Enter | Key::F2,
                    ..
                },
                false,
            ) => {
                self.edit.start();
                Reaction::focusing(Part::Input)
            }
            // Typing replaces the text, as in a spreadsheet cell.
            (
                Input::Key {
                    key: Key::Char(ch),
                    mods,
                    ..
                },
                false,
            ) if !mods.ctrl && !mods.alt => {
                self.edit.start();
                self.edit.input(&ch.to_string());
                Reaction::focusing(Part::Input)
            }
            (Input::Text { value, .. }, true) => {
                self.edit.input(&value);
                Reaction::handled()
            }
            (
                Input::Key {
                    key: Key::Enter, ..
                },
                true,
            ) => {
                if self.save(gate) {
                    Reaction::focusing(Part::Preview)
                } else {
                    Reaction::handled()
                }
            }
            // Tab saves and is left to move focus on.
            (
                Input::Key { key: Key::Tab, .. }
                | Input::Blur {
                    part: Part::Input, ..
                },
                true,
            ) => {
                self.save(gate);
                Reaction::default()
            }
            (
                Input::Key {
                    key: Key::Escape, ..
                },
                true,
            ) => {
                self.edit.cancel();
                Reaction::focusing(Part::Preview)
            }
            _ => Reaction::default(),
        }
    }

    fn text(&self, part: Part, _: Option<&str>) -> Option<String> {
        (part == Part::Input).then(|| {
            self.edit
                .draft()
                .unwrap_or_else(|| self.edit.value())
                .to_string()
        })
    }

    fn value(&self) -> Vec<String> {
        vec![self.edit.value().to_string()]
    }

    fn set_value(&mut self, value: Vec<String>) {
        if let Some(text) = value.first() {
            self.edit = InlineEdit::new(text);
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "editable",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        if action != "set" {
            return Err(ActError::UnknownAction {
                action: action.to_string(),
                valid: ACTIONS,
            });
        }
        let text = item.ok_or(ActError::MissingItem { action: "set" })?;
        if text == self.edit.value() {
            return Ok(());
        }
        if !gate(&ValueChange {
            value: vec![text.to_string()],
            source: Source::Agent,
            reason: Reason::Action,
        }) {
            return Err(ActError::Refused);
        }
        self.edit = InlineEdit::new(text);
        Ok(())
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
    use crate::keys::Modifiers;

    fn key(editable: &mut Editable, key: Key, gate: Gate) -> Reaction<Part> {
        editable.send(
            Input::Key {
                key,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            gate,
        )
    }

    fn typed(editable: &mut Editable, text: &str) {
        editable.send(
            Input::Text {
                part: Part::Input,
                item: None,
                value: text.into(),
            },
            &mut |_| true,
        );
    }

    #[test]
    fn f2_opens_the_field_on_the_current_text_and_enter_saves() {
        let mut title = Editable::new("Untitled");
        let opened = key(&mut title, Key::F2, &mut |_| true);
        assert_eq!(opened.focus.unwrap().part, Part::Input);
        assert_eq!(title.text(Part::Input, None).as_deref(), Some("Untitled"));
        typed(&mut title, "Roadmap");
        let saved = key(&mut title, Key::Enter, &mut |_| true);
        assert_eq!(title.value(), ["Roadmap"]);
        assert_eq!(saved.focus.unwrap().part, Part::Preview);
        assert!(title.semantics(Part::Input, None).hidden);
    }

    #[test]
    fn typing_on_the_preview_replaces_the_text() {
        let mut title = Editable::new("Untitled");
        key(&mut title, Key::Char('R'), &mut |_| true);
        assert_eq!(title.text(Part::Input, None).as_deref(), Some("R"));
    }

    #[test]
    fn escape_puts_the_old_text_back() {
        let mut title = Editable::new("Untitled");
        key(&mut title, Key::Enter, &mut |_| true);
        typed(&mut title, "Scrapped");
        key(&mut title, Key::Escape, &mut |_| true);
        assert_eq!(title.value(), ["Untitled"]);
        assert!(!title.is_editing());
    }

    #[test]
    fn leaving_the_field_saves_and_a_refusal_keeps_it_open() {
        let mut title = Editable::new("Untitled");
        key(&mut title, Key::Enter, &mut |_| true);
        typed(&mut title, "Draft");
        title.send(
            Input::Blur {
                part: Part::Input,
                item: None,
            },
            &mut |_| true,
        );
        assert_eq!(title.value(), ["Draft"]);

        key(&mut title, Key::Enter, &mut |_| true);
        typed(&mut title, "");
        key(&mut title, Key::Enter, &mut |_| false);
        assert!(
            title.is_editing(),
            "the owner refused, so it can be corrected"
        );
        assert_eq!(title.value(), ["Draft"]);
    }

    #[test]
    fn saving_unchanged_text_asks_nobody() {
        let mut title = Editable::new("Untitled");
        key(&mut title, Key::Enter, &mut |_| true);
        let mut asked = false;
        key(&mut title, Key::Enter, &mut |_| {
            asked = true;
            true
        });
        assert!(!asked && !title.is_editing());
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(|| Editable::new("Untitled"), &[]);
    }
}
