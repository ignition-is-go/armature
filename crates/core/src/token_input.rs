use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::Key;
use crate::primitive::{Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    /// The text field tokens are typed into.
    Input,
    /// One token. Render one per entry in the value; pressing it removes it.
    Token,
}

const ACTIONS: &[&str] = &["add", "remove", "clear"];

/// A field that turns what is typed into a list of tokens, such as tags or
/// recipients. Enter or a comma adds a token and Backspace in an empty field removes
/// the last. The value is the tokens.
#[derive(Clone, Debug, Default)]
pub struct TokenInput {
    tokens: Vec<String>,
    draft: String,
}

impl TokenInput {
    fn set(&mut self, tokens: Vec<String>, source: Source, reason: Reason, gate: Gate) -> bool {
        if tokens == self.tokens {
            return true;
        }
        let allowed = gate(&ValueChange {
            value: tokens.clone(),
            source,
            reason,
        });
        if allowed {
            self.tokens = tokens;
        }
        allowed
    }

    fn with(&self, token: &str) -> Vec<String> {
        let token = token.trim();
        let mut tokens = self.tokens.clone();
        if !token.is_empty() && !tokens.iter().any(|existing| existing == token) {
            tokens.push(token.to_string());
        }
        tokens
    }

    fn without(&self, token: &str) -> Vec<String> {
        self.tokens
            .iter()
            .filter(|existing| *existing != token)
            .cloned()
            .collect()
    }

    /// Turns the draft into a token, if the owner allows, and empties the field.
    fn commit_draft(&mut self, reason: Reason, gate: Gate) {
        let tokens = self.with(&self.draft);
        if self.set(tokens, Source::User, reason, gate) {
            self.draft.clear();
        }
    }
}

impl PartOf for Part {
    type Primitive = TokenInput;
}

