use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, ValueChange};
use crate::keys::{Dir, Key, Modifiers};
use crate::menu::{self, Entry, Menu};
use crate::popup::{Align, Placement, Side};
use crate::primitive::{
    Focusable, Input, Item, PartOf, Primitive, Reaction, Relations, Role, Semantics,
};
use crate::roving::Ends;
use crate::roving::{self, Nav};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    /// One menu's title in the bar, keyed by the menu.
    Trigger,
    /// One menu's popup, keyed by the menu. Draw the entries of `shown` inside it.
    Content,
    /// One entry, keyed `menu/entry`.
    Item,
}

const ACTIONS: &[&str] = &["open", "close", "run", "toggle"];

/// One menu of the bar.
#[derive(Clone, Debug)]
struct BarMenu {
    key: String,
    /// What its trigger says.
    title: String,
    menu: Menu,
}

/// A row of menus, as at the top of a desktop application.
///
/// Left and Right move along the bar, and with a menu open they move to the next menu.
/// Entries are keyed `menu/entry`, in the value and in what is offered to the gate.
#[derive(Clone, Debug, Default)]
pub struct Menubar {
    menus: Vec<BarMenu>,
    dir: Dir,
    open: Option<usize>,
    focused: usize,
}

impl Menubar {
    /// Each menu is its key, its title and its entries.
    #[must_use]
    pub fn new(menus: Vec<(&str, &str, Vec<Entry>)>, dir: Dir) -> Self {
        let menus = menus
            .into_iter()
            .map(|(key, title, entries)| BarMenu {
                key: key.to_string(),
                title: title.to_string(),
                menu: Menu::new(entries),
            })
            .collect();
        Self {
            menus,
            dir,
            open: None,
            focused: 0,
        }
    }

    /// The entries on show in a menu: its top level, or the submenu last opened.
    #[must_use]
    pub fn shown(&self, menu: &str) -> Vec<&Entry> {
        self.at(menu)
            .and_then(|at| self.menu_at(at))
            .map(Menu::shown)
            .unwrap_or_default()
    }

    #[must_use]
    pub fn open_menu(&self) -> Option<&str> {
        Some(self.menus.get(self.open?)?.key.as_str())
    }

    fn key_at(&self, at: usize) -> String {
        self.menus
            .get(at)
            .map(|bar| bar.key.clone())
            .unwrap_or_default()
    }

    fn menu_at(&self, at: usize) -> Option<&Menu> {
        self.menus.get(at).map(|bar| &bar.menu)
    }

    fn menu_at_mut(&mut self, at: usize) -> Option<&mut Menu> {
        self.menus.get_mut(at).map(|bar| &mut bar.menu)
    }

    fn close_menu(&mut self, at: usize) {
        if let Some(menu) = self.menu_at_mut(at) {
            menu.act("close", None, &mut |_| true).ok();
        }
    }

    fn at(&self, menu: &str) -> Option<usize> {
        self.menus.iter().position(|bar| bar.key == menu)
    }

    fn split(item: &str) -> (&str, Option<&str>) {
        match item.split_once('/') {
            Some((menu, entry)) => (menu, Some(entry)),
            None => (item, None),
        }
    }

    /// Hands an input to one menu, and translates what it answers back to the bar's
    /// own parts and keys.
    fn forward(&mut self, at: usize, input: Input<menu::Part>, gate: Gate) -> Reaction<Part> {
        let key = self.key_at(at);
        let Some(menu) = self.menu_at_mut(at) else {
            return Reaction::default();
        };
        let reaction = menu.send(input, &mut |change| {
            let value = change
                .value
                .iter()
                .map(|entry| prefixed(&key, entry))
                .collect();
            gate(&ValueChange {
                value,
                source: change.source,
                reason: change.reason,
            })
        });
        if !self.menu_at(at).is_some_and(Menu::is_open) && self.open == Some(at) {
            self.open = None;
        }
        Reaction {
            handled: reaction.handled,
            effects: reaction.effects,
            focus: reaction.focus.map(|target| match target.part {
                menu::Part::Item => crate::primitive::FocusTarget {
                    part: Part::Item,
                    item: target.item.map(|entry| format!("{key}/{entry}")),
                },
                menu::Part::Trigger => crate::primitive::FocusTarget {
                    part: Part::Trigger,
                    item: Some(key.clone()),
                },
                menu::Part::Content | menu::Part::Area => crate::primitive::FocusTarget {
                    part: Part::Content,
                    item: Some(key.clone()),
                },
            }),
        }
    }

