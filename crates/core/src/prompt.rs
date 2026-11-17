use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::input_history::InputHistory;
use crate::keys::Key;
use crate::primitive::{Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Input,
}

const ACTIONS: &[&str] = &["submit"];

/// A field for sending one entry after another, such as a chat box or a command
/// line. Enter offers the entry to the owner and clears the field. ArrowUp and
/// ArrowDown recall earlier entries. It keeps no value of its own.
#[derive(Clone, Debug, Default)]
pub struct Prompt {
    text: String,
    history: InputHistory,
}

impl Prompt {
    fn submit(&mut self, text: &str, source: Source, reason: Reason, gate: Gate) -> bool {
        let sent = gate(&ValueChange {
            value: vec![text.to_string()],
            source,
            reason,
        });
        if sent {
            self.history.push(text);
        }
        sent
    }
}

impl PartOf for Part {
    type Primitive = Prompt;
}

impl Primitive for Prompt {
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
                self.text = value;
                Reaction::handled()
            }
            Input::Key {
                key: Key::Enter, ..
            } if !self.text.trim().is_empty() => {
                let text = self.text.clone();
                if self.submit(&text, Source::User, Reason::Text, gate) {
                    self.text.clear();
                }
                Reaction::handled()
            }
            Input::Key {
                key: Key::ArrowUp, ..
            } => match self.history.older(&self.text).map(str::to_string) {
                Some(entry) => {
                    self.text = entry;
                    Reaction::handled()
                }
                None => Reaction::default(),
            },
            Input::Key {
                key: Key::ArrowDown,
                ..
            } => match self.history.newer().map(str::to_string) {
                Some(entry) => {
                    self.text = entry;
                    Reaction::handled()
                }
                None => Reaction::default(),
            },
            _ => Reaction::default(),
        }
    }

    fn text(&self, _: Part, _: Option<&str>) -> Option<String> {
        Some(self.text.clone())
    }

    fn value(&self) -> Vec<String> {
        Vec::new()
    }

    fn set_value(&mut self, _: Vec<String>) {}

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "prompt",
            label: label.to_string(),
            value: Vec::new(),
            expanded: None,
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        if action != "submit" {
            return Err(ActError::UnknownAction {
                action: action.to_string(),
                valid: ACTIONS,
            });
        }
        let text = item.ok_or(ActError::MissingItem { action: "submit" })?;
        if self.submit(text, Source::Agent, Reason::Action, gate) {
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

    fn typed(prompt: &mut Prompt, text: &str) {
        prompt.send(
            Input::Text {
                part: Part::Input,
                item: None,
                value: text.into(),
            },
            &mut |_| true,
        );
    }

    fn key(prompt: &mut Prompt, key: Key, sent: &mut Vec<String>) -> bool {
        let reaction = prompt.send(
            Input::Key {
                key,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |change| {
                sent.extend(change.value.iter().cloned());
                true
            },
        );
        reaction.handled
    }

    fn shown(prompt: &Prompt) -> String {
        prompt.text(Part::Input, None).unwrap()
    }

    #[test]
    fn enter_sends_the_entry_and_empties_the_field() {
        let mut prompt = Prompt::default();
        let mut sent = Vec::new();
        typed(&mut prompt, "build");
        key(&mut prompt, Key::Enter, &mut sent);
        assert_eq!(sent, ["build"]);
        assert_eq!(shown(&prompt), "");
        assert!(!key(&mut prompt, Key::Enter, &mut sent), "nothing to send");
    }

    #[test]
    fn arrows_recall_earlier_entries_and_return_to_what_was_being_typed() {
        let mut prompt = Prompt::default();
        let mut sent = Vec::new();
        for entry in ["build", "test"] {
            typed(&mut prompt, entry);
            key(&mut prompt, Key::Enter, &mut sent);
        }
        typed(&mut prompt, "dep");
        key(&mut prompt, Key::ArrowUp, &mut sent);
        assert_eq!(shown(&prompt), "test");
        key(&mut prompt, Key::ArrowUp, &mut sent);
        assert_eq!(shown(&prompt), "build");
        assert!(
            !key(&mut prompt, Key::ArrowUp, &mut sent),
            "the oldest entry is the end"
        );
        key(&mut prompt, Key::ArrowDown, &mut sent);
        key(&mut prompt, Key::ArrowDown, &mut sent);
        assert_eq!(shown(&prompt), "dep");
    }

    #[test]
    fn a_refused_entry_stays_in_the_field_and_out_of_the_history() {
        let mut prompt = Prompt::default();
        typed(&mut prompt, "rm -rf");
        prompt.send(
            Input::Key {
                key: Key::Enter,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| false,
        );
        assert_eq!(shown(&prompt), "rm -rf");
        typed(&mut prompt, "");
        assert!(!key(&mut prompt, Key::ArrowUp, &mut Vec::new()));
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(Prompt::default, &[]);
    }
}