impl Primitive for TokenInput {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Input, Part::Token]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    /// The field is what a person types into, so it carries the name.
    fn named_part(&self) -> Option<Part> {
        Some(Part::Input)
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match part {
            Part::Root => Semantics {
                role: Some(Role::Group),
                ..Default::default()
            },
            Part::Input => Semantics {
                role: Some(Role::TextBox),
                focusable: Focusable::TabStop,
                ..Default::default()
            },
            Part::Token => Semantics {
                role: Some(Role::Button),
                hidden: !item
                    .is_some_and(|token| self.tokens.iter().any(|existing| existing == token)),
                focusable: Focusable::Programmatic,
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            // A comma ends a token, so pasting "a, b, c" adds three.
            Input::Text { value, .. } if value.contains(',') => {
                let mut pieces: Vec<&str> = value.split(',').collect();
                let rest = pieces.pop().unwrap_or_default().to_string();
                let mut tokens = self.tokens.clone();
                for piece in pieces {
                    tokens = Self {
                        tokens,
                        draft: String::new(),
                    }
                    .with(piece);
                }
                self.draft = if self.set(tokens, Source::User, Reason::Text, gate) {
                    rest
                } else {
                    value
                };
                Reaction::handled()
            }
            Input::Text { value, .. } => {
                self.draft = value;
                Reaction::handled()
            }
            Input::Key {
                key: Key::Enter, ..
            } if !self.draft.trim().is_empty() => {
                self.commit_draft(Reason::Keyboard, gate);
                Reaction::handled()
            }
            Input::Key {
                key: Key::Backspace,
                ..
            } if self.draft.is_empty() && !self.tokens.is_empty() => {
                let tokens = self.tokens[..self.tokens.len().saturating_sub(1)].to_vec();
                self.set(tokens, Source::User, Reason::Keyboard, gate);
                Reaction::handled()
            }
            Input::Press {
                part: Part::Token,
                item: Some(token),
                ..
            } => {
                let tokens = self.without(&token);
                self.set(tokens, Source::User, Reason::Press, gate);
                Reaction::focusing(Part::Input)
            }
            _ => Reaction::default(),
        }
    }

    fn text(&self, part: Part, _: Option<&str>) -> Option<String> {
        (part == Part::Input).then(|| self.draft.clone())
    }

    fn value(&self) -> Vec<String> {
        self.tokens.clone()
    }

    fn set_value(&mut self, value: Vec<String>) {
        self.tokens = Vec::new();
        for token in value {
            self.tokens = self.with(&token);
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .tokens
            .iter()
            .map(|token| SnapshotItem {
                key: token.clone(),
                label: token.clone(),
                disabled: false,
                selected: true,
            })
            .collect();
        Snapshot {
            name: "tokens",
            label: label.to_string(),
            value: self.tokens.clone(),
            expanded: None,
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let tokens = match action {
            "clear" => Vec::new(),
            "add" => self.with(item.ok_or(ActError::MissingItem { action: "add" })?),
            "remove" => {
                let token = item.ok_or(ActError::MissingItem { action: "remove" })?;
                if !self.tokens.iter().any(|existing| existing == token) {
                    return Err(ActError::UnknownItem(token.to_string()));
                }
                self.without(token)
            }
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        if self.set(tokens, Source::Agent, Reason::Action, gate) {
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

    fn typed(tokens: &mut TokenInput, text: &str) {
        tokens.send(
            Input::Text {
                part: Part::Input,
                item: None,
                value: text.into(),
            },
            &mut |_| true,
        );
    }

    fn key(tokens: &mut TokenInput, key: Key) -> bool {
        tokens
            .send(
                Input::Key {
                    key,
                    mods: Modifiers::default(),
                    now_ms: 0,
                },
                &mut |_| true,
            )
            .handled
    }

    #[test]
    fn enter_turns_what_was_typed_into_a_token_and_empties_the_field() {
        let mut tokens = TokenInput::default();
        typed(&mut tokens, "  rust ");
        assert!(key(&mut tokens, Key::Enter));
        assert_eq!(tokens.value(), ["rust"]);
        assert_eq!(tokens.text(Part::Input, None).as_deref(), Some(""));
        assert!(
            !key(&mut tokens, Key::Enter),
            "an empty field leaves Enter alone"
        );
    }

    #[test]
    fn commas_split_pasted_text_into_tokens_and_keep_the_remainder() {
        let mut tokens = TokenInput::default();
        typed(&mut tokens, "a, b,c, d");
        assert_eq!(tokens.value(), ["a", "b", "c"]);
        assert_eq!(tokens.text(Part::Input, None).as_deref(), Some(" d"));
    }

    #[test]
    fn a_token_is_only_added_once() {
        let mut tokens = TokenInput::default();
        typed(&mut tokens, "a,a,");
        assert_eq!(tokens.value(), ["a"]);
    }

    #[test]
    fn backspace_in_an_empty_field_removes_the_last_token() {
        let mut tokens = TokenInput::default();
        typed(&mut tokens, "a,b,");
        assert!(key(&mut tokens, Key::Backspace));
        assert_eq!(tokens.value(), ["a"]);
        typed(&mut tokens, "x");
        assert!(
            !key(&mut tokens, Key::Backspace),
            "with text in the field, Backspace edits the text"
        );
    }

    #[test]
    fn pressing_a_token_removes_it_and_returns_focus_to_the_field() {
        let mut tokens = TokenInput::default();
        typed(&mut tokens, "a,b,");
        let pressed = tokens.send(
            Input::Press {
                part: Part::Token,
                item: Some("a".into()),
                mods: Modifiers::default(),
            },
            &mut |_| true,
        );
        assert_eq!(tokens.value(), ["b"]);
        assert_eq!(pressed.focus.unwrap().part, Part::Input);
        assert!(tokens.semantics(Part::Token, Some("a")).hidden);
    }

    #[test]
    fn a_refused_token_stays_in_the_field_to_be_corrected() {
        let mut tokens = TokenInput::default();
        typed(&mut tokens, "nope");
        tokens.send(
            Input::Key {
                key: Key::Enter,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| false,
        );
        assert!(tokens.value().is_empty());
        assert_eq!(tokens.text(Part::Input, None).as_deref(), Some("nope"));
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(TokenInput::default, &[]);
    }
}
