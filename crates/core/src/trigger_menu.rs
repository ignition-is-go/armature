use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::items::Items;
use crate::keys::Key;
use crate::popup::{Align, Placement, Side};
use crate::primitive::{
    Focusable, Input, Item, PartOf, Primitive, Reaction, Relations, Role, Semantics,
};
use crate::roving::{self, Nav};
use crate::trigger_match::{TriggerRule, find_trigger_match};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The text being written.
    Input,
    /// The popup of suggestions.
    List,
    /// One suggestion. Its key starts with the character that calls for it, such as
    /// `@ada` or `/deploy`, and its label is what is shown and searched.
    Option,
    /// Shown in the list when nothing matches.
    Empty,
}

const ACTIONS: &[&str] = &["set", "insert"];

/// A text field where typing a character such as `@` or `/` brings up suggestions,
/// as for mentioning a person or running a command. Choosing one writes it into
/// the text. The value is the text.
#[derive(Clone, Debug, Default)]
pub struct TriggerMenu {
    triggers: Vec<char>,
    items: Items,
    text: String,
    active: Option<String>,
    /// Where in the text a match was dismissed with Escape, so it stays closed
    /// until the person starts another.
    dismissed: Option<usize>,
    /// What each command takes after it, by the command's key.
    arguments: Vec<(String, Vec<String>)>,
}

impl TriggerMenu {
    #[must_use]
    pub fn new(triggers: &[char]) -> Self {
        Self {
            triggers: triggers.to_vec(),
            ..Default::default()
        }
    }

    /// Says what a command takes after it, such as an environment for `/deploy`.
    #[must_use]
    pub fn arguments(mut self, key: &str, names: &[&str]) -> Self {
        self.arguments.push((
            key.to_string(),
            names.iter().map(std::string::ToString::to_string).collect(),
        ));
        self
    }

    /// What the command last written still needs: the name of its next argument,
    /// to show as a hint after the caret. `None` once it has them all.
    pub fn argument_hint(&self) -> Option<&str> {
        let complete = self.text.ends_with(char::is_whitespace);
        let words: Vec<&str> = self.text.split_whitespace().collect();
        // The command nearest the end, and how many words follow it.
        let (at, names) = words.iter().enumerate().rev().find_map(|(at, word)| {
            let entry = self.arguments.iter().find(|(key, _)| {
                self.items.get(key).is_some_and(|item| {
                    word.strip_prefix(|ch: char| self.triggers.contains(&ch))
                        == Some(item.label.as_str())
                        && key.starts_with(word.chars().next().unwrap_or(' '))
                })
            })?;
            Some((at, &entry.1))
        })?;
        let given = (words.len().saturating_sub(at)).saturating_sub(1);
        // A word still being typed is the argument in progress, not one given.
        let given = if complete {
            given
        } else {
            given.saturating_sub(1)
        };
        names.get(given).map(String::as_str)
    }

    /// The suggestion being asked for: where it starts, its character, and what has
    /// been typed after it. The character must start a word, or follow an opening
    /// bracket or quote, and the query runs to the end of the text. A full-width
    /// character an input method committed counts as its plain form.
    #[must_use]
    pub fn query(&self) -> Option<(usize, char, &str)> {
        let rules: Vec<TriggerRule> = self
            .triggers
            .iter()
            .map(|trigger| TriggerRule::new(&trigger.to_string()))
            .collect();
        let found = find_trigger_match(&self.text, 0, &rules)?;
        let (start, written) = self.text.char_indices().nth(found.start)?;
        let typed = self
            .text
            .get(start.saturating_add(written.len_utf8())..)
            .unwrap_or_default();
        (self.dismissed != Some(start)).then_some((
            start,
            self.triggers.get(found.rule).copied().unwrap_or_default(),
            typed,
        ))
    }

    #[must_use]
    pub fn is_open(&self) -> bool {
        self.query().is_some()
    }

    /// The suggestions on show, in order.
    #[must_use]
    pub fn results(&self) -> Vec<&str> {
        let Some((_, trigger, typed)) = self.query() else {
            return Vec::new();
        };
        let typed = typed.to_lowercase();
        let fits = |item: &&Item| {
            item.key.starts_with(trigger) && item.label.to_lowercase().contains(&typed)
        };
        self.items
            .iter()
            .filter(fits)
            .map(|item| item.key.as_str())
            .collect()
    }

