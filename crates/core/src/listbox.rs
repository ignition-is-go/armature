use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::items::Items;
use crate::keys::{Dir, Key, Modifiers, Orientation};
use crate::primitive::{Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics};
use crate::roving::Ends;
use crate::roving::{self, Nav};
use crate::selection::{self, Mode};
use crate::typeahead::Typeahead;

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    pub mode: Mode,
    pub orientation: Orientation,
    pub dir: Dir,
    pub wrap: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    Option,
    /// Optional. Wraps options that belong together, keyed by the group.
    Group,
    /// The heading of a group, keyed like it. It names the group.
    GroupLabel,
}

const ACTIONS: &[&str] = &["select", "deselect", "clear"];

#[derive(Clone, Debug, Default)]
pub struct Listbox {
    opts: Options,
    items: Items,
    highlighted: Option<String>,
    selected: Vec<String>,
    anchor: Option<String>,
    typeahead: Typeahead,
}

impl Listbox {
    #[must_use]
    pub fn new(opts: Options) -> Self {
        Self {
            opts,
            ..Default::default()
        }
    }

    #[must_use]
    pub fn highlighted(&self) -> Option<&str> {
        self.highlighted.as_deref()
    }

    fn on_key(&mut self, key: Key, mods: Modifiers, now_ms: u64, gate: Gate) -> Reaction<Part> {
        // PageDown and PageUp go ten items, stopping at the ends.
        if let Key::PageDown | Key::PageUp = key {
            let nav = if key == Key::PageDown {
                Nav::Next
            } else {
                Nav::Prev
            };
            let mut focus = None;
            for _ in 0..10 {
                match self.items.step(
                    focus.as_deref().or(self.highlighted.as_deref()),
                    nav,
                    Ends::Stop,
                ) {
                    Some(next) => focus = Some(next),
                    None => break,
                }
            }
            if let Some(key) = &focus {
                self.highlighted = Some(key.clone());
            }
            return Reaction::to_item(Part::Option, focus);
        }
        if let Some(nav) = roving::nav_for_key(key, self.opts.orientation, self.opts.dir) {
            let focus = self.items.step(
                self.highlighted.as_deref(),
                nav,
                Ends::wrapping(self.opts.wrap),
            );
            if let Some(key) = &focus {
                self.highlighted = Some(key.clone());
                if mods.shift && self.opts.mode == Mode::Multiple {
                    let range = Modifiers {
                        shift: true,
                        ctrl: false,
                        alt: false,
                    };
                    self.select(key, range, Reason::Keyboard, gate);
                }
            }
            return Reaction::to_item(Part::Option, focus);
        }
        match key {
            Key::Enter | Key::Space => {
                let Some(key) = self.highlighted.clone() else {
                    return Reaction::default();
                };
                self.select(&key, mods, Reason::Keyboard, gate);
                Reaction::to_item(Part::Option, None)
            }
            Key::Char('a' | 'A') if mods.ctrl && self.opts.mode == Mode::Multiple => {
                let order = self.items.enabled_keys();
                let next = selection::toggle_all(self.opts.mode, &self.selected, &order);
                self.commit(next, Source::User, Reason::Keyboard, gate);
                Reaction::handled()
            }
            Key::Char(ch) if !mods.ctrl => {
                let labels: Vec<(&str, bool)> = self
                    .items
                    .iter()
                    .map(|i| (i.label.as_str(), !i.disabled))
                    .collect();
                let current = self
                    .highlighted
                    .as_deref()
                    .and_then(|k| self.items.index_of(k));
                let target = self.typeahead.push(ch, now_ms, &labels, current);
                let focus = target.map(|i| self.items.key_at(i));
                if let Some(key) = &focus {
                    self.highlighted = Some(key.clone());
                }
                Reaction::to_item_if_any(Part::Option, focus)
            }
            _ => Reaction::default(),
        }
    }

    fn select(&mut self, key: &str, mods: Modifiers, reason: Reason, gate: Gate) {
        let next = self.proposal(key, mods);
        let committed = self.commit(next, Source::User, reason, gate);
        if committed && (!mods.shift || self.anchor.is_none()) {
            self.anchor = Some(key.to_string());
        }
    }

    fn proposal(&self, key: &str, mods: Modifiers) -> Vec<String> {
        let order = self.items.enabled_keys();
        selection::propose(
            self.opts.mode,
            &self.selected,
            self.anchor.as_deref(),
            key,
            mods,
            &order,
        )
    }

