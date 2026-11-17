use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::items::Items;
use crate::keys::Key;
use crate::primitive::{
    Focusable, Input, Item, PartOf, Primitive, Reaction, Relations, Role, Semantics,
};
use crate::roving::Nav;
use crate::text_match::fuzzy_score;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The text field. Focus stays here while the arrows move through the options.
    Input,
    List,
    Option,
}

const ACTIONS: &[&str] = &["search", "select"];

/// A command palette: a text field that filters a list of options as the person
/// types. Choosing an option reports it as the value.
#[derive(Clone, Debug, Default)]
pub struct Command {
    items: Items,
    query: String,
    highlighted: Option<String>,
    selected: Option<String>,
}

impl Command {
    pub fn query(&self) -> &str {
        &self.query
    }

    fn matches(&self, item: &Item) -> bool {
        fuzzy_score(&self.query, &item.label).is_some()
    }

    /// The options still shown, best match first.
    pub fn results(&self) -> Vec<&str> {
        let mut scored: Vec<(u32, &str)> = self
            .items
            .iter()
            .filter_map(|item| Some((fuzzy_score(&self.query, &item.label)?, item.key.as_str())))
            .collect();
        scored.sort_by_key(|scored| std::cmp::Reverse(scored.0));
        scored.into_iter().map(|(_, key)| key).collect()
    }

    fn selectable(&self) -> Vec<String> {
        self.items
            .iter()
            .filter(|item| !item.disabled && self.matches(item))
            .map(|item| item.key.clone())
            .collect()
    }

    fn search(&mut self, query: String) {
        self.query = query;
        let shown = self.selectable();
        let best = self
            .results()
            .into_iter()
            .find(|key| shown.iter().any(|s| s == key))
            .map(str::to_string);
        self.highlighted = best;
    }

    fn choose(&mut self, key: &str, source: Source, reason: Reason, gate: Gate) -> bool {
        let allowed = gate(&ValueChange {
            value: vec![key.to_string()],
            source,
            reason,
        });
        if allowed {
            self.selected = Some(key.to_string());
        }
        allowed
    }
}

impl PartOf for Part {
    type Primitive = Command;
}