    fn write(&mut self, text: String, source: Source, reason: Reason, gate: Gate) -> bool {
        if text == self.text {
            return true;
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

    /// Writes a suggestion over what was typed to ask for it, with a space after.
    fn insert(&mut self, key: &str, source: Source, reason: Reason, gate: Gate) -> bool {
        let Some((start, trigger, _)) = self.query() else {
            return false;
        };
        let Some(item) = self
            .items
            .get(key)
            .filter(|item| !item.disabled && item.key.starts_with(trigger))
        else {
            return false;
        };
        let text = format!(
            "{}{trigger}{} ",
            self.text.get(..start).unwrap_or_default(),
            item.label
        );
        let done = self.write(text, source, reason, gate);
        if done {
            self.active = None;
        }
        done
    }

    fn first(&self) -> Option<String> {
        self.results()
            .into_iter()
            .find(|key| self.items.is_enabled(key))
            .map(str::to_string)
    }
}

impl PartOf for Part {
    type Primitive = TriggerMenu;
}

impl Primitive for TriggerMenu {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Input, Part::List, Part::Option, Part::Empty]
    }

    fn is_item(part: Part) -> bool {
        part == Part::Option
    }

    fn popup_of(part: Part) -> Option<Placement<Part>> {
        (part == Part::List).then_some(Placement {
            anchor: Part::Input,
            side: Side::Bottom,
            align: Align::Start,
            gap: 4.0,
        })
    }

    fn relations(part: Part) -> Relations<Part> {
        match part {
            Part::Input => Relations {
                controls: Some(Part::List),
                ..Default::default()
            },
            _ => Relations::default(),
        }
    }

    fn named_part(&self) -> Option<Part> {
        Some(Part::Input)
    }

    fn upsert_item(&mut self, item: Item) {
        self.items.upsert(item);
    }

    fn remove_item(&mut self, key: &str) {
        self.items.remove(key);
    }