    fn open_menu_at(&mut self, at: usize, last: bool, gate: Gate) -> Reaction<Part> {
        if let Some(was) = self.open.filter(|was| *was != at) {
            self.close_menu(was);
        }
        self.open = Some(at);
        self.focused = at;
        let key = if last { Key::ArrowUp } else { Key::ArrowDown };
        self.forward(
            at,
            Input::Key {
                key,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            gate,
        )
    }

    fn neighbour(&self, forward: bool) -> usize {
        let nav = if forward { Nav::Next } else { Nav::Prev };
        roving::step(
            &vec![true; self.menus.len()],
            Some(self.focused),
            nav,
            Ends::Wrap,
        )
        .unwrap_or(self.focused)
    }
}

/// A menu's own value entry, such as `wrap`, `theme=dark` or `run:new`, with the
/// menu's key put in front of the entry it names.
fn prefixed(menu: &str, entry: &str) -> String {
    match entry.split_once(':') {
        Some((verb, key)) => format!("{verb}:{menu}/{key}"),
        None => format!("{menu}/{entry}"),
    }
}

impl Menubar {
    /// The entry an item key names inside its menu.
    fn entry_of(key: &str) -> Option<String> {
        Self::split(key).1.map(str::to_string)
    }

    /// The keys that move forward and back along the bar, for its reading direction.
    fn sideways_keys(&self) -> (Key, Key) {
        if self.dir == Dir::Rtl {
            (Key::ArrowLeft, Key::ArrowRight)
        } else {
            (Key::ArrowRight, Key::ArrowLeft)
        }
    }

    /// A key with every menu closed: moving along the bar, or opening the focused menu.
    fn on_key_on_the_bar(&mut self, key: Key, gate: Gate) -> Reaction<Part> {
        let (forward, back) = self.sideways_keys();
        match key {
            key if key == forward || key == back => {
                self.focused = self.neighbour(key == forward);
                Reaction::to_item(Part::Trigger, Some(self.key_at(self.focused)))
            }
            _ if self.menus.is_empty() => Reaction::default(),
            Key::Enter | Key::Space | Key::ArrowDown => {
                self.open_menu_at(self.focused, false, gate)
            }
            Key::ArrowUp => self.open_menu_at(self.focused, true, gate),
            _ => Reaction::default(),
        }
    }

    /// A key with a menu open goes to that menu. A sideways arrow the menu had no use
    /// for moves to the next menu.
    fn on_key_in_a_menu(
        &mut self,
        at: usize,
        input: Input<menu::Part>,
        key: Key,
        gate: Gate,
    ) -> Reaction<Part> {
        let (forward, back) = self.sideways_keys();
        let reaction = self.forward(at, input, gate);
        if !reaction.handled && reaction.focus.is_none() && (key == forward || key == back) {
            let next = self.neighbour(key == forward);
            return self.open_menu_at(next, false, gate);
        }
        reaction
    }
}

impl PartOf for Part {
    type Primitive = Menubar;
}

impl Primitive for Menubar {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Trigger, Part::Content, Part::Item]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn hovers(part: Part) -> bool {
        matches!(part, Part::Trigger | Part::Item)
    }

