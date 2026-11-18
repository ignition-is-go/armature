use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::{Key, Modifiers};
use crate::primitive::{Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics};
use crate::roving::{self, Nav};
use crate::selection::{self, Mode};
use crate::transfer::{transfer, transfer_message};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The list items start in.
    Available,
    /// The list items are moved to.
    Chosen,
    /// One item, in whichever list holds it.
    Option,
    /// Moves the selected items to the chosen list.
    Add,
    /// Moves the selected items back.
    Remove,
}

const ACTIONS: &[&str] = &["add", "remove"];

/// Where each list is kept among the two.
const AVAILABLE: usize = 0;
const CHOSEN: usize = 1;

/// Two lists with buttons to move the selected items between them. The value is the
/// keys in the chosen list.
#[derive(Clone, Debug, Default)]
pub struct TransferList {
    /// The available keys, then the chosen ones.
    lists: [Vec<String>; 2],
    labels: Vec<(String, String)>,
    selected: Vec<String>,
    anchor: Option<String>,
    focused: Option<String>,
    on_button: Option<Part>,
}

impl TransferList {
    /// Starts with every key in the available list. The lists own their items, so
    /// what is rendered only supplies labels.
    #[must_use]
    pub fn new(available: &[&str]) -> Self {
        Self {
            lists: [
                available
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect(),
                Vec::new(),
            ],
            ..Default::default()
        }
    }

    /// One of the two lists, by its place: `AVAILABLE` or `CHOSEN`.
    fn side(&self, side: usize) -> &[String] {
        self.lists.get(side).map_or(&[], Vec::as_slice)
    }

    /// The keys in the available list (`false`) or the chosen one (`true`), in order.
    #[must_use]
    pub fn list(&self, chosen: bool) -> &[String] {
        self.side(if chosen { CHOSEN } else { AVAILABLE })
    }

    fn side_of(&self, key: &str) -> Option<usize> {
        self.lists
            .iter()
            .position(|list| list.iter().any(|k| k == key))
    }

    fn label(&self, key: &str) -> String {
        self.labels
            .iter()
            .find(|(k, _)| k == key)
            .map_or_else(|| key.to_string(), |(_, label)| label.clone())
    }

    /// Moves `keys` to one side if the owner allows, and says where focus should go:
    /// to the nearest item left behind, so the keyboard is not stranded.
    fn move_to(
        &mut self,
        to: usize,
        keys: &[String],
        source: Source,
        reason: Reason,
        gate: Gate,
    ) -> Option<Reaction<Part>> {
        let from = 1_usize.saturating_sub(to);
        let moving: Vec<String> = self
            .side(from)
            .iter()
            .filter(|key| keys.contains(key))
            .cloned()
            .collect();
        if moving.is_empty() {
            return Some(Reaction::handled());
        }
        let first = self
            .side(from)
            .iter()
            .position(|key| moving.contains(key))
            .unwrap_or(0);
        let mut lists = self.lists.clone();
        transfer(&mut lists, &moving, to, None);
        if !gate(&ValueChange {
            value: lists.get(CHOSEN).cloned().unwrap_or_default(),
            source,
            reason,
        }) {
            return None;
        }
        self.lists = lists;
        self.selected.retain(|key| !moving.contains(key));
        let stay = self.side(from);
        let landing = stay
            .get(first.min(stay.len().saturating_sub(1)))
            .or_else(|| self.side(to).last())
            .cloned();
        self.focused.clone_from(&landing);
        let destination = if to == 1 { "chosen" } else { "available" };
        Some(
            Reaction::item(Part::Option, true, landing)
                .announcing(transfer_message(moving.len(), destination)),
        )
    }

    fn press_button(
        &mut self,
        part: Part,
        source: Source,
        reason: Reason,
        gate: Gate,
    ) -> Reaction<Part> {
        let to = usize::from(part == Part::Add);
        self.move_to(to, &self.selected.clone(), source, reason, gate)
            .unwrap_or_else(Reaction::handled)
    }

    fn tab_stop(&self, side: usize) -> Option<&str> {
        let list = self.side(side);
        self.focused
            .as_deref()
            .filter(|key| list.iter().any(|k| k == key))
            .or_else(|| list.first().map(String::as_str))
    }
}

impl PartOf for Part {
    type Primitive = TransferList;
}

