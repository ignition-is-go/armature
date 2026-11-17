use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::{Dir, Key};
use crate::primitive::{Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics};
use crate::token_text::{
    Segment, Token, from_text, line_boundary, normalize, offset_text, replace_range, slice_value,
    to_label, to_text, token_at, value_length, word_boundary,
};
use crate::trigger_match::{TriggerMatch, TriggerRule, find_trigger_match};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The editor. It takes focus and the keys. Draw its content from `segments`
    /// with the caret at `caret`.
    Root,
    /// One token, keyed by its id. Pressing it puts the caret after it.
    Token,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// Whether Enter starts a new line. When it does not, Enter is left to the
    /// owner, to send what was written.
    pub multiline: bool,
    pub read_only: bool,
    pub dir: Dir,
}

const ACTIONS: &[&str] = &["set", "insert", "remove_token", "clear"];

/// Text with tokens set into it, edited at a caret: a prompt that mentions people
/// and files, say. A token is one step for the caret and goes whole when deleted.
/// The value is the content written as text, each token as `[@label](kind:value)`.
#[derive(Clone, Debug, Default)]
pub struct TokenEditor {
    opts: Options,
    value: Vec<Segment>,
    /// Where typing goes, counted in characters and tokens.
    caret: usize,
}

impl TokenEditor {
    pub fn new(opts: Options) -> Self {
        TokenEditor {
            opts,
            ..Default::default()
        }
    }

    /// The content in order: runs of text, and tokens.
    pub fn segments(&self) -> &[Segment] {
        &self.value
    }

    pub fn caret(&self) -> usize {
        self.caret
    }

    /// The content as a person reads it, with each token as its label.
    pub fn label(&self) -> String {
        to_label(&self.value)
    }

    /// The trigger being typed just before the caret, such as `@al`, for offering
    /// suggestions. Tokens end a query.
    pub fn trigger(&self, rules: &[TriggerRule]) -> Option<TriggerMatch> {
        let before: String = offset_text(&self.value).chars().take(self.caret).collect();
        find_trigger_match(&before, 0, rules)
    }

    /// Replaces a trigger and its query with a token and a space, as choosing a
    /// suggestion does. Returns whether the owner allowed it.
    pub fn complete(&mut self, found: &TriggerMatch, token: Token, gate: Gate) -> bool {
        self.replace(
            found.start,
            found.end,
            vec![Segment::Token(token), Segment::Text(" ".into())],
            Source::User,
            Reason::Press,
            gate,
        )
    }

    /// Puts `content` in place of `start..end` and the caret after it.
    fn replace(
        &mut self,
        start: usize,
        end: usize,
        content: Vec<Segment>,
        source: Source,
        reason: Reason,
        gate: Gate,
    ) -> bool {
        let added = value_length(&normalize(content.clone()));
        let next = replace_range(&self.value, start, end, content);
        if next == self.value {
            return true;
        }
        let allowed = gate(&ValueChange {
            value: vec![to_text(&next)],
            source,
            reason,
        });
        if allowed {
            self.value = next;
            self.caret = start + added;
        }
        allowed
    }

    fn typed(&mut self, text: &str, gate: Gate) -> Reaction<Part> {
        self.replace(
            self.caret,
            self.caret,
            vec![Segment::Text(text.to_string())],
            Source::User,
            Reason::Text,
            gate,
        );
        Reaction::handled()
    }

    fn on_key(&mut self, key: Key, by_word: bool, shift: bool, gate: Gate) -> Reaction<Part> {
        let length = value_length(&self.value);
        let (back, forward) = if self.opts.dir == Dir::Rtl {
            (Key::ArrowRight, Key::ArrowLeft)
        } else {
            (Key::ArrowLeft, Key::ArrowRight)
        };
        match key {
            key if key == back || key == forward => {
                let ahead = key == forward;
                self.caret = match (by_word, ahead) {
                    (true, _) => word_boundary(&self.value, self.caret, ahead),
                    (false, true) => (self.caret + 1).min(length),
                    (false, false) => self.caret.saturating_sub(1),
                };
                Reaction::handled()
            }
            Key::Home | Key::End => {
                let text: Vec<char> = offset_text(&self.value).chars().collect();
                self.caret = if key == Key::Home {
                    text[..self.caret]
                        .iter()
                        .rposition(|ch| *ch == '\n')
                        .map_or(0, |at| at + 1)
                } else {
                    text[self.caret..]
                        .iter()
                        .position(|ch| *ch == '\n')
                        .map_or(length, |at| self.caret + at)
                };
                Reaction::handled()
            }
            _ if self.opts.read_only => Reaction::default(),
            Key::Backspace if self.caret > 0 => {
                let start = if by_word {
                    word_boundary(&self.value, self.caret, false)
                } else {
                    self.caret - 1
                };
                self.replace(
                    start,
                    self.caret,
                    Vec::new(),
                    Source::User,
                    Reason::Text,
                    gate,
                );
                Reaction::handled()
            }
            Key::Backspace => Reaction::handled(),
            Key::Char(ch) if !by_word => self.typed(&ch.to_string(), gate),
            Key::Space => self.typed(" ", gate),
            Key::Enter if self.opts.multiline || shift => self.typed("\n", gate),
            _ => Reaction::default(),
        }
    }
}

