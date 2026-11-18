use std::collections::{HashMap, HashSet};

use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::Key;
use crate::popup::{Align, Placement, Side};
use crate::primitive::{
    Focusable, Input, Item, PartOf, Primitive, Reaction, Relations, Role, Semantics,
};
use crate::roving::{self, Nav};
use crate::typeahead::Typeahead;

/// What an entry in a menu does when chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Runs something and closes the menu.
    Action,
    /// Turns on and off. The menu stays open.
    Checkbox,
    /// One of the entries sharing this group name is on at a time.
    Radio(String),
    /// Opens the entries whose `parent` is this one, in place of the current ones.
    Submenu,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub label: String,
    pub kind: Kind,
    /// The submenu this entry sits in, or `None` at the top.
    pub parent: Option<String>,
    pub disabled: bool,
    /// A key that runs the entry from anywhere, with the menu open or closed.
    pub shortcut: Option<crate::shortcut::Shortcut>,
    /// Whether choosing it destroys something, so it can be drawn as a warning.
    pub destructive: bool,
}

impl Entry {
    #[must_use]
    pub fn action(key: &str, label: &str) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            kind: Kind::Action,
            parent: None,
            disabled: false,
            shortcut: None,
            destructive: false,
        }
    }

    #[must_use]
    pub fn of(mut self, kind: Kind) -> Self {
        self.kind = kind;
        self
    }

    #[must_use]
    pub fn inside(mut self, parent: &str) -> Self {
        self.parent = Some(parent.into());
        self
    }

    #[must_use]
    pub const fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    #[must_use]
    pub const fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    /// Gives the entry a key written as `mod+shift+n`. One that cannot be read is
    /// ignored.
    #[must_use]
    pub fn keys(mut self, shortcut: &str) -> Self {
        self.shortcut = crate::shortcut::Shortcut::parse(shortcut);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Trigger,
    /// The popup. Draw the entries of `shown` inside it.
    Content,
    /// One entry, keyed by its key.
    Item,
    /// Optional. A region whose context-menu gesture, such as a right click, opens
    /// the menu where the pointer is.
    Area,
}

const ACTIONS: &[&str] = &["open", "close", "run", "toggle"];

/// A menu of actions, checkboxes, radio choices and submenus, opened from a button.
///
/// A submenu takes the place of the entries it was opened from, and `ArrowLeft` or
/// Escape goes back. Choosing an action offers `run:<key>` to the gate. The value is
/// what is on: a checkbox's key, and `group=key` for each radio group.
#[derive(Clone, Debug, Default)]
pub struct Menu {
    entries: Vec<Entry>,
    open: bool,
    /// The submenus opened, outermost first.
    path: Vec<String>,
    active: Option<String>,
    checked: HashSet<String>,
    chosen: HashMap<String, String>,
    typeahead: Typeahead,
    /// Where the pointer was when the menu was called up as a context menu.
    point: Option<crate::geometry::Point>,
    disabled: bool,
    close_on_select: bool,
}

impl Menu {
    #[must_use]
    pub fn new(entries: Vec<Entry>) -> Self {
        Self {
            entries,
            ..Default::default()
        }
    }

    /// A menu whose trigger is disabled, so no one can open it by hand.
    #[must_use]
    pub const fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    /// Closes the menu when a checkbox or radio entry is chosen too.
    #[must_use]
    pub const fn closing_on_select(mut self) -> Self {
        self.close_on_select = true;
        self
    }

