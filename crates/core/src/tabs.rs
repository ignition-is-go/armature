use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::items::Items;
use crate::keys::{Dir, Key, Orientation};
use crate::primitive::{
    Focusable, Input, Item, PartOf, Primitive, Reaction, Relations, Role, Semantics,
};
use crate::roving::{self, Ends};

/// When a tab shows its panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Activation {
    /// As soon as focus reaches it.
    #[default]
    Automatic,
    /// When Enter or Space is pressed on it. The arrows only move focus.
    Manual,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub orientation: Orientation,
    pub dir: Dir,
    pub activation: Activation,
    /// Whether the arrows come round from the last tab to the first.
    pub ends: Ends,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            orientation: Orientation::Horizontal,
            dir: Dir::Ltr,
            activation: Activation::Automatic,
            ends: Ends::Wrap,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    List,
    Tab,
    /// The content for one tab. Takes the tab's key as its item.
    Panel,
}

const ACTIONS: &[&str] = &["select"];

/// Tabs. Moving focus to a tab selects it, unless `manual` is set, when Enter or
/// Space does.
#[derive(Clone, Debug, Default)]
pub struct Tabs {
    opts: Options,
    items: Items,
    selected: Option<String>,
    focused: Option<String>,
}

impl Tabs {
    #[must_use]
    pub fn new(opts: Options) -> Self {
        Self {
            opts,
            ..Default::default()
        }
    }

    /// The chosen tab, or the first enabled one until the user chooses.
    fn current(&self) -> Option<&str> {
        self.selected
            .as_deref()
            .filter(|key| self.items.is_enabled(key))
            .or_else(|| self.items.first_enabled())
    }