    fn popup_of(part: Part) -> Option<Placement<Part>> {
        (part == Part::Content).then_some(Placement {
            anchor: Part::Trigger,
            side: Side::Bottom,
            align: Align::Start,
            gap: 2.0,
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
            _ => Relations::default(),
        }
    }

    fn named_part(&self) -> Option<Part> {
        Some(Part::Root)
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        let (menu, entry) = item.map_or(("", None), Self::split);
        let at = self.at(menu);
        match (part, at) {
            (Part::Root, _) => Semantics {
                role: Some(Role::MenuBar),
                ..Default::default()
            },
            (Part::Trigger, Some(at)) => Semantics {
                role: Some(Role::MenuItem),
                expanded: Some(self.open == Some(at)),
                focusable: if self.focused == at {
                    Focusable::TabStop
                } else {
                    Focusable::Programmatic
                },
                active: self.open == Some(at),
                ..Default::default()
            },
            (Part::Content, Some(at)) => Semantics {
                role: Some(Role::Menu),
                hidden: self.open != Some(at),
                ..Default::default()
            },
            (Part::Item, Some(at)) => self
                .menu_at(at)
                .map(|menu| menu.semantics(menu::Part::Item, entry))
                .unwrap_or_default(),
            (Part::Trigger, None) => Semantics {
                role: Some(Role::MenuItem),
                ..Default::default()
            },
            (_, None) => Semantics {
                hidden: true,
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match (input, self.open) {
            (
                Input::Focus {
                    part: Part::Trigger,
                    item: Some(key),
                },
                _,
            ) => {
                self.focused = self.at(&key).unwrap_or(self.focused);
                Reaction::default()
            }
            (
                Input::Press {
                    part: Part::Trigger,
                    item: Some(key),
                    ..
                },
                open,
            ) => match self.at(&key) {
                Some(at) if open == Some(at) => {
                    self.close_menu(at);
                    self.open = None;
                    Reaction::handled()
                }
                Some(at) => self.open_menu_at(at, false, gate),
                None => Reaction::default(),
            },
            // With a menu open, the pointer moving along the bar opens each in turn.
            (
                Input::Hover {
                    part: Part::Trigger,
                    item: Some(key),
                    inside: true,
                    ..
                },
                Some(open),
            ) => match self.at(&key) {
                Some(at) if at != open => self.open_menu_at(at, false, gate),
                _ => Reaction::default(),
            },
            (
                Input::Press {
                    part: Part::Item,
                    item: Some(key),
                    mods,
                },
                Some(at),
            ) => {
                let press = Input::Press {
                    part: menu::Part::Item,
                    item: Self::entry_of(&key),
                    mods,
                };
                self.forward(at, press, gate)
            }
            (
                Input::Hover {
                    part: Part::Item,
                    item: Some(key),
                    inside,
                    now_ms,
                },
                Some(at),
            ) => {
                let hover = Input::Hover {
                    part: menu::Part::Item,
                    item: Self::entry_of(&key),
                    inside,
                    now_ms,
                };
                self.forward(at, hover, gate)
            }
            (Input::Key { key, .. }, None) => self.on_key_on_the_bar(key, gate),
            (Input::Key { key, mods, now_ms }, Some(at)) => {
                self.on_key_in_a_menu(at, Input::Key { key, mods, now_ms }, key, gate)
            }
            (Input::PressOutside | Input::WindowBlur | Input::Blur { .. }, Some(at)) => {
                self.close_menu(at);
                self.open = None;
                Reaction::default()
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        self.menus
            .iter()
            .flat_map(|BarMenu { key, menu, .. }| {
                menu.value()
                    .into_iter()
                    .map(move |entry| prefixed(key, &entry))
            })
            .collect()
    }

    fn set_value(&mut self, value: Vec<String>) {
        for BarMenu { key, menu, .. } in &mut self.menus {
            let own = value
                .iter()
                .filter_map(|entry| entry.strip_prefix(&format!("{key}/")))
                .map(str::to_string)
                .collect();
            menu.set_value(own);
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .menus
            .iter()
            .flat_map(|BarMenu { key, title, menu }| {
                menu.snapshot("")
                    .items
                    .into_iter()
                    .map(move |item| SnapshotItem {
                        key: format!("{key}/{}", item.key),
                        label: format!("{title} > {}", item.label),
                        ..item
                    })
            })
            .collect();
        Snapshot {
            name: "menubar",
            label: label.to_string(),
            value: self.value(),
            expanded: Some(self.open.is_some()),
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let action: &'static str = match action {
            "close" => {
                let open = self.open.take();
                if let Some(menu) = open.and_then(|at| self.menu_at_mut(at)) {
                    menu.act("close", None, &mut |_| true)?;
                }
                return Ok(());
            }
            "open" => "open",
            "run" => "run",
            "toggle" => "toggle",
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        let item = item.ok_or(ActError::MissingItem { action })?;
        let (menu, entry) = Self::split(item);
        let at = self
            .at(menu)
            .ok_or_else(|| ActError::UnknownItem(item.to_string()))?;
        if action == "open" {
            self.open_menu_at(at, false, gate);
            return Ok(());
        }
        let key = self.key_at(at);
        let Some(menu) = self.menu_at_mut(at) else {
            return Err(ActError::UnknownItem(key));
        };
        menu.act(action, entry, &mut |change| {
            let value = change
                .value
                .iter()
                .map(|entry| prefixed(&key, entry))
                .collect();
            gate(&ValueChange {
                value,
                source: change.source,
                reason: change.reason,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::Kind;

    fn bar() -> Menubar {
        Menubar::new(
            vec![
                (
                    "file",
                    "File",
                    vec![
                        Entry::action("new", "New"),
                        Entry::action("recent", "Recent").of(Kind::Submenu),
                        Entry::action("a.txt", "a.txt").inside("recent"),
                    ],
                ),
                (
                    "view",
                    "View",
                    vec![Entry::action("wrap", "Word wrap").of(Kind::Checkbox)],
                ),
            ],
            Dir::Ltr,
        )
    }

    fn key(bar: &mut Menubar, key: Key, offered: &mut Vec<String>) -> Reaction<Part> {
        bar.send(
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
    fn arrows_move_along_the_bar_and_between_open_menus() {
        let (mut bar, mut offered) = (bar(), Vec::new());
        assert_eq!(
            key(&mut bar, Key::ArrowDown, &mut offered).focus_item(),
            Some("file/new")
        );
        assert_eq!(
            key(&mut bar, Key::ArrowRight, &mut offered).focus_item(),
            Some("view/wrap"),
            "on to the next menu"
        );
        assert_eq!(bar.open_menu(), Some("view"));
        key(&mut bar, Key::Space, &mut offered);
        assert_eq!(bar.value(), ["view/wrap"]);
        let closed = key(&mut bar, Key::Escape, &mut offered);
        assert_eq!((bar.open_menu(), closed.focus_item()), (None, Some("view")));
        assert_eq!(
            key(&mut bar, Key::ArrowLeft, &mut offered).focus_item(),
            Some("file")
        );
    }

    #[test]
    fn a_sideways_arrow_inside_a_menu_still_opens_and_leaves_a_submenu() {
        let (mut bar, mut offered) = (bar(), Vec::new());
        key(&mut bar, Key::ArrowDown, &mut offered);
        key(&mut bar, Key::ArrowDown, &mut offered);
        assert_eq!(
            key(&mut bar, Key::ArrowRight, &mut offered).focus_item(),
            Some("file/a.txt")
        );
        assert_eq!(
            key(&mut bar, Key::ArrowLeft, &mut offered).focus_item(),
            Some("file/recent")
        );
        assert_eq!(bar.open_menu(), Some("file"));
        key(&mut bar, Key::ArrowUp, &mut offered);
        key(&mut bar, Key::Enter, &mut offered);
        assert_eq!(offered, ["run:file/new"]);
        assert_eq!(bar.open_menu(), None);
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(bar, &[]);
    }
}