    fn set_order(&mut self, rendered: &[(String, String)]) {
        self.items.set_order(rendered);
    }

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        let open = self.is_open();
        match (part, item) {
            (Part::Input, _) => Semantics {
                role: Some(Role::TextBox),
                expanded: Some(open),
                focusable: Focusable::TabStop,
                ..Default::default()
            },
            (Part::List, _) => Semantics {
                role: Some(Role::Listbox),
                hidden: !open,
                ..Default::default()
            },
            (Part::Option, Some(key)) => {
                let shown = self.results().contains(&key);
                Semantics {
                    role: Some(Role::Option),
                    disabled: !self.items.is_enabled(key),
                    hidden: !shown,
                    active: shown && self.active.as_deref() == Some(key),
                    selected: Some(false),
                    ..Default::default()
                }
            }
            (Part::Option, None) => Semantics {
                role: Some(Role::Option),
                ..Default::default()
            },
            (Part::Empty, _) => Semantics {
                role: Some(Role::Status),
                hidden: !open || !self.results().is_empty(),
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        let open = self.is_open();
        match input {
            Input::Text {
                part: Part::Input,
                value,
                ..
            } => {
                if self.write(value, Source::User, Reason::Text, gate) {
                    // Starting another match forgets the one that was dismissed.
                    if self.dismissed.is_some_and(|at| self.text.len() <= at) {
                        self.dismissed = None;
                    }
                    // The first match is ready for Enter.
                    self.active = self.first();
                }
                Reaction::handled()
            }
            Input::Press {
                part: Part::Option,
                item: Some(key),
                ..
            } => {
                self.insert(&key, Source::User, Reason::Press, gate);
                Reaction::focusing(Part::Input)
            }
            Input::Key {
                key: key @ (Key::ArrowDown | Key::ArrowUp),
                ..
            } if open => {
                let results: Vec<String> = self.results().into_iter().map(str::to_string).collect();
                let enabled: Vec<bool> = results
                    .iter()
                    .map(|key| self.items.is_enabled(key))
                    .collect();
                let at = self
                    .active
                    .as_deref()
                    .and_then(|key| results.iter().position(|result| result == key));
                let nav = if key == Key::ArrowDown {
                    Nav::Next
                } else {
                    Nav::Prev
                };
                if let Some(next) = roving::step(&enabled, at, nav, true) {
                    self.active = Some(results[next].clone());
                }
                Reaction::handled()
            }
            Input::Key {
                key: Key::Enter | Key::Tab,
                ..
            } if open => match self.active.clone() {
                Some(key) => {
                    self.insert(&key, Source::User, Reason::Keyboard, gate);
                    Reaction::handled()
                }
                None => Reaction::default(),
            },
            // Escape closes the suggestions and leaves the text as typed.
            Input::Key {
                key: Key::Escape, ..
            } if open => {
                self.dismissed = self.query().map(|(start, _, _)| start);
                self.active = None;
                Reaction::handled()
            }
            Input::PressOutside | Input::WindowBlur | Input::Blur { .. } if open => {
                self.dismissed = self.query().map(|(start, _, _)| start);
                Reaction::default()
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
            self.dismissed = None;
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .items
            .iter()
            .map(|item| SnapshotItem {
                key: item.key.clone(),
                label: item.label.clone(),
                disabled: item.disabled,
                selected: false,
            })
            .collect();
        Snapshot {
            name: "trigger-menu",
            label: label.to_string(),
            value: self.value(),
            expanded: Some(self.is_open()),
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let action: &'static str = match action {
            "set" => "set",
            "insert" => "insert",
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        let given = item.ok_or(ActError::MissingItem { action })?;
        if action == "set" {
            return self
                .write(given.to_string(), Source::Agent, Reason::Action, gate)
                .then_some(())
                .ok_or(ActError::Refused);
        }
        // An agent names the suggestion, and it goes at the end of the text.
        let item = self
            .items
            .get(given)
            .ok_or_else(|| ActError::UnknownItem(given.to_string()))?;
        if item.disabled {
            return Err(ActError::Disabled(given.to_string()));
        }
        let trigger = given.chars().next().unwrap_or('@');
        let gap = if self.text.is_empty() || self.text.ends_with(char::is_whitespace) {
            ""
        } else {
            " "
        };
        let text = format!("{}{gap}{trigger}{} ", self.text, item.label);
        self.write(text, Source::Agent, Reason::Action, gate)
            .then_some(())
            .ok_or(ActError::Refused)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Modifiers;

    fn menu() -> TriggerMenu {
        let mut menu = TriggerMenu::new(&['@', '/']);
        for (key, label) in [
            ("@ada", "ada"),
            ("@alan", "alan"),
            ("@grace", "grace"),
            ("/deploy", "deploy"),
        ] {
            menu.upsert_item(Item {
                key: key.into(),
                label: label.into(),
                disabled: false,
            });
        }
        menu
    }

    fn typed(menu: &mut TriggerMenu, text: &str) {
        menu.send(
            Input::Text {
                part: Part::Input,
                item: None,
                value: text.into(),
            },
            &mut |_| true,
        );
    }

    fn key(menu: &mut TriggerMenu, key: Key) -> Reaction<Part> {
        menu.send(
            Input::Key {
                key,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        )
    }

    #[test]
    fn a_trigger_at_the_start_of_a_word_brings_up_its_own_suggestions() {
        let mut menu = menu();
        typed(&mut menu, "ping @a");
        assert_eq!(menu.results(), ["@ada", "@alan", "@grace"]);
        typed(&mut menu, "ping @al");
        assert_eq!(menu.results(), ["@alan"]);
        typed(&mut menu, "mail a@b");
        assert!(!menu.is_open(), "an at sign inside a word asks for nothing");
        typed(&mut menu, "/");
        assert_eq!(menu.results(), ["/deploy"]);
    }

    #[test]
    fn enter_writes_the_suggestion_into_the_text() {
        let mut menu = menu();
        typed(&mut menu, "ping @a");
        key(&mut menu, Key::ArrowDown);
        assert!(key(&mut menu, Key::Enter).handled);
        assert_eq!(menu.value(), ["ping @alan "]);
        assert!(!menu.is_open());
        assert!(
            !key(&mut menu, Key::Enter).handled,
            "with nothing to choose, Enter is the field's own"
        );
    }

    #[test]
    fn escape_closes_the_suggestions_until_another_is_started() {
        let mut menu = menu();
        typed(&mut menu, "@gr");
        key(&mut menu, Key::Escape);
        typed(&mut menu, "@gra");
        assert!(!menu.is_open());
        typed(&mut menu, "@gra and @a");
        assert!(menu.is_open());
    }

    #[test]
    fn it_conforms() {
        let items: Vec<Item> = ["@a", "@b", "/c"]
            .iter()
            .map(|k| Item {
                key: k.to_string(),
                label: k[1..].to_string(),
                disabled: *k == "@b",
            })
            .collect();
        crate::testing::assert_conforms(|| TriggerMenu::new(&['@', '/']), &items);
    }

    #[test]
    fn a_command_says_what_it_still_needs() {
        let mut menu = menu().arguments("/deploy", &["environment", "version"]);
        typed(&mut menu, "/dep");
        key(&mut menu, Key::Enter);
        assert_eq!(menu.value(), ["/deploy "]);
        assert_eq!(menu.argument_hint(), Some("environment"));
        typed(&mut menu, "/deploy prod");
        assert_eq!(
            menu.argument_hint(),
            Some("environment"),
            "still being typed"
        );
        typed(&mut menu, "/deploy prod ");
        assert_eq!(menu.argument_hint(), Some("version"));
        typed(&mut menu, "/deploy prod 1.2 ");
        assert_eq!(menu.argument_hint(), None);
        typed(&mut menu, "hello @ada ");
        assert_eq!(menu.argument_hint(), None, "a mention takes nothing");
    }
}