    /// Whether choosing the entry destroys something.
    #[must_use]
    pub fn is_destructive(&self, key: &str) -> bool {
        self.entry(key).is_some_and(|entry| entry.destructive)
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// The entries of the level on show: the top, or the submenu last opened.
    #[must_use]
    pub fn shown(&self) -> Vec<&Entry> {
        let level = self.path.last();
        self.entries
            .iter()
            .filter(|entry| entry.parent.as_ref() == level)
            .collect()
    }

    /// The submenus opened to reach the level on show, for a heading or back button.
    #[must_use]
    pub fn path(&self) -> &[String] {
        &self.path
    }

    #[must_use]
    pub fn is_on(&self, key: &str) -> bool {
        match self.entry(key).map(|entry| &entry.kind) {
            Some(Kind::Checkbox) => self.checked.contains(key),
            Some(Kind::Radio(group)) => self.chosen.get(group).is_some_and(|chosen| chosen == key),
            _ => false,
        }
    }

    fn entry(&self, key: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.key == key)
    }

    fn written(&self, checked: &HashSet<String>, chosen: &HashMap<String, String>) -> Vec<String> {
        self.entries
            .iter()
            .filter_map(|entry| match &entry.kind {
                Kind::Checkbox if checked.contains(&entry.key) => Some(entry.key.clone()),
                Kind::Radio(group) if chosen.get(group) == Some(&entry.key) => {
                    Some(format!("{group}={}", entry.key))
                }
                _ => None,
            })
            .collect()
    }

    fn close(&mut self) {
        self.point = None;
        self.open = false;
        self.path.clear();
        self.active = None;
    }

    fn open_at(&mut self, last: bool) -> Reaction<Part> {
        self.open = true;
        self.path.clear();
        self.active = self.edge(last);
        Reaction::to_item(Part::Item, self.active.clone())
    }

    fn edge(&self, last: bool) -> Option<String> {
        let mut enabled = self.shown().into_iter().filter(|entry| !entry.disabled);
        if last {
            enabled.next_back()
        } else {
            enabled.next()
        }
        .map(|entry| entry.key.clone())
    }

    /// Does what an entry does. Returns where focus goes.
    fn choose(
        &mut self,
        key: &str,
        source: Source,
        reason: Reason,
        gate: Gate,
    ) -> Result<Reaction<Part>, ActError> {
        let entry = self
            .entry(key)
            .ok_or_else(|| ActError::UnknownItem(key.to_string()))?
            .clone();
        if entry.disabled {
            return Err(ActError::Disabled(key.to_string()));
        }
        match entry.kind {
            Kind::Action => {
                if !gate(&ValueChange {
                    value: vec![format!("run:{key}")],
                    source,
                    reason,
                }) {
                    return Err(ActError::Refused);
                }
                self.close();
                Ok(Reaction::focusing(Part::Trigger))
            }
            Kind::Submenu => {
                self.path.push(entry.key);
                self.active = self.edge(false);
                Ok(Reaction::to_item(Part::Item, self.active.clone()))
            }
            Kind::Checkbox | Kind::Radio(_) => self.switch(&entry, source, reason, gate),
        }
    }

    /// Flips a checkbox entry, or makes a radio entry the one chosen in its group.
    fn switch(
        &mut self,
        entry: &Entry,
        source: Source,
        reason: Reason,
        gate: Gate,
    ) -> Result<Reaction<Part>, ActError> {
        let (mut checked, mut chosen) = (self.checked.clone(), self.chosen.clone());
        match &entry.kind {
            Kind::Radio(group) => drop(chosen.insert(group.clone(), entry.key.clone())),
            _ if !checked.remove(&entry.key) => drop(checked.insert(entry.key.clone())),
            _ => {}
        }
        let value = self.written(&checked, &chosen);
        let changes = value != self.value();
        if changes
            && !gate(&ValueChange {
                value,
                source,
                reason,
            })
        {
            return Err(ActError::Refused);
        }
        (self.checked, self.chosen) = (checked, chosen);
        if self.close_on_select {
            self.close();
            return Ok(Reaction::focusing(Part::Trigger));
        }
        Ok(Reaction::handled())
    }

    /// Chooses an entry from the keyboard. A refusal is still the menu's to answer.
    fn choose_by_key(&mut self, key: &str, gate: Gate) -> Reaction<Part> {
        self.choose(key, Source::User, Reason::Keyboard, gate)
            .unwrap_or_else(|_| Reaction::handled())
    }