impl Primitive for Command {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Input, Part::List, Part::Option]
    }

    fn is_item(part: Part) -> bool {
        part == Part::Option
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

    fn upsert_item(&mut self, item: Item) {
        self.items.upsert(item);
    }

    fn remove_item(&mut self, key: &str) {
        self.items.remove(key);
        if self.highlighted.as_deref() == Some(key) {
            self.highlighted = None;
        }
    }

    fn set_order(&mut self, rendered: &[(String, String)]) {
        self.items.set_order(rendered);
    }

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match (part, item) {
            (Part::Input, _) => Semantics {
                role: Some(Role::TextBox),
                focusable: Focusable::TabStop,
                ..Default::default()
            },
            (Part::List, _) => Semantics {
                role: Some(Role::Listbox),
                ..Default::default()
            },
            (Part::Option, Some(key)) => {
                let found = self.items.get(key);
                Semantics {
                    role: Some(Role::Option),
                    selected: Some(self.selected.as_deref() == Some(key)),
                    disabled: found.is_none_or(|item| item.disabled),
                    hidden: !found.is_some_and(|item| self.matches(item)),
                    focusable: Focusable::Programmatic,
                    active: self.highlighted.as_deref() == Some(key),
                    ..Default::default()
                }
            }
            (Part::Option, None) => Semantics {
                role: Some(Role::Option),
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Text { value, .. } => {
                self.search(value);
                Reaction::handled()
            }
            Input::Key {
                key: Key::ArrowDown | Key::ArrowUp,
                ..
            } if self.selectable().is_empty() => Reaction::handled(),
            Input::Key {
                key: key @ (Key::ArrowDown | Key::ArrowUp),
                ..
            } => {
                let shown = self.selectable();
                let at = self
                    .highlighted
                    .as_ref()
                    .and_then(|h| shown.iter().position(|key| key == h));
                let enabled = vec![true; shown.len()];
                let nav = if key == Key::ArrowDown {
                    Nav::Next
                } else {
                    Nav::Prev
                };
                let next = crate::roving::step(&enabled, at, nav, true).unwrap_or(0);
                self.highlighted = Some(shown[next].clone());
                Reaction::handled()
            }
            Input::Key {
                key: Key::Enter, ..
            } => {
                let Some(key) = self.highlighted.clone() else {
                    return Reaction::default();
                };
                self.choose(&key, Source::User, Reason::Keyboard, gate);
                Reaction::handled()
            }
            // A press takes focus to the option, so it is handed straight back.
            Input::Press {
                part: Part::Option,
                item: Some(key),
                ..
            } if self.items.is_enabled(&key) => {
                self.highlighted = Some(key.clone());
                self.choose(&key, Source::User, Reason::Press, gate);
                Reaction::focusing(Part::Input)
            }
            _ => Reaction::default(),
        }
    }

    fn text(&self, part: Part, _: Option<&str>) -> Option<String> {
        (part == Part::Input).then(|| self.query.clone())
    }

    fn value(&self) -> Vec<String> {
        self.selected.iter().cloned().collect()
    }

    fn set_value(&mut self, value: Vec<String>) {
        self.selected = value.into_iter().find(|key| self.items.get(key).is_some());
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .items
            .iter()
            .filter(|item| self.matches(item))
            .map(|item| SnapshotItem {
                key: item.key.clone(),
                label: item.label.clone(),
                disabled: item.disabled,
                selected: self.selected.as_deref() == Some(item.key.as_str()),
            })
            .collect();
        Snapshot {
            name: "command",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        match action {
            "search" => {
                self.search(item.unwrap_or_default().to_string());
                Ok(())
            }
            "select" => {
                let key = item.ok_or(ActError::MissingItem { action: "select" })?;
                let found = self
                    .items
                    .get(key)
                    .ok_or_else(|| ActError::UnknownItem(key.to_string()))?;
                if found.disabled {
                    return Err(ActError::Disabled(key.to_string()));
                }
                if self.choose(key, Source::Agent, Reason::Action, gate) {
                    Ok(())
                } else {
                    Err(ActError::Refused)
                }
            }
            other => Err(ActError::UnknownAction {
                action: other.to_string(),
                valid: ACTIONS,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Modifiers;

    fn command() -> Command {
        let mut command = Command::default();
        for (key, label, disabled) in [
            ("open", "Open file", false),
            ("save", "Save file", false),
            ("close", "Close window", false),
            ("print", "Print file", true),
        ] {
            command.upsert_item(Item {
                key: key.into(),
                label: label.into(),
                disabled,
            });
        }
        command
    }

    fn typed(command: &mut Command, text: &str) {
        command.send(
            Input::Text {
                part: Part::Input,
                item: None,
                value: text.into(),
            },
            &mut |_| true,
        );
    }

    fn key(command: &mut Command, key: Key) {
        command.send(
            Input::Key {
                key,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        );
    }

    #[test]
    fn typing_hides_options_that_do_not_match() {
        let mut command = command();
        typed(&mut command, "file");
        assert!(!command.semantics(Part::Option, Some("save")).hidden);
        assert!(command.semantics(Part::Option, Some("close")).hidden);
        assert_eq!(command.text(Part::Input, None).as_deref(), Some("file"));
    }

    #[test]
    fn the_best_match_is_highlighted_and_enter_chooses_it() {
        let mut command = command();
        typed(&mut command, "sa");
        assert!(command.semantics(Part::Option, Some("save")).active);
        key(&mut command, Key::Enter);
        assert_eq!(command.value(), ["save"]);
    }

    #[test]
    fn arrows_walk_the_shown_options_and_skip_disabled_ones() {
        let mut command = command();
        typed(&mut command, "file");
        key(&mut command, Key::ArrowDown);
        key(&mut command, Key::ArrowDown);
        assert!(
            command.semantics(Part::Option, Some("open")).active,
            "wrapped past the disabled print"
        );
        key(&mut command, Key::ArrowUp);
        assert!(command.semantics(Part::Option, Some("save")).active);
    }

    #[test]
    fn results_are_ranked_best_first() {
        let mut command = command();
        typed(&mut command, "cl");
        assert_eq!(command.results(), ["close"]);
        typed(&mut command, "o");
        assert_eq!(command.results()[0], "open");
    }

    #[test]
    fn a_press_chooses_and_hands_focus_back_to_the_field() {
        let mut command = command();
        let pressed = command.send(
            Input::Press {
                part: Part::Option,
                item: Some("close".into()),
                mods: Modifiers::default(),
            },
            &mut |_| true,
        );
        assert_eq!(command.value(), ["close"]);
        assert_eq!(pressed.focus.unwrap().part, Part::Input);
    }

    #[test]
    fn nothing_to_choose_means_enter_does_nothing() {
        let mut command = command();
        typed(&mut command, "zzz");
        key(&mut command, Key::ArrowDown);
        key(&mut command, Key::Enter);
        assert!(command.value().is_empty());
        assert!(command.snapshot("Commands").items.is_empty());
    }
}
