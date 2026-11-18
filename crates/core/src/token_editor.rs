use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::{Dir, Key};
use crate::primitive::{
    Effect, Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics,
};
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

const ACTIONS: &[&str] = &["set", "insert", "remove_token", "clear", "undo", "redo"];

/// How many edits can be undone.
const HISTORY: usize = 100;

/// Text with tokens set into it, edited at a caret: a prompt that mentions people
/// and files, say. A token is one step for the caret and goes whole when deleted.
/// The value is the content written as text, each token as `[@label](kind:value)`.
#[derive(Clone, Debug, Default)]
pub struct TokenEditor {
    opts: Options,
    value: Vec<Segment>,
    /// Where typing goes, counted in characters and tokens.
    caret: usize,
    /// The other end of the selection, when there is one.
    anchor: Option<usize>,
    /// Earlier contents and where the caret was, newest last.
    undone: Vec<(Vec<Segment>, usize)>,
    redone: Vec<(Vec<Segment>, usize)>,
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

    /// What is selected, as where it starts and ends. `None` when it is empty.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor.filter(|anchor| *anchor != self.caret)?;
        Some((anchor.min(self.caret), anchor.max(self.caret)))
    }

    /// The range an edit replaces: the selection, or nothing at the caret.
    fn target(&self) -> (usize, usize) {
        self.selection().unwrap_or((self.caret, self.caret))
    }

    /// Puts a token at the caret that is still loading, such as a file being
    /// uploaded, and returns its id for `settle`. `None` if the owner refused.
    pub fn insert_pending(&mut self, token: Token, gate: Gate) -> Option<String> {
        // An id the content already has names that token, so this one gets its own.
        let taken = crate::token_text::offset_of_token(&self.value, &token.id).is_some();
        let token = Token {
            pending: true,
            id: if taken {
                String::new()
            } else {
                token.id.clone()
            },
            ..token
        };
        let (start, end) = self.target();
        if !self.replace(
            start,
            end,
            vec![Segment::Token(token)],
            Source::User,
            Reason::Action,
            gate,
        ) {
            return None;
        }
        token_at(&self.value, self.caret - 1).map(|token| token.id.clone())
    }

    /// Says a loading token has finished. One that loaded stays, settled, here and
    /// in what undo would bring back. One that failed is taken out.
    pub fn settle(&mut self, id: &str, loaded: bool, gate: Gate) {
        let settled = |value: &mut Vec<Segment>| {
            for segment in value.iter_mut() {
                if let Segment::Token(token) = segment
                    && token.id == id
                {
                    token.pending = false;
                }
            }
        };
        settled(&mut self.value);
        self.undone
            .iter_mut()
            .chain(self.redone.iter_mut())
            .for_each(|(value, _)| settled(value));
        if !loaded && let Some(at) = crate::token_text::offset_of_token(&self.value, id) {
            let caret = self.caret;
            self.replace(at, at + 1, Vec::new(), Source::User, Reason::Action, gate);
            self.caret = if caret > at { caret - 1 } else { caret }.min(value_length(&self.value));
            // A token that never loaded is not something to undo back to.
            self.undone.pop();
        }
    }

    /// Changes what a token shows. Returns whether there is such a token and the
    /// owner allowed it.
    pub fn update_token(&mut self, id: &str, label: &str, gate: Gate) -> bool {
        let Some(at) = crate::token_text::offset_of_token(&self.value, id) else {
            return false;
        };
        let Some(token) = token_at(&self.value, at).cloned() else {
            return false;
        };
        let caret = self.caret;
        let allowed = self.replace(
            at,
            at + 1,
            vec![Segment::Token(Token {
                label: label.to_string(),
                ..token
            })],
            Source::User,
            Reason::Action,
            gate,
        );
        self.caret = caret;
        allowed
    }

    /// Steps back or forward through the edits. Returns whether there was one and
    /// the owner allowed it.
    fn travel(&mut self, back: bool, source: Source, gate: Gate) -> bool {
        let (from, to) = if back {
            (&mut self.undone, &mut self.redone)
        } else {
            (&mut self.redone, &mut self.undone)
        };
        let Some((value, caret)) = from.last().cloned() else {
            return false;
        };
        if !gate(&ValueChange {
            value: vec![to_text(&value)],
            source,
            reason: Reason::Edit,
        }) {
            return false;
        }
        from.pop();
        to.push((std::mem::replace(&mut self.value, value), self.caret));
        self.caret = caret.min(value_length(&self.value));
        self.anchor = None;
        true
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
            self.undone
                .push((std::mem::replace(&mut self.value, next), self.caret));
            if self.undone.len() > HISTORY {
                self.undone.remove(0);
            }
            self.redone.clear();
            self.caret = start + added;
            self.anchor = None;
        }
        allowed
    }

    fn typed(&mut self, text: &str, gate: Gate) -> Reaction<Part> {
        let (start, end) = self.target();
        self.replace(
            start,
            end,
            vec![Segment::Text(text.to_string())],
            Source::User,
            Reason::Text,
            gate,
        );
        Reaction::handled()
    }

    /// The shortcuts held with Ctrl, or Cmd on a Mac.
    fn command(&mut self, key: Key, shift: bool, gate: Gate) -> Option<Reaction<Part>> {
        let copied = |editor: &Self| {
            editor.selection().map(|(start, end)| Effect::Copy {
                text: to_text(&slice_value(&editor.value, start, end)),
            })
        };
        Some(match key {
            Key::Char('a' | 'A') => {
                self.anchor = Some(0);
                self.caret = value_length(&self.value);
                Reaction::handled()
            }
            Key::Char('c' | 'C') => {
                copied(self).map_or_else(Reaction::handled, |copy| Reaction::handled().with(copy))
            }
            Key::Char('x' | 'X') if !self.opts.read_only => {
                let (Some(copy), Some((start, end))) = (copied(self), self.selection()) else {
                    return Some(Reaction::handled());
                };
                self.replace(start, end, Vec::new(), Source::User, Reason::Text, gate);
                Reaction::handled().with(copy)
            }
            Key::Char('z' | 'Z') if !self.opts.read_only => {
                self.travel(!shift, Source::User, gate);
                Reaction::handled()
            }
            Key::Char('y' | 'Y') if !self.opts.read_only => {
                self.travel(false, Source::User, gate);
                Reaction::handled()
            }
            _ => return None,
        })
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
                // Shift keeps the other end of the selection where it is. Without
                // it, a selection collapses to the side the arrow points at.
                match (shift, self.selection()) {
                    (true, _) => self.anchor = self.anchor.or(Some(self.caret)),
                    (false, Some((start, end))) if !by_word => {
                        self.anchor = None;
                        self.caret = if ahead { end } else { start };
                        return Reaction::handled();
                    }
                    (false, _) => self.anchor = None,
                }
                self.caret = match (by_word, ahead) {
                    (true, _) => word_boundary(&self.value, self.caret, ahead),
                    (false, true) => (self.caret + 1).min(length),
                    (false, false) => self.caret.saturating_sub(1),
                };
                Reaction::handled()
            }
            Key::Home | Key::End => {
                self.anchor = if shift {
                    self.anchor.or(Some(self.caret))
                } else {
                    None
                };
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
            Key::Backspace if self.selection().is_some() => {
                let (start, end) = self.target();
                self.replace(start, end, Vec::new(), Source::User, Reason::Text, gate);
                Reaction::handled()
            }
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
                selected: Some(
                    item.and_then(|id| crate::token_text::offset_of_token(&self.value, id))
                        .is_some_and(|at| {
                            self.selection()
                                .is_some_and(|(start, end)| start <= at && at < end)
                        }),
                ),
                busy: item
                    .and_then(|id| crate::token_text::offset_of_token(&self.value, id))
                    .and_then(|at| token_at(&self.value, at))
                    .is_some_and(|token| token.pending),
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
                match mods
                    .ctrl
                    .then(|| self.command(key, mods.shift, gate))
                    .flatten()
                {
                    Some(reaction) => reaction,
                    None => self.on_key(key, mods.ctrl || mods.alt, mods.shift, gate),
                }
            }
            Input::Paste { text } if !self.opts.read_only => self.typed(&text, gate),
            Input::Press {
                part: Part::Token,
                item: Some(id),
                ..
            } => {
                if let Some(at) = crate::token_text::offset_of_token(&self.value, &id) {
                    self.caret = at + 1;
                    self.anchor = None;
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
        let length = value_length(&self.value);
        let needs = |action: &'static str| item.ok_or(ActError::MissingItem { action });
        let allowed = match action {
            "undo" => self.travel(true, Source::Agent, gate) || self.undone.is_empty(),
            "redo" => self.travel(false, Source::Agent, gate) || self.redone.is_empty(),
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
        assert_eq!(editor.selection(), Some((0, 4)));
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