    /// Leaves the submenu on show, landing on the entry that opened it.
    fn back(&mut self) -> Reaction<Part> {
        self.active = self.path.pop();
        Reaction::to_item(Part::Item, self.active.clone())
    }
}

impl Menu {
    /// An entry's own key runs it, whether or not the menu is open.
    fn on_hotkey(&mut self, shortcut: crate::shortcut::Shortcut, gate: Gate) -> Reaction<Part> {
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.shortcut == Some(shortcut) && entry.kind != Kind::Submenu);
        let Some(key) = entry.map(|entry| entry.key.clone()) else {
            return Reaction::default();
        };
        let was = (self.open, self.path.clone(), self.active.clone());
        let ran = self
            .choose(&key, Source::User, Reason::Keyboard, gate)
            .is_ok();
        // Running one by its key leaves the menu as it was, and focus too.
        if !was.0 {
            (self.open, self.path, self.active) = was;
        }
        Reaction {
            handled: ran,
            ..Reaction::default()
        }
    }

    /// A key pressed while the menu is open: moving between entries, opening and
    /// leaving submenus, choosing, closing, and typing to find an entry.
    fn on_key_while_open(
        &mut self,
        key: Key,
        mods: crate::keys::Modifiers,
        now_ms: u64,
        gate: Gate,
    ) -> Reaction<Part> {
        let shown: Vec<Entry> = self.shown().into_iter().cloned().collect();
        let enabled: Vec<bool> = shown.iter().map(|entry| !entry.disabled).collect();
        let at = self
            .active
            .as_deref()
            .and_then(|key| shown.iter().position(|entry| entry.key == key));
        let go = |nav| {
            roving::step(&enabled, at, nav, true)
                .and_then(|index| shown.get(index))
                .map(|entry| entry.key.clone())
        };
        let target = match key {
            Key::ArrowDown => go(Nav::Next),
            Key::ArrowUp => go(Nav::Prev),
            Key::Home => go(Nav::First),
            Key::End => go(Nav::Last),
            Key::Enter | Key::Space => {
                let Some(key) = self.active.clone() else {
                    return Reaction::handled();
                };
                return self.choose_by_key(&key, gate);
            }
            Key::ArrowRight => {
                let submenu = at
                    .and_then(|at| shown.get(at))
                    .filter(|entry| entry.kind == Kind::Submenu && !entry.disabled);
                let Some(key) = submenu.map(|entry| entry.key.clone()) else {
                    return Reaction::default();
                };
                return self.choose_by_key(&key, gate);
            }
            Key::ArrowLeft | Key::Escape if !self.path.is_empty() => return self.back(),
            Key::Escape => {
                self.close();
                return Reaction::focusing(Part::Trigger);
            }
            // Tab closes the menu and is left to move focus on from the trigger.
            Key::Tab => {
                self.close();
                return Reaction {
                    handled: false,
                    ..Reaction::focusing(Part::Trigger)
                };
            }
            Key::Char(ch) if !mods.ctrl && !mods.alt => {
                let labels: Vec<(&str, bool)> = shown
                    .iter()
                    .map(|entry| (entry.label.as_str(), !entry.disabled))
                    .collect();
                self.typeahead
                    .push(ch, now_ms, &labels, at)
                    .and_then(|index| shown.get(index))
                    .map(|entry| entry.key.clone())
            }
            _ => return Reaction::default(),
        };
        if let Some(key) = &target {
            self.active = Some(key.clone());
        }
        Reaction::to_item(Part::Item, target)
    }
}

impl PartOf for Part {
    type Primitive = Menu;
}