impl PartOf for Part {
    type Primitive = TokenEditor;
}

impl Primitive for TokenEditor {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Token]
    }

    fn is_item(part: Part) -> bool {
        part == Part::Token
    }

    /// The editor owns its tokens. Items are not used.
    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match part {
            Part::Root => Semantics {
                role: Some(Role::TextBox),
                focusable: Focusable::TabStop,
                ..Default::default()
            },
            // The token just before the caret is the one Backspace would take.
            Part::Token => Semantics {
                active: self
                    .caret
                    .checked_sub(1)
                    .and_then(|at| token_at(&self.value, at))
                    .is_some_and(|token| Some(token.id.as_str()) == item),
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Key { key, mods, .. } => {
                self.on_key(key, mods.ctrl || mods.alt, mods.shift, gate)
            }
            Input::Paste { text } if !self.opts.read_only => self.typed(&text, gate),
            Input::Press {
                part: Part::Token,
                item: Some(id),
                ..
            } => {
                if let Some(at) = crate::token_text::offset_of_token(&self.value, &id) {
                    self.caret = at + 1;
                }
                Reaction::focusing(Part::Root)
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        vec![to_text(&self.value)]
    }

    fn set_value(&mut self, value: Vec<String>) {
        self.value = from_text(value.first().map(String::as_str).unwrap_or_default());
        self.caret = value_length(&self.value);
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let tokens = self.value.iter().filter_map(|segment| match segment {
            Segment::Token(token) => Some(SnapshotItem {
                key: token.id.clone(),
                label: token.label.clone(),
                disabled: false,
                selected: false,
            }),
            Segment::Text(_) => None,
        });
        Snapshot {
            name: "token_editor",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: tokens.collect(),
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let length = value_length(&self.value);
        let needs = |action: &'static str| item.ok_or(ActError::MissingItem { action });
        let allowed = match action {
            "clear" => self.replace(0, length, Vec::new(), Source::Agent, Reason::Action, gate),
            // Both take the written form, so an agent can add tokens as links.
            "set" => self.replace(
                0,
                length,
                from_text(needs("set")?),
                Source::Agent,
                Reason::Action,
                gate,
            ),
            "insert" => self.replace(
                self.caret,
                self.caret,
                from_text(needs("insert")?),
                Source::Agent,
                Reason::Action,
                gate,
            ),
            "remove_token" => {
                let id = needs("remove_token")?;
                let at = crate::token_text::offset_of_token(&self.value, id)
                    .ok_or_else(|| ActError::UnknownItem(id.to_string()))?;
                self.replace(at, at + 1, Vec::new(), Source::Agent, Reason::Action, gate)
            }
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        if allowed {
            Ok(())
        } else {
            Err(ActError::Refused)
        }
    }
}

/// The content before and after the caret, for drawing the caret between them.
pub fn split_at_caret(editor: &TokenEditor) -> (Vec<Segment>, Vec<Segment>) {
    (
        slice_value(editor.segments(), 0, editor.caret()),
        slice_value(editor.segments(), editor.caret(), usize::MAX),
    )
}

/// Where the caret goes for Home or End on a line, exposed for adapters that
/// handle those keys themselves.
pub fn line_edge(editor: &TokenEditor, forward: bool) -> usize {
    line_boundary(editor.segments(), editor.caret(), forward)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Modifiers;

    fn key(editor: &mut TokenEditor, key: Key) -> Reaction<Part> {
        editor.send(
            Input::Key {
                key,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        )
    }

    fn word(editor: &mut TokenEditor, key: Key) {
        editor.send(
            Input::Key {
                key,
                mods: Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
                now_ms: 0,
            },
            &mut |_| true,
        );
    }

    fn type_text(editor: &mut TokenEditor, text: &str) {
        for ch in text.chars() {
            key(editor, if ch == ' ' { Key::Space } else { Key::Char(ch) });
        }
    }

    #[test]
    fn typing_a_trigger_and_choosing_puts_a_token_in_the_text() {
        let mut editor = TokenEditor::default();
        type_text(&mut editor, "ask @al");
        let found = editor.trigger(&[TriggerRule::new("@")]).unwrap();
        assert_eq!((found.query.as_str(), found.start), ("al", 4));
        assert!(editor.complete(
            &found,
            Token::new("mention", "alice", "@Alice"),
            &mut |_| true
        ));
        assert_eq!(editor.value(), ["ask [@@Alice](mention:alice) "]);
        assert_eq!(editor.label(), "ask @Alice ");
        assert_eq!(editor.caret(), 6, "after the token and the space");
        assert_eq!(editor.trigger(&[TriggerRule::new("@")]), None);
    }

    #[test]
    fn the_caret_steps_over_a_token_and_backspace_takes_it_whole() {
        let mut editor = TokenEditor::default();
        editor.set_value(vec!["hi [@a.ts](file:src/a.ts) there".into()]);
        assert_eq!(editor.caret(), 10);
        word(&mut editor, Key::ArrowLeft);
        assert_eq!(editor.caret(), 5, "a word back");
        key(&mut editor, Key::ArrowLeft);
        key(&mut editor, Key::Backspace);
        assert_eq!(editor.value(), ["hi  there"]);
        assert_eq!(editor.caret(), 3);
        word(&mut editor, Key::Backspace);
        assert_eq!(editor.value(), [" there"]);
    }

    #[test]
    fn the_token_before_the_caret_is_marked_and_a_press_moves_the_caret_after_it() {
        let mut editor = TokenEditor::default();
        editor.set_value(vec!["[@a](x:1) and [@b](x:2)".into()]);
        let ids: Vec<String> = editor
            .snapshot("Prompt")
            .items
            .iter()
            .map(|item| item.key.clone())
            .collect();
        assert!(editor.semantics(Part::Token, Some(&ids[1])).active);
        let pressed = editor.send(
            Input::Press {
                part: Part::Token,
                item: Some(ids[0].clone()),
                mods: Modifiers::default(),
            },
            &mut |_| true,
        );
        assert_eq!(
            (editor.caret(), pressed.focus.map(|focus| focus.part)),
            (1, Some(Part::Root))
        );
        assert!(editor.semantics(Part::Token, Some(&ids[0])).active);
    }

    #[test]
    fn the_owner_can_refuse_an_edit_and_an_agent_writes_tokens_as_links() {
        let mut editor = TokenEditor::default();
        editor.send(
            Input::Key {
                key: Key::Char('x'),
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| false,
        );
        assert_eq!((editor.value(), editor.caret()), (vec![String::new()], 0));
        let mut sources = Vec::new();
        editor
            .act(
                "insert",
                Some("see [@Docs](file:docs/a.md)"),
                &mut |change| {
                    sources.push(change.source);
                    true
                },
            )
            .unwrap();
        assert_eq!(editor.label(), "see Docs");
        assert_eq!(sources, [Source::Agent]);
        let id = editor.snapshot("Prompt").items[0].key.clone();
        editor
            .act("remove_token", Some(&id), &mut |_| true)
            .unwrap();
        assert_eq!(editor.value(), ["see "]);
        assert!(
            editor
                .act("remove_token", Some("nope"), &mut |_| true)
                .is_err()
        );
    }

    #[test]
    fn enter_is_the_owners_unless_the_editor_takes_lines() {
        let mut single = TokenEditor::default();
        assert!(!key(&mut single, Key::Enter).handled);
        let mut lines = TokenEditor::new(Options {
            multiline: true,
            ..Default::default()
        });
        type_text(&mut lines, "a");
        key(&mut lines, Key::Enter);
        type_text(&mut lines, "b");
        assert_eq!(lines.value(), ["a\nb"]);
        key(&mut lines, Key::Home);
        assert_eq!(lines.caret(), 2);
        let mut locked = TokenEditor::new(Options {
            read_only: true,
            ..Default::default()
        });
        type_text(&mut locked, "x");
        assert_eq!(locked.value(), [""]);
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(TokenEditor::default, &[]);
    }
}