    fn select(&mut self, key: &str, source: Source, reason: Reason, gate: Gate) -> bool {
        if self.current() == Some(key) {
            return true;
        }
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
    type Primitive = Tabs;
}

impl Primitive for Tabs {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::List, Part::Tab, Part::Panel]
    }

    fn is_item(part: Part) -> bool {
        part == Part::Tab
    }

    fn relations(part: Part) -> Relations<Part> {
        match part {
            Part::Tab => Relations {
                controls: Some(Part::Panel),
                ..Default::default()
            },
            Part::Panel => Relations {
                labelled_by: Some(Part::Tab),
                ..Default::default()
            },
            Part::List => Relations::default(),
        }
    }

    fn upsert_item(&mut self, item: Item) {
        self.items.upsert(item);
    }

    fn remove_item(&mut self, key: &str) {
        let neighbour = self.items.remove(key);
        if self.selected.as_deref() == Some(key) {
            self.selected = neighbour.clone();
        }
        if self.focused.as_deref() == Some(key) {
            self.focused = neighbour;
        }
    }

    fn set_order(&mut self, rendered: &[(String, String)]) {
        self.items.set_order(rendered);
    }

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        let is_current = item.is_some() && self.current() == item;
        match part {
            Part::List => Semantics {
                role: Some(Role::TabList),
                orientation: Some(self.opts.orientation),
                ..Default::default()
            },
            Part::Tab => Semantics {
                role: Some(Role::Tab),
                selected: Some(is_current),
                disabled: !item.is_some_and(|key| self.items.is_enabled(key)),
                focusable: if is_current {
                    Focusable::TabStop
                } else {
                    Focusable::Programmatic
                },
                active: item.is_some() && self.focused.as_deref() == item,
                ..Default::default()
            },
            Part::Panel => Semantics {
                role: Some(Role::TabPanel),
                hidden: !is_current,
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        if roving::is_platform_arrow(&input) {
            return Reaction::default();
        }
        match input {
            Input::Key { key, .. } => {
                if let Some(nav) = roving::nav_for_key(key, self.opts.orientation, self.opts.dir) {
                    let from = self.focused.as_deref().or_else(|| self.current());
                    let focus = self.items.step(from, nav, self.opts.ends);
                    if let Some(key) = &focus {
                        self.focused = Some(key.clone());
                        if self.opts.activation == Activation::Automatic {
                            self.select(key, Source::User, Reason::Keyboard, gate);
                        }
                    }
                    return Reaction::to_item(Part::Tab, focus);
                }
                match (key, self.focused.clone()) {
                    (Key::Enter | Key::Space, Some(key)) => {
                        self.select(&key, Source::User, Reason::Keyboard, gate);
                        Reaction::handled()
                    }
                    _ => Reaction::default(),
                }
            }
            Input::Focus {
                item: Some(item), ..
            } => {
                if self.items.is_enabled(&item) {
                    self.focused = Some(item);
                }
                Reaction::default()
            }
            Input::Press {
                item: Some(item), ..
            } => {
                if !self.items.is_enabled(&item) {
                    return Reaction::default();
                }
                self.focused = Some(item.clone());
                self.select(&item, Source::User, Reason::Press, gate);
                Reaction::to_item(Part::Tab, Some(item))
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        self.current().map(str::to_string).into_iter().collect()
    }

    fn set_value(&mut self, value: Vec<String>) {
        if let Some(key) = value.into_iter().find(|key| self.items.is_enabled(key)) {
            self.selected = Some(key);
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "tabs",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: self
                .items
                .iter()
                .map(|item| SnapshotItem {
                    key: item.key.clone(),
                    label: item.label.clone(),
                    disabled: item.disabled,
                    selected: self.current() == Some(item.key.as_str()),
                })
                .collect(),
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        if action != "select" {
            return Err(ActError::UnknownAction {
                action: action.to_string(),
                valid: ACTIONS,
            });
        }
        let key = item.ok_or(ActError::MissingItem { action: "select" })?;
        let found = self
            .items
            .get(key)
            .ok_or_else(|| ActError::UnknownItem(key.to_string()))?;
        if found.disabled {
            return Err(ActError::Disabled(key.to_string()));
        }
        if self.select(key, Source::Agent, Reason::Action, gate) {
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

    fn tabs() -> Tabs {
        let mut tabs = Tabs::default();
        for (key, disabled) in [("overview", false), ("billing", true), ("usage", false)] {
            tabs.upsert_item(Item {
                key: key.into(),
                label: key.into(),
                disabled,
            });
        }
        tabs
    }

    fn key(tabs: &mut Tabs, key: Key) -> Reaction<Part> {
        let mods = Modifiers::default();
        tabs.send(
            Input::Key {
                key,
                mods,
                now_ms: 0,
            },
            &mut |_| true,
        )
    }

    #[test]
    fn the_first_enabled_tab_is_current_and_only_its_panel_shows() {
        let tabs = tabs();
        assert_eq!(tabs.value(), ["overview"]);
        assert!(!tabs.semantics(Part::Panel, Some("overview")).hidden);
        assert!(tabs.semantics(Part::Panel, Some("usage")).hidden);
    }

    #[test]
    fn arrows_select_as_they_move_and_wrap_past_disabled_tabs() {
        let mut tabs = tabs();
        assert_eq!(key(&mut tabs, Key::ArrowRight).focus_item(), Some("usage"));
        assert_eq!(tabs.value(), ["usage"]);
        assert_eq!(
            key(&mut tabs, Key::ArrowRight).focus_item(),
            Some("overview")
        );
        assert_eq!(tabs.value(), ["overview"]);
    }

    #[test]
    fn vertical_arrows_are_left_alone_in_a_horizontal_tab_list() {
        let mut tabs = tabs();
        assert!(!key(&mut tabs, Key::ArrowDown).handled);
    }

    #[test]
    fn only_the_current_tab_is_a_tab_stop() {
        let mut tabs = tabs();
        key(&mut tabs, Key::End);
        assert_eq!(
            tabs.semantics(Part::Tab, Some("usage")).focusable,
            Focusable::TabStop
        );
        assert_eq!(
            tabs.semantics(Part::Tab, Some("overview")).focusable,
            Focusable::Programmatic
        );
    }

    #[test]
    fn a_refused_change_still_moves_focus() {
        let mut tabs = tabs();
        let mods = Modifiers::default();
        let reaction = tabs.send(
            Input::Key {
                key: Key::End,
                mods,
                now_ms: 0,
            },
            &mut |_| false,
        );
        assert_eq!(reaction.focus_item(), Some("usage"));
        assert_eq!(tabs.value(), ["overview"]);
    }

    #[test]
    fn an_agent_can_switch_tabs_but_not_to_a_disabled_one() {
        let mut tabs = tabs();
        tabs.act("select", Some("usage"), &mut |_| true).unwrap();
        assert_eq!(tabs.value(), ["usage"]);
        assert_eq!(
            tabs.act("select", Some("billing"), &mut |_| true),
            Err(ActError::Disabled("billing".into()))
        );
    }

    #[test]
    fn with_manual_activation_the_arrows_only_move_focus() {
        let mut tabs = Tabs::new(Options {
            activation: Activation::Manual,
            ends: Ends::Stop,
            ..Default::default()
        });
        for key in ["one", "two"] {
            tabs.upsert_item(Item {
                key: key.into(),
                label: key.into(),
                disabled: false,
            });
        }
        let key = |tabs: &mut Tabs, key| {
            tabs.send(
                Input::Key {
                    key,
                    mods: Modifiers::default(),
                    now_ms: 0,
                },
                &mut |_| true,
            )
        };
        assert_eq!(key(&mut tabs, Key::ArrowRight).focus_item(), Some("two"));
        assert_eq!(tabs.value(), ["one"], "focus moved, the tab did not");
        assert_eq!(
            key(&mut tabs, Key::ArrowRight).focus_item(),
            None,
            "no coming round"
        );
        key(&mut tabs, Key::Enter);
        assert_eq!(tabs.value(), ["two"]);
    }
}