    fn commit(&mut self, next: Vec<String>, source: Source, reason: Reason, gate: Gate) -> bool {
        if next == self.selected {
            return true;
        }
        let change = ValueChange {
            value: next,
            source,
            reason,
        };
        let allowed = gate(&change);
        if allowed {
            self.selected = change.value;
        }
        allowed
    }

    /// The one item reachable by Tab: the highlight, else the first selected item,
    /// else the first enabled one.
    pub(crate) fn tab_stop(&self) -> Option<&str> {
        let enabled = |key: &&str| self.items.is_enabled(key);
        self.highlighted
            .as_deref()
            .filter(enabled)
            .or_else(|| self.selected.iter().map(String::as_str).find(enabled))
            .or_else(|| self.items.first_enabled())
    }

    fn is_selected(&self, key: &str) -> bool {
        self.selected.iter().any(|k| k == key)
    }
}

impl Listbox {
    /// The selection with `key` added, as holding Ctrl and pressing it would leave it.
    fn with_added(&self, key: &str) -> Vec<String> {
        if self.is_selected(key) {
            return self.selected.clone();
        }
        let adding = Modifiers {
            ctrl: true,
            shift: false,
            alt: false,
        };
        self.proposal(key, adding)
    }
}

impl PartOf for Part {
    type Primitive = Listbox;
}

impl Primitive for Listbox {
    type Part = Part;

