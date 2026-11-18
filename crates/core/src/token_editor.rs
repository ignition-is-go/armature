use std::ops::Range;

use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::{Dir, Key};
use crate::primitive::{
    Effect, Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics,
};
use crate::token_text::{
    Segment, Token, from_text, normalize, offset_of_token, offset_text, replace_range, slice_value,
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

const ACTIONS: &[&str] = &["set", "insert", "remove_token", "clear", "undo", "redo"];

/// How many edits can be undone.
const HISTORY: usize = 100;

/// Who made an edit and how, as the owner's gate hears it.
#[derive(Clone, Copy)]
struct Cause {
    source: Source,
    reason: Reason,
}

impl Cause {
    /// Typing, deleting, pasting and cutting.
    const TYPED: Self = Self {
        source: Source::User,
        reason: Reason::Text,
    };
    /// Choosing a suggestion.
    const CHOSEN: Self = Self {
        source: Source::User,
        reason: Reason::Press,
    };
    /// A call the owner makes on the person's behalf, such as attaching a file.
    const OWNER: Self = Self {
        source: Source::User,
        reason: Reason::Action,
    };
    const AGENT: Self = Self {
        source: Source::Agent,
        reason: Reason::Action,
    };
}

/// The content at an earlier moment, for undo.
#[derive(Clone, Debug)]
struct Past {
    value: Vec<Segment>,
    caret: usize,
}

/// Text with tokens set into it, edited at a caret:
///
/// a prompt that mentions people and files, say. A token is one step for the caret and
/// goes whole when deleted. The value is the content written as text, each token as
/// `[@label](kind:value)`.
#[derive(Clone, Debug, Default)]
pub struct TokenEditor {
    opts: Options,
    value: Vec<Segment>,
    /// Where typing goes, counted in characters and tokens.
    caret: usize,
    /// The other end of the selection, when there is one.
    anchor: Option<usize>,
    /// What undo steps back to, newest last.
    undone: Vec<Past>,
    /// What redo steps forward to, newest last.
    redone: Vec<Past>,
}

// Reading the editor.
impl TokenEditor {
    #[must_use]
    pub fn new(opts: Options) -> Self {
        Self {
            opts,
            ..Default::default()
        }
    }

    /// The content in order: runs of text, and tokens.
    #[must_use]
    pub fn segments(&self) -> &[Segment] {
        &self.value
    }

    #[must_use]
    pub const fn caret(&self) -> usize {
        self.caret
    }

    /// What is selected, as where it starts and ends. `None` when it is empty.
    #[must_use]
    pub fn selection(&self) -> Option<Range<usize>> {
        let anchor = self.anchor.filter(|anchor| *anchor != self.caret)?;
        Some(anchor.min(self.caret)..anchor.max(self.caret))
    }

    /// The content before and after the caret, for drawing the caret between them.
    #[must_use]
    pub fn split_at_caret(&self) -> (Vec<Segment>, Vec<Segment>) {
        (
            slice_value(&self.value, 0, self.caret),
            slice_value(&self.value, self.caret, usize::MAX),
        )
    }

    /// The content as a person reads it, with each token as its label.
    #[must_use]
    pub fn label(&self) -> String {
        to_label(&self.value)
    }

    /// The trigger being typed just before the caret, such as `@al`, for offering
    /// suggestions. Tokens end a query.
    #[must_use]
    pub fn trigger(&self, rules: &[TriggerRule]) -> Option<TriggerMatch> {
        let before: String = offset_text(&self.value).chars().take(self.caret).collect();
        find_trigger_match(&before, 0, rules)
    }

    fn length(&self) -> usize {
        value_length(&self.value)
    }

    /// The range the next edit replaces: the selection, or nothing at the caret.
    fn target(&self) -> Range<usize> {
        self.selection().unwrap_or(self.caret..self.caret)
    }

    /// Where a token sits, and the token.
    fn token(&self, id: &str) -> Option<(usize, &Token)> {
        let at = offset_of_token(&self.value, id)?;
        Some((at, token_at(&self.value, at)?))
    }

    /// The token just before the caret: the one Backspace would take.
    fn token_before_caret(&self) -> Option<&Token> {
        token_at(&self.value, self.caret.checked_sub(1)?)
    }

    /// Where the line the caret is on starts.
    fn line_start(&self) -> usize {
        let before: Vec<char> = offset_text(&self.value).chars().take(self.caret).collect();
        before
            .iter()
            .rposition(|ch| *ch == '\n')
            .map_or(0, |line_break| line_break.saturating_add(1))
    }

    /// Where the line the caret is on ends.
    fn line_end(&self) -> usize {
        let after = offset_text(&self.value)
            .chars()
            .skip(self.caret)
            .position(|ch| ch == '\n');
        after.map_or_else(
            || self.length(),
            |line_break| self.caret.saturating_add(line_break),
        )
    }
}

// Changing the content.
impl TokenEditor {
    /// Replaces a trigger and its query with a token and a space, as choosing a
    /// suggestion does. Returns whether the owner allowed it.
    pub fn complete(&mut self, found: &TriggerMatch, token: Token, gate: Gate) -> bool {
        let content = vec![Segment::Token(token), Segment::Text(" ".into())];
        self.replace(found.start..found.end, content, Cause::CHOSEN, gate)
    }

    /// Puts a token at the caret that is still loading, such as a file being
    /// uploaded, and returns its id for `settle`. `None` if the owner refused.
    pub fn insert_pending(&mut self, token: Token, gate: Gate) -> Option<String> {
        // An id the content already has names that token, so this one gets its own.
        let id = if self.token(&token.id).is_some() {
            String::new()
        } else {
            token.id.clone()
        };
        let token = Token {
            pending: true,
            id,
            ..token
        };
        if !self.replace(
            self.target(),
            vec![Segment::Token(token)],
            Cause::OWNER,
            gate,
        ) {
            return None;
        }
        self.token_before_caret().map(|token| token.id.clone())
    }

    /// Says a loading token has finished. One that loaded stays, settled, here and
    /// in what undo would bring back. One that failed is taken out.
    pub fn settle(&mut self, id: &str, loaded: bool, gate: Gate) {
        let settle_in = |value: &mut Vec<Segment>| {
            for segment in value {
                if let Segment::Token(token) = segment
                    && token.id == id
                {
                    token.pending = false;
                }
            }
        };
        settle_in(&mut self.value);
        for past in self.undone.iter_mut().chain(&mut self.redone) {
            settle_in(&mut past.value);
        }
        if loaded {
            return;
        }
        let Some((at, _)) = self.token(id) else {
            return;
        };
        let caret = self.caret;
        if self.replace(at..at.saturating_add(1), Vec::new(), Cause::OWNER, gate) {
            // The caret stays with the text it was in.
            self.caret = if caret > at {
                caret.saturating_sub(1)
            } else {
                caret
            };
            // A token that never loaded is not something to undo back to.
            self.undone.pop();
        }
    }

    /// Changes what a token shows. Returns whether there is such a token and the
    /// owner allowed it.
    pub fn update_token(&mut self, id: &str, label: &str, gate: Gate) -> bool {
        let Some((at, token)) = self.token(id) else {
            return false;
        };
        let renamed = Token {
            label: label.to_string(),
            ..token.clone()
        };
        let caret = self.caret;
        let allowed = self.replace(
            at..at.saturating_add(1),
            vec![Segment::Token(renamed)],
            Cause::OWNER,
            gate,
        );
        self.caret = caret;
        allowed
    }

    /// Puts `content` in place of `range` and the caret after it. Every edit goes
    /// through here, so each one is offered to the owner and can be undone.
    fn replace(
        &mut self,
        range: Range<usize>,
        content: Vec<Segment>,
        cause: Cause,
        gate: Gate,
    ) -> bool {
        let added = value_length(&normalize(content.clone()));
        let next = replace_range(&self.value, range.start, range.end, content);
        if next == self.value {
            return true;
        }
        let change = ValueChange {
            value: vec![to_text(&next)],
            source: cause.source,
            reason: cause.reason,
        };
        if !gate(&change) {
            return false;
        }
        let before = Past {
            value: std::mem::replace(&mut self.value, next),
            caret: self.caret,
        };
        self.undone.push(before);
        if self.undone.len() > HISTORY {
            self.undone.remove(0);
        }
        self.redone.clear();
        self.caret = range.start.saturating_add(added);
        self.anchor = None;
        true
    }

    fn delete(&mut self, range: Range<usize>, cause: Cause, gate: Gate) -> bool {
        self.replace(range, Vec::new(), cause, gate)
    }

    fn insert_text(&mut self, text: &str, gate: Gate) {
        self.replace(
            self.target(),
            vec![Segment::Text(text.to_string())],
            Cause::TYPED,
            gate,
        );
    }

    /// Undoes the last edit, or redoes the last one undone. Returns whether there
    /// was one and the owner allowed it.
    fn step_history(&mut self, undo: bool, source: Source, gate: Gate) -> bool {
        let (from, to) = if undo {
            (&mut self.undone, &mut self.redone)
        } else {
            (&mut self.redone, &mut self.undone)
        };
        let Some(past) = from.last() else {
            return false;
        };
        let change = ValueChange {
            value: vec![to_text(&past.value)],
            source,
            reason: Reason::Edit,
        };
        if !gate(&change) {
            return false;
        }
        let Some(past) = from.pop() else {
            return false;
        };
        to.push(Past {
            value: std::mem::replace(&mut self.value, past.value),
            caret: self.caret,
        });
        self.caret = past.caret.min(value_length(&self.value));
        self.anchor = None;
        true
    }
}

// Keys.
impl TokenEditor {
    /// A key held with Ctrl, or Cmd on a Mac: select all, copy, cut, undo, redo.
    /// `None` for a key that is not one of these.
    fn on_command(&mut self, key: Key, shift: bool, gate: Gate) -> Option<Reaction<Part>> {
        let Key::Char(letter) = key else {
            return None;
        };
        let may_edit = !self.opts.read_only;
        let copy_selection = |editor: &Self| {
            let selected = editor.selection()?;
            Some(Effect::Copy {
                text: to_text(&slice_value(&editor.value, selected.start, selected.end)),
            })
        };
        let reaction = match letter.to_ascii_lowercase() {
            'a' => {
                self.anchor = Some(0);
                self.caret = self.length();
                Reaction::handled()
            }
            'c' => copy_selection(self)
                .map_or_else(Reaction::handled, |copy| Reaction::handled().with(copy)),
            'x' if may_edit => match (copy_selection(self), self.selection()) {
                (Some(copy), Some(selected)) => {
                    self.delete(selected, Cause::TYPED, gate);
                    Reaction::handled().with(copy)
                }
                _ => Reaction::handled(),
            },
            'z' if may_edit => {
                self.step_history(!shift, Source::User, gate);
                Reaction::handled()
            }
            'y' if may_edit => {
                self.step_history(false, Source::User, gate);
                Reaction::handled()
            }
            _ => return None,
        };
        Some(reaction)
    }

    fn on_key(&mut self, key: Key, by_word: bool, shift: bool, gate: Gate) -> Reaction<Part> {
        let (back, forward) = if self.opts.dir == Dir::Rtl {
            (Key::ArrowRight, Key::ArrowLeft)
        } else {
            (Key::ArrowLeft, Key::ArrowRight)
        };
        match key {
            key if key == back || key == forward => self.move_caret(key == forward, by_word, shift),
            Key::Home => self.move_caret_to(self.line_start(), shift),
            Key::End => self.move_caret_to(self.line_end(), shift),
            _ if self.opts.read_only => return Reaction::default(),
            Key::Backspace => self.delete_back(by_word, gate),
            Key::Char(ch) if !by_word => self.insert_text(&ch.to_string(), gate),
            Key::Space => self.insert_text(" ", gate),
            Key::Enter if self.opts.multiline || shift => self.insert_text("\n", gate),
            _ => return Reaction::default(),
        }
        Reaction::handled()
    }

    /// Moves the caret. With Shift the other end of the selection stays where it
    /// is; without, the selection is dropped.
    fn move_caret_to(&mut self, caret: usize, shift: bool) {
        self.anchor = if shift {
            self.anchor.or(Some(self.caret))
        } else {
            None
        };
        self.caret = caret;
    }

    /// An arrow key: one step, or one word. A plain arrow with something selected
    /// only collapses the selection, to the side the arrow points at.
    fn move_caret(&mut self, ahead: bool, by_word: bool, shift: bool) {
        let to = match self.selection() {
            Some(selected) if !shift && !by_word => {
                if ahead {
                    selected.end
                } else {
                    selected.start
                }
            }
            _ if by_word => word_boundary(&self.value, self.caret, ahead),
            _ if ahead => (self.caret.saturating_add(1)).min(self.length()),
            _ => self.caret.saturating_sub(1),
        };
        self.move_caret_to(to, shift);
    }

    /// Backspace: the selection, or else the word or the one step before the caret.
    fn delete_back(&mut self, by_word: bool, gate: Gate) {
        let range = match self.selection() {
            Some(selected) => selected,
            None if by_word => word_boundary(&self.value, self.caret, false)..self.caret,
            None => self.caret.saturating_sub(1)..self.caret,
        };
        self.delete(range, Cause::TYPED, gate);
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
        if part == Part::Root {
            return Semantics {
                role: Some(Role::TextBox),
                focusable: Focusable::TabStop,
                ..Default::default()
            };
        }
        let Some((at, token)) = item.and_then(|id| self.token(id)) else {
            return Semantics::default();
        };
        Semantics {
            selected: Some(
                self.selection()
                    .is_some_and(|selected| selected.contains(&at)),
            ),
            busy: token.pending,
            active: self
                .token_before_caret()
                .is_some_and(|before| before.id == token.id),
            ..Default::default()
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Key { key, mods, .. } => {
                let command = if mods.ctrl {
                    self.on_command(key, mods.shift, gate)
                } else {
                    None
                };
                command.unwrap_or_else(|| self.on_key(key, mods.ctrl || mods.alt, mods.shift, gate))
            }
            Input::Paste { text } if !self.opts.read_only => {
                self.insert_text(&text, gate);
                Reaction::handled()
            }
            Input::Press {
                part: Part::Token,
                item: Some(id),
                ..
            } => {
                if let Some((at, _)) = self.token(&id) {
                    self.move_caret_to(at.saturating_add(1), false);
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
        self.caret = self.length();
        self.anchor = None;
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
        let written = |action: &'static str| item.ok_or(ActError::MissingItem { action });
        let allowed = match action {
            // With nothing to undo or redo there is nothing to refuse.
            "undo" => self.undone.is_empty() || self.step_history(true, Source::Agent, gate),
            "redo" => self.redone.is_empty() || self.step_history(false, Source::Agent, gate),
            "clear" => self.delete(0..self.length(), Cause::AGENT, gate),
            // Both take the written form, so an agent can add tokens as links.
            "set" => self.replace(
                0..self.length(),
                from_text(written("set")?),
                Cause::AGENT,
                gate,
            ),
            "insert" => self.replace(
                self.caret..self.caret,
                from_text(written("insert")?),
                Cause::AGENT,
                gate,
            ),
            "remove_token" => {
                let id = written("remove_token")?;
                let (at, _) = self
                    .token(id)
                    .ok_or_else(|| ActError::UnknownItem(id.to_string()))?;
                self.delete(at..at.saturating_add(1), Cause::AGENT, gate)
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
    fn a_selection_is_replaced_by_typing_and_copied_as_text() {
        let mut editor = TokenEditor::default();
        editor.set_value(vec!["hi [@a.ts](file:src/a.ts) there".into()]);
        let shift = Modifiers {
            shift: true,
            ..Default::default()
        };
        let ctrl = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        key(&mut editor, Key::Home);
        for _ in 0..4 {
            editor.send(
                Input::Key {
                    key: Key::ArrowRight,
                    mods: shift,
                    now_ms: 0,
                },
                &mut |_| true,
            );
        }
        assert_eq!(editor.selection(), Some(0..4));
        let id = editor.snapshot("Prompt").items[0].key.clone();
        assert_eq!(
            editor.semantics(Part::Token, Some(&id)).selected,
            Some(true)
        );
        let copied = editor.send(
            Input::Key {
                key: Key::Char('c'),
                mods: ctrl,
                now_ms: 0,
            },
            &mut |_| true,
        );
        assert_eq!(
            copied.effects,
            [Effect::Copy {
                text: "hi [@a.ts](file:src/a.ts)".into()
            }]
        );
        key(&mut editor, Key::Char('x'));
        assert_eq!(
            (editor.value(), editor.caret(), editor.selection()),
            (vec!["x there".to_string()], 1, None)
        );
        editor.send(
            Input::Key {
                key: Key::Char('a'),
                mods: ctrl,
                now_ms: 0,
            },
            &mut |_| true,
        );
        key(&mut editor, Key::Backspace);
        assert_eq!(editor.value(), [""]);
    }

    #[test]
    fn edits_are_undone_and_redone() {
        let mut editor = TokenEditor::default();
        let ctrl = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        type_text(&mut editor, "ab");
        editor.send(
            Input::Key {
                key: Key::Char('z'),
                mods: ctrl,
                now_ms: 0,
            },
            &mut |_| true,
        );
        assert_eq!((editor.value(), editor.caret()), (vec!["a".to_string()], 1));
        editor.send(
            Input::Key {
                key: Key::Char('z'),
                mods: Modifiers {
                    shift: true,
                    ..ctrl
                },
                now_ms: 0,
            },
            &mut |_| true,
        );
        assert_eq!(editor.value(), ["ab"]);
        editor.act("undo", None, &mut |_| true).unwrap();
        editor.act("undo", None, &mut |_| true).unwrap();
        editor.act("undo", None, &mut |_| true).unwrap();
        assert_eq!(
            editor.value(),
            [""],
            "undo with nothing left changes nothing"
        );
        type_text(&mut editor, "c");
        editor.act("redo", None, &mut |_| true).unwrap();
        assert_eq!(editor.value(), ["c"], "a new edit forgets what was undone");
    }

    // Upstream's token-input.test.ts, "pending tokens" and "token ids".
    #[test]
    fn a_token_removed_while_it_loads_comes_back_settled_on_undo() {
        let mut editor = TokenEditor::default();
        let id = editor
            .insert_pending(Token::new("file", "src/a.ts", "a.ts"), &mut |_| true)
            .unwrap();
        assert!(editor.semantics(Part::Token, Some(&id)).busy);
        editor
            .act("remove_token", Some(&id), &mut |_| true)
            .unwrap();
        editor.settle(&id, true, &mut |_| true);
        editor.act("undo", None, &mut |_| true).unwrap();
        let restored = editor.snapshot("Prompt").items[0].key.clone();
        assert_eq!(restored, id);
        assert!(!editor.semantics(Part::Token, Some(&id)).busy);
    }

    #[test]
    fn a_token_that_fails_to_load_goes_and_leaves_the_others() {
        let mut editor = TokenEditor::default();
        editor.set_value(vec!["[@b.ts](file:src/b.ts)".into()]);
        let kept = editor.snapshot("Prompt").items[0].key.clone();
        // One inserted with an id the content already has gets its own.
        let id = editor
            .insert_pending(
                Token::new("file", "src/a.ts", "a.ts").with_id(&kept),
                &mut |_| true,
            )
            .unwrap();
        assert_ne!(id, kept);
        editor.settle(&id, false, &mut |_| true);
        let labels: Vec<String> = editor
            .snapshot("Prompt")
            .items
            .iter()
            .map(|item| item.label.clone())
            .collect();
        assert_eq!(labels, ["b.ts"]);
        // "a repeated id names one token, so updating or removing it touches only that one"
        assert!(editor.update_token(&kept, "c.ts", &mut |_| true));
        assert_eq!(editor.label(), "c.ts");
        assert!(!editor.update_token("nope", "x", &mut |_| true));
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(TokenEditor::default, &[]);
    }
}