impl Primitive for Menu {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Trigger, Part::Content, Part::Item, Part::Area]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn hovers(part: Part) -> bool {
        part == Part::Item
    }

    fn context(part: Part) -> bool {
        part == Part::Area
    }

    fn hotkeys(&self) -> Vec<crate::shortcut::Shortcut> {
        self.entries
            .iter()
            .filter(|entry| !entry.disabled)
            .filter_map(|entry| entry.shortcut)
            .collect()
    }

    fn popup_point(&self) -> Option<crate::geometry::Point> {
        self.point.filter(|_| self.open)
    }

    fn popup_of(part: Part) -> Option<Placement<Part>> {
        (part == Part::Content).then_some(Placement {
            anchor: Part::Trigger,
            side: Side::Bottom,
            align: Align::Start,
            gap: 4.0,
        })
    }

    fn relations(part: Part) -> Relations<Part> {
        match part {
            Part::Trigger => Relations {
                controls: Some(Part::Content),
                ..Default::default()
            },
            Part::Content => Relations {
                labelled_by: Some(Part::Trigger),
                ..Default::default()
            },
            Part::Item | Part::Area => Relations::default(),
        }
    }

    fn named_part(&self) -> Option<Part> {
        Some(Part::Trigger)
    }

    /// The menu owns its entries. What is rendered only renames them.
    fn upsert_item(&mut self, item: Item) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.key == item.key && !item.label.is_empty())
        {
            entry.label = item.label;
        }
    }

    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match (part, item.and_then(|key| self.entry(key))) {
            (Part::Trigger, _) => Semantics {
                role: Some(Role::Button),
                expanded: Some(self.open),
                disabled: self.disabled,
                focusable: Focusable::TabStop,
                ..Default::default()
            },
            (Part::Content, _) => Semantics {
                role: Some(Role::Menu),
                hidden: !self.open,
                ..Default::default()
            },
            (Part::Item, Some(entry)) => {
                let on_show = self.open && entry.parent.as_ref() == self.path.last();
                let (role, checked) = match entry.kind {
                    Kind::Action | Kind::Submenu => (Role::MenuItem, None),
                    Kind::Checkbox => (Role::MenuItemCheckbox, Some(self.is_on(&entry.key))),
                    Kind::Radio(_) => (Role::MenuItemRadio, Some(self.is_on(&entry.key))),
                };
                Semantics {
                    role: Some(role),
                    checked,
                    expanded: (entry.kind == Kind::Submenu).then_some(false),
                    disabled: entry.disabled,
                    destructive: entry.destructive,
                    hidden: !on_show,
                    focusable: if on_show {
                        Focusable::Programmatic
                    } else {
                        Focusable::No
                    },
                    active: on_show && self.active.as_deref() == Some(entry.key.as_str()),
                    ..Default::default()
                }
            }
            (Part::Area, _) => Semantics::default(),
            (Part::Item, None) => Semantics {
                role: Some(Role::MenuItem),
                hidden: item.is_some(),
                ..Default::default()
            },
        }
    }
    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        // A disabled trigger never opens the menu.
        if self.disabled
            && !self.open
            && matches!(
                input,
                Input::Press { .. } | Input::Key { .. } | Input::ContextMenu { .. }
            )
        {
            return Reaction::default();
        }
        match (input, self.open) {
            // An entry's own key runs it, whether or not the menu is open.
            (Input::Hotkey { shortcut }, _) => self.on_hotkey(shortcut, gate),
            // Called up where the pointer is, as a context menu.
            (Input::ContextMenu { at, .. }, _) => {
                let reaction = self.open_at(false);
                self.point = Some(at);
                reaction
            }
            (
                Input::Press {
                    part: Part::Trigger,
                    ..
                }
                | Input::Key {
                    key: Key::Enter | Key::Space | Key::ArrowDown,
                    ..
                },
                false,
            ) => self.open_at(false),
            (
                Input::Press {
                    part: Part::Trigger,
                    ..
                },
                true,
            ) => {
                self.close();
                Reaction::handled()
            }
            (
                Input::Key {
                    key: Key::ArrowUp, ..
                },
                false,
            ) => self.open_at(true),
            (Input::PressOutside | Input::WindowBlur | Input::Blur { .. }, true) => {
                self.close();
                Reaction::default()
            }
            (
                Input::Press {
                    part: Part::Item,
                    item: Some(key),
                    ..
                },
                true,
            ) => self
                .choose(&key, Source::User, Reason::Press, gate)
                .unwrap_or_else(|_| Reaction::handled()),
            // The entry under the pointer is the one the keyboard would act on.
            (
                Input::Hover {
                    part: Part::Item,
                    item: Some(key),
                    inside: true,
                    ..
                },
                true,
            ) => {
                if self.entry(&key).is_some_and(|entry| !entry.disabled) {
                    self.active = Some(key);
                }
                Reaction::default()
            }
            (
                Input::Focus {
                    part: Part::Item,
                    item: Some(key),
                },
                true,
            ) => {
                self.active = Some(key);
                Reaction::default()
            }
            (Input::Key { key, mods, now_ms }, true) => {
                self.on_key_while_open(key, mods, now_ms, gate)
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        self.written(&self.checked, &self.chosen)
    }

    fn set_value(&mut self, value: Vec<String>) {
        self.checked.clear();
        self.chosen.clear();
        for entry in value {
            match entry.split_once('=') {
                Some((group, key)) => drop(self.chosen.insert(group.to_string(), key.to_string())),
                None => drop(self.checked.insert(entry)),
            }
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .entries
            .iter()
            .map(|entry| SnapshotItem {
                key: entry.key.clone(),
                label: entry.label.clone(),
                disabled: entry.disabled,
                selected: false,
            })
            .collect();
        Snapshot {
            name: "menu",
            label: label.to_string(),
            value: self.value(),
            expanded: Some(self.open),
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let action: &'static str = match action {
            "open" => {
                self.open_at(false);
                return Ok(());
            }
            "close" => {
                self.close();
                return Ok(());
            }
            "run" => "run",
            "toggle" => "toggle",
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        let key = item.ok_or(ActError::MissingItem { action })?;
        // An agent names the entry directly, whichever submenu it is in.
        let was = (self.open, self.path.clone(), self.active.clone());
        let result = self
            .choose(key, Source::Agent, Reason::Action, gate)
            .map(|_| ());
        if self
            .entry(key)
            .is_some_and(|entry| entry.kind == Kind::Submenu)
            || result.is_err()
        {
            (self.open, self.path, self.active) = was;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Modifiers;

    fn menu() -> Menu {
        Menu::new(vec![
            Entry::action("new", "New file"),
            Entry::action("open", "Open").disabled(),
            Entry::action("wrap", "Word wrap").of(Kind::Checkbox),
            Entry::action("share", "Share").of(Kind::Submenu),
            Entry::action("email", "Email").inside("share"),
            Entry::action("link", "Copy link").inside("share"),
            Entry::action("light", "Light").of(Kind::Radio("theme".into())),
            Entry::action("dark", "Dark").of(Kind::Radio("theme".into())),
        ])
    }

    fn key(menu: &mut Menu, key: Key, offered: &mut Vec<String>) -> Reaction<Part> {
        menu.send(
            Input::Key {
                key,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |change| {
                offered.extend(change.value.iter().cloned());
                true
            },
        )
    }

    #[test]
    fn the_keyboard_opens_walks_past_disabled_entries_and_runs_one() {
        let (mut menu, mut offered) = (menu(), Vec::new());
        assert_eq!(
            key(&mut menu, Key::ArrowDown, &mut offered).focus_item(),
            Some("new")
        );
        assert_eq!(
            key(&mut menu, Key::ArrowDown, &mut offered).focus_item(),
            Some("wrap")
        );
        assert_eq!(
            key(&mut menu, Key::ArrowUp, &mut offered).focus_item(),
            Some("new")
        );
        let ran = key(&mut menu, Key::Enter, &mut offered);
        assert_eq!(offered, ["run:new"]);
        assert!(!menu.is_open());
        assert_eq!(ran.focus.unwrap().part, Part::Trigger);
    }

    #[test]
    fn checkboxes_and_radios_change_the_value_and_keep_the_menu_open() {
        let (mut menu, mut offered) = (menu(), Vec::new());
        key(&mut menu, Key::ArrowDown, &mut offered);
        key(&mut menu, Key::Char('w'), &mut offered);
        key(&mut menu, Key::Space, &mut offered);
        key(&mut menu, Key::End, &mut offered);
        key(&mut menu, Key::Enter, &mut offered);
        assert_eq!(menu.value(), ["wrap", "theme=dark"]);
        assert!(menu.is_open());
        assert_eq!(menu.semantics(Part::Item, Some("dark")).checked, Some(true));
        assert_eq!(
            menu.semantics(Part::Item, Some("light")).checked,
            Some(false)
        );
    }

    #[test]
    fn a_submenu_takes_over_the_menu_and_escape_steps_back_out() {
        let (mut menu, mut offered) = (menu(), Vec::new());
        key(&mut menu, Key::ArrowDown, &mut offered);
        key(&mut menu, Key::Char('s'), &mut offered);
        assert_eq!(
            key(&mut menu, Key::ArrowRight, &mut offered).focus_item(),
            Some("email")
        );
        assert_eq!(
            menu.shown()
                .iter()
                .map(|entry| entry.key.as_str())
                .collect::<Vec<_>>(),
            ["email", "link"]
        );
        assert!(menu.semantics(Part::Item, Some("new")).hidden);
        assert_eq!(
            key(&mut menu, Key::Escape, &mut offered).focus_item(),
            Some("share")
        );
        assert!(menu.is_open());
        key(&mut menu, Key::Escape, &mut offered);
        assert!(!menu.is_open());
    }

    #[test]
    fn an_agent_runs_an_entry_inside_a_submenu_by_name() {
        let mut menu = menu();
        let mut offered = Vec::new();
        menu.act("run", Some("link"), &mut |change| {
            offered.extend(change.value.iter().cloned());
            true
        })
        .unwrap();
        assert_eq!(offered, ["run:link"]);
        assert_eq!(
            menu.act("run", Some("open"), &mut |_| true),
            Err(ActError::Disabled("open".into()))
        );
    }

    #[test]
    fn a_context_gesture_opens_the_menu_where_the_pointer_is() {
        let mut menu = menu();
        let at = crate::geometry::Point::new(120.0, 80.0);
        let opened = menu.send(
            Input::ContextMenu {
                part: Part::Area,
                item: None,
                at,
            },
            &mut |_| true,
        );
        assert!(menu.is_open());
        assert_eq!(opened.focus_item(), Some("new"));
        assert_eq!(menu.popup_point(), Some(at));
        menu.send(Input::PressOutside, &mut |_| true);
        assert_eq!(menu.popup_point(), None);
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(menu, &[]);
    }

    #[test]
    fn an_entry_runs_from_its_own_key_with_the_menu_closed() {
        let mut menu = Menu::new(vec![
            Entry::action("new", "New file").keys("mod+n"),
            Entry::action("wrap", "Word wrap")
                .of(Kind::Checkbox)
                .keys("alt+z"),
        ]);
        assert_eq!(menu.hotkeys().len(), 2);
        let mut offered = Vec::new();
        let mut press = |menu: &mut Menu, keys: &str| {
            let shortcut = crate::shortcut::Shortcut::parse(keys).unwrap();
            menu.send(Input::Hotkey { shortcut }, &mut |change| {
                offered.extend(change.value.iter().cloned());
                true
            })
        };
        assert!(press(&mut menu, "mod+n").handled);
        press(&mut menu, "alt+z");
        assert_eq!(offered, ["run:new", "wrap"]);
        assert!(!menu.is_open());
        assert_eq!(menu.value(), ["wrap"]);
    }
}