    fn relations(part: Part) -> crate::primitive::Relations<Part> {
        match part {
            Part::Group => crate::primitive::Relations {
                labelled_by: Some(Part::GroupLabel),
                ..Default::default()
            },
            _ => crate::primitive::Relations::default(),
        }
    }

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Option, Part::Group, Part::GroupLabel]
    }

    fn is_item(part: Part) -> bool {
        part == Part::Option
    }

    fn upsert_item(&mut self, item: Item) {
        self.items.upsert(item);
    }

    fn remove_item(&mut self, key: &str) {
        let neighbour = self.items.remove(key);
        if self.highlighted.as_deref() == Some(key) {
            self.highlighted = neighbour;
        }
    }

    fn set_order(&mut self, rendered: &[(String, String)]) {
        self.items.set_order(rendered);
    }

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match (part, item) {
            (Part::Group, _) => Semantics {
                role: Some(Role::Group),
                ..Default::default()
            },
            (Part::GroupLabel, _) => Semantics::default(),
            (Part::Root, _) => Semantics {
                role: Some(Role::Listbox),
                multiselectable: Some(self.opts.mode == Mode::Multiple),
                orientation: Some(self.opts.orientation),
                ..Default::default()
            },
            (Part::Option, Some(key)) => Semantics {
                role: Some(Role::Option),
                selected: Some(self.is_selected(key)),
                disabled: !self.items.is_enabled(key),
                focusable: if self.tab_stop() == Some(key) {
                    Focusable::TabStop
                } else {
                    Focusable::Programmatic
                },
                active: self.highlighted.as_deref() == Some(key),
                ..Default::default()
            },
            (Part::Option, None) => Semantics {
                role: Some(Role::Option),
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        if crate::roving::is_platform_arrow(&input) {
            return Reaction::default();
        }
        match input {
            Input::Key { key, mods, now_ms } => self.on_key(key, mods, now_ms, gate),
            Input::Focus {
                item: Some(item), ..
            } => {
                if self.items.is_enabled(&item) {
                    self.highlighted = Some(item);
                }
                Reaction::default()
            }
            Input::Press {
                item: Some(item),
                mods,
                ..
            } => {
                if !self.items.is_enabled(&item) {
                    return Reaction::default();
                }
                self.highlighted = Some(item.clone());
                self.select(&item, mods, Reason::Press, gate);
                Reaction::to_item(Part::Option, Some(item))
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        self.selected.clone()
    }

    fn set_value(&mut self, value: Vec<String>) {
        let limit = if self.opts.mode == Mode::Single {
            1
        } else {
            usize::MAX
        };
        self.selected = self
            .items
            .iter()
            .filter(|item| value.contains(&item.key))
            .map(|item| item.key.clone())
            .take(limit)
            .collect();
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "listbox",
            label: label.to_string(),
            value: self.selected.clone(),
            expanded: None,
            actions: ACTIONS,
            items: self
                .items
                .iter()
                .map(|item| SnapshotItem {
                    key: item.key.clone(),
                    label: item.label.clone(),
                    disabled: item.disabled,
                    selected: self.is_selected(&item.key),
                })
                .collect(),
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let action = match action {
            "clear" => "clear",
            "select" => "select",
            "deselect" => "deselect",
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        // Selecting an option also moves the highlight to it.
        let mut landed = None;
        let next = if action == "clear" {
            Vec::new()
        } else {
            let key = item.ok_or(ActError::MissingItem { action })?;
            let found = self
                .items
                .get(key)
                .ok_or_else(|| ActError::UnknownItem(key.to_string()))?;
            if found.disabled {
                return Err(ActError::Disabled(key.to_string()));
            }
            if action == "deselect" {
                self.selected
                    .iter()
                    .filter(|k| *k != key)
                    .cloned()
                    .collect()
            } else {
                landed = Some(key.to_string());
                self.with_added(key)
            }
        };
        if self.commit(next, Source::Agent, Reason::Action, gate) {
            if landed.is_some() {
                self.highlighted = landed.clone();
                self.anchor = landed;
            }
            Ok(())
        } else {
            Err(ActError::Refused)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listbox(mode: Mode) -> Listbox {
        let mut lb = Listbox::new(Options {
            mode,
            ..Default::default()
        });
        for (key, label, disabled) in [
            ("apple", "Apple", false),
            ("banana", "Banana", true),
            ("cherry", "Cherry", false),
            ("date", "Date", false),
        ] {
            lb.upsert_item(Item {
                key: key.into(),
                label: label.into(),
                disabled,
            });
        }
        lb
    }

    fn key(lb: &mut Listbox, key: Key, mods: Modifiers) -> Reaction<Part> {
        lb.send(
            Input::Key {
                key,
                mods,
                now_ms: 0,
            },
            &mut |_| true,
        )
    }

    fn press(lb: &mut Listbox, item: &str, gate: Gate) -> Reaction<Part> {
        lb.send(
            Input::Press {
                part: Part::Option,
                item: Some(item.into()),
                mods: NONE,
            },
            gate,
        )
    }

    const NONE: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
    };
    const SHIFT: Modifiers = Modifiers {
        shift: true,
        ctrl: false,
        alt: false,
    };

    #[test]
    fn arrows_move_focus_past_disabled_items_without_selecting() {
        let mut lb = listbox(Mode::Single);
        assert_eq!(
            key(&mut lb, Key::ArrowDown, NONE).focus_item(),
            Some("apple")
        );
        assert_eq!(
            key(&mut lb, Key::ArrowDown, NONE).focus_item(),
            Some("cherry")
        );
        assert_eq!(lb.value(), Vec::<String>::new());
    }

    #[test]
    fn arrows_continue_from_wherever_focus_landed() {
        let mut lb = listbox(Mode::Single);
        lb.send(
            Input::Focus {
                part: Part::Option,
                item: Some("cherry".into()),
            },
            &mut |_| true,
        );
        assert_eq!(
            key(&mut lb, Key::ArrowDown, NONE).focus_item(),
            Some("date")
        );
    }

    #[test]
    fn space_selects_the_highlighted_item() {
        let mut lb = listbox(Mode::Single);
        key(&mut lb, Key::End, NONE);
        key(&mut lb, Key::Space, NONE);
        assert_eq!(lb.value(), ["date"]);
        assert_eq!(
            lb.semantics(Part::Option, Some("date")).selected,
            Some(true)
        );
    }

    #[test]
    fn shift_arrow_extends_a_multiple_selection() {
        let mut lb = listbox(Mode::Multiple);
        press(&mut lb, "apple", &mut |_| true);
        key(&mut lb, Key::ArrowDown, SHIFT);
        key(&mut lb, Key::ArrowDown, SHIFT);
        assert_eq!(lb.value(), ["apple", "cherry", "date"]);
    }

    #[test]
    fn typing_jumps_to_a_matching_item() {
        let mut lb = listbox(Mode::Single);
        let reaction = key(&mut lb, Key::Char('d'), NONE);
        assert_eq!(reaction.focus_item(), Some("date"));
        assert!(!key(&mut lb, Key::Char('z'), NONE).handled);
    }

    #[test]
    fn the_owner_can_set_the_value_but_only_to_real_items() {
        let mut lb = listbox(Mode::Multiple);
        lb.set_value(vec!["date".into(), "ghost".into(), "apple".into()]);
        assert_eq!(lb.value(), ["apple", "date"]);
        let mut single = listbox(Mode::Single);
        single.set_value(vec!["date".into(), "apple".into()]);
        assert_eq!(single.value(), ["apple"]);
    }

    #[test]
    fn exactly_one_option_is_a_tab_stop() {
        let mut lb = listbox(Mode::Single);
        let stops = |lb: &Listbox| {
            ["apple", "banana", "cherry", "date"]
                .into_iter()
                .filter(|k| lb.semantics(Part::Option, Some(k)).focusable == Focusable::TabStop)
                .collect::<Vec<_>>()
        };
        assert_eq!(stops(&lb), ["apple"]);
        key(&mut lb, Key::End, NONE);
        assert_eq!(stops(&lb), ["date"]);
    }

    #[test]
    fn pressing_a_disabled_item_does_nothing() {
        let mut lb = listbox(Mode::Single);
        assert_eq!(press(&mut lb, "banana", &mut |_| true), Reaction::default());
        assert_eq!(lb.value(), Vec::<String>::new());
    }

    #[test]
    fn the_owner_can_cancel_a_change() {
        let mut lb = listbox(Mode::Single);
        let mut seen = Vec::new();
        press(&mut lb, "apple", &mut |change| {
            seen.push(change.clone());
            false
        });
        assert_eq!(lb.value(), Vec::<String>::new());
        assert_eq!(seen[0].value, ["apple"]);
        assert_eq!(seen[0].source, Source::User);
    }

    #[test]
    fn removing_the_highlighted_item_recovers_to_a_neighbour() {
        let mut lb = listbox(Mode::Single);
        key(&mut lb, Key::End, NONE);
        lb.remove_item("date");
        assert_eq!(lb.highlighted(), Some("cherry"));
    }

    #[test]
    fn set_order_follows_the_rendered_order_and_fills_labels() {
        let mut lb = Listbox::default();
        lb.upsert_item(Item {
            key: "b".into(),
            label: String::new(),
            disabled: false,
        });
        lb.upsert_item(Item {
            key: "a".into(),
            label: String::new(),
            disabled: false,
        });
        lb.set_order(&[("a".into(), " Alpha ".into()), ("b".into(), "Beta".into())]);
        let labels: Vec<_> = lb.snapshot("").items.into_iter().map(|i| i.label).collect();
        assert_eq!(labels, ["Alpha", "Beta"]);
    }

    #[test]
    fn agent_actions_change_the_value_and_are_marked_as_agent() {
        let mut lb = listbox(Mode::Multiple);
        let mut sources = Vec::new();
        let mut gate = |change: &ValueChange| {
            sources.push(change.source);
            true
        };
        lb.act("select", Some("date"), &mut gate).unwrap();
        lb.act("select", Some("apple"), &mut gate).unwrap();
        lb.act("deselect", Some("date"), &mut gate).unwrap();
        assert_eq!(lb.value(), ["apple"]);
        assert_eq!(sources, [Source::Agent; 3]);
        assert!(lb.snapshot("Fruit").items[0].selected);
    }

    #[test]
    fn agent_errors_say_what_would_have_worked() {
        let mut lb = listbox(Mode::Single);
        let err = lb.act("open", None, &mut |_| true).unwrap_err();
        assert_eq!(
            err.to_string(),
            "unknown action \"open\"; valid actions: select, deselect, clear"
        );
        assert_eq!(
            lb.act("select", Some("banana"), &mut |_| true),
            Err(ActError::Disabled("banana".into()))
        );
        assert_eq!(
            lb.act("select", Some("apple"), &mut |_| false),
            Err(ActError::Refused)
        );
        assert_eq!(lb.highlighted(), None, "a refused action leaves no trace");
    }

    #[test]
    fn a_group_of_options_is_named_by_its_heading() {
        let list = Listbox::default();
        assert_eq!(
            list.semantics(Part::Group, Some("citrus")).role,
            Some(Role::Group)
        );
        assert_eq!(
            Listbox::relations(Part::Group).labelled_by,
            Some(Part::GroupLabel)
        );
    }
}