impl Primitive for TransferList {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[
            Part::Available,
            Part::Chosen,
            Part::Option,
            Part::Add,
            Part::Remove,
        ]
    }

    fn is_item(part: Part) -> bool {
        part == Part::Option
    }

    fn upsert_item(&mut self, item: Item) {
        self.labels.retain(|(key, _)| *key != item.key);
        self.labels.push((item.key, item.label));
    }

    fn remove_item(&mut self, _: &str) {}

    /// Each list owns its order once items start moving, so the rendered text only
    /// fills in labels.
    fn set_order(&mut self, rendered: &[(String, String)]) {
        for (key, text) in rendered {
            if let Some(entry) = self
                .labels
                .iter_mut()
                .find(|(k, label)| k == key && label.is_empty())
            {
                entry.1 = text.trim().to_string();
            }
        }
    }

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        let has_selection = |side: usize| {
            self.selected
                .iter()
                .any(|key| self.side_of(key) == Some(side))
        };
        match (part, item) {
            (Part::Available | Part::Chosen, _) => Semantics {
                role: Some(Role::Listbox),
                multiselectable: Some(true),
                ..Default::default()
            },
            (Part::Option, Some(key)) => {
                let side = self.side_of(key);
                Semantics {
                    role: Some(Role::Option),
                    selected: Some(self.selected.iter().any(|k| k == key)),
                    hidden: side.is_none(),
                    focusable: match side {
                        Some(side) if self.tab_stop(side) == Some(key) => Focusable::TabStop,
                        Some(_) => Focusable::Programmatic,
                        None => Focusable::No,
                    },
                    active: self.focused.as_deref() == Some(key),
                    ..Default::default()
                }
            }
            (Part::Option, None) => Semantics {
                role: Some(Role::Option),
                ..Default::default()
            },
            (button, _) => {
                let enabled = has_selection(usize::from(button != Part::Add));
                Semantics {
                    role: Some(Role::Button),
                    disabled: !enabled,
                    focusable: if enabled {
                        Focusable::TabStop
                    } else {
                        Focusable::No
                    },
                    ..Default::default()
                }
            }
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        if roving::is_platform_arrow(&input) {
            return Reaction::default();
        }
        match input {
            Input::Focus {
                part: Part::Option,
                item,
            } => {
                self.focused = item;
                self.on_button = None;
                Reaction::default()
            }
            Input::Focus {
                part: part @ (Part::Add | Part::Remove),
                ..
            } => {
                self.on_button = Some(part);
                Reaction::default()
            }
            Input::Press {
                part: part @ (Part::Add | Part::Remove),
                ..
            } => self.press_button(part, Source::User, Reason::Press, gate),
            Input::Press {
                part: Part::Option,
                item: Some(key),
                mods,
            } => {
                let Some(side) = self.side_of(&key) else {
                    return Reaction::default();
                };
                let order: Vec<&str> = self.side(side).iter().map(String::as_str).collect();
                // A range never reaches across into the other list.
                let anchor = self
                    .anchor
                    .as_deref()
                    .filter(|anchor| self.side_of(anchor) == Some(side));
                self.selected =
                    selection::propose(Mode::Multiple, &self.selected, anchor, &key, mods, &order);
                if !mods.shift || anchor.is_none() {
                    self.anchor = Some(key.clone());
                }
                self.focused = Some(key.clone());
                Reaction::item(Part::Option, true, Some(key))
            }
            Input::Key {
                key: Key::Enter | Key::Space,
                mods,
                ..
            } => match (self.on_button, self.focused.clone()) {
                (Some(part), _) => self.press_button(part, Source::User, Reason::Keyboard, gate),
                // Space toggles the focused item, as it does in any multiple list.
                (None, Some(key)) => {
                    let toggle = Modifiers { ctrl: true, ..mods };
                    self.send(
                        Input::Press {
                            part: Part::Option,
                            item: Some(key),
                            mods: toggle,
                        },
                        gate,
                    )
                }
                (None, None) => Reaction::default(),
            },
            Input::Key { key, .. } if self.on_button.is_none() => {
                let Some(focused) = self.focused.clone() else {
                    return Reaction::default();
                };
                let Some(side) = self.side_of(&focused) else {
                    return Reaction::default();
                };
                let nav = match key {
                    Key::ArrowDown => Nav::Next,
                    Key::ArrowUp => Nav::Prev,
                    Key::Home => Nav::First,
                    Key::End => Nav::Last,
                    _ => return Reaction::default(),
                };
                let list = self.side(side);
                let at = list.iter().position(|k| *k == focused);
                let target = roving::step(&vec![true; list.len()], at, nav, false)
                    .and_then(|index| list.get(index).cloned());
                if let Some(key) = &target {
                    self.focused = Some(key.clone());
                }
                Reaction::item(Part::Option, true, target)
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        self.side(CHOSEN).to_vec()
    }

    fn set_value(&mut self, value: Vec<String>) {
        let all: Vec<String> = self.lists.concat();
        let chosen: Vec<String> = value.into_iter().filter(|key| all.contains(key)).collect();
        let available = all
            .iter()
            .filter(|key| !chosen.contains(key))
            .cloned()
            .collect();
        self.lists = [available, chosen];
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .lists
            .concat()
            .into_iter()
            .map(|key| SnapshotItem {
                label: self.label(&key),
                disabled: false,
                selected: self.side(CHOSEN).contains(&key),
                key,
            })
            .collect();
        Snapshot {
            name: "transfer-list",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let (action, to): (&'static str, usize) = match action {
            "add" => ("add", 1),
            "remove" => ("remove", 0),
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        let key = item.ok_or(ActError::MissingItem { action })?;
        if self.side_of(key).is_none() {
            return Err(ActError::UnknownItem(key.to_string()));
        }
        match self.move_to(to, &[key.to_string()], Source::Agent, Reason::Action, gate) {
            Some(_) => Ok(()),
            None => Err(ActError::Refused),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitive::Effect;

    fn lists() -> TransferList {
        TransferList::new(&["read", "write", "share", "delete"])
    }

    fn press(
        lists: &mut TransferList,
        part: Part,
        item: Option<&str>,
        mods: Modifiers,
    ) -> Reaction<Part> {
        lists.send(
            Input::Press {
                part,
                item: item.map(str::to_string),
                mods,
            },
            &mut |_| true,
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
    fn the_selected_items_move_across_and_focus_stays_with_what_is_left() {
        let mut lists = lists();
        press(&mut lists, Part::Option, Some("write"), NONE);
        press(&mut lists, Part::Option, Some("share"), SHIFT);
        let moved = press(&mut lists, Part::Add, None, NONE);
        assert_eq!(lists.value(), ["write", "share"]);
        assert_eq!(lists.list(false), ["read", "delete"]);
        assert_eq!(
            moved.focus_item(),
            Some("delete"),
            "the item that took the moved ones' place"
        );
        assert_eq!(
            moved.effects,
            [Effect::Announce {
                message: "2 items moved to chosen.".into(),
                assertive: false
            }]
        );
    }

    #[test]
    fn emptying_a_list_sends_focus_to_the_other_one() {
        let mut lists = lists();
        press(&mut lists, Part::Option, Some("read"), NONE);
        press(&mut lists, Part::Option, Some("delete"), SHIFT);
        let moved = press(&mut lists, Part::Add, None, NONE);
        assert_eq!(lists.list(false).len(), 0);
        assert_eq!(moved.focus_item(), Some("delete"));
    }

    #[test]
    fn a_button_is_disabled_until_its_side_has_a_selection() {
        let mut lists = lists();
        assert!(lists.semantics(Part::Add, None).disabled);
        press(&mut lists, Part::Option, Some("read"), NONE);
        assert!(!lists.semantics(Part::Add, None).disabled);
        assert!(lists.semantics(Part::Remove, None).disabled);
    }

    #[test]
    fn arrows_stay_within_the_focused_items_list() {
        let mut lists = lists();
        lists.act("add", Some("read"), &mut |_| true).unwrap();
        lists.send(
            Input::Focus {
                part: Part::Option,
                item: Some("delete".into()),
            },
            &mut |_| true,
        );
        let down = lists.send(
            Input::Key {
                key: Key::ArrowDown,
                mods: NONE,
                now_ms: 0,
            },
            &mut |_| true,
        );
        assert_eq!(
            down.focus_item(),
            None,
            "delete is last in its list, and read is in the other"
        );
    }

    #[test]
    fn the_owner_can_refuse_a_move() {
        let mut lists = lists();
        press(&mut lists, Part::Option, Some("delete"), NONE);
        lists.send(
            Input::Press {
                part: Part::Add,
                item: None,
                mods: NONE,
            },
            &mut |_| false,
        );
        assert_eq!(lists.value(), Vec::<String>::new());
        assert_eq!(
            lists.act("add", Some("delete"), &mut |_| false),
            Err(ActError::Refused)
        );
    }

    #[test]
    fn it_conforms() {
        let items: Vec<Item> = ["read", "write", "share"]
            .iter()
            .map(|key| Item {
                key: key.to_string(),
                label: key.to_string(),
                disabled: false,
            })
            .collect();
        crate::testing::assert_conforms(|| TransferList::new(&["read", "write", "share"]), &items);
    }
}
