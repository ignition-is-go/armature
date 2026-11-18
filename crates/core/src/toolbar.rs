use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::items::Items;
use crate::keys::{Dir, Key, Orientation};
use crate::primitive::{Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics};
use crate::roving;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    Button,
}

const ACTIONS: &[&str] = &["press"];

/// A row of buttons that is one stop in the Tab order, with arrows moving between
/// them. Pressing a button offers its key to the owner; nothing is kept as a value.
#[derive(Clone, Debug)]
pub struct Toolbar {
    orientation: Orientation,
    dir: Dir,
    items: Items,
    focused: Option<String>,
}

impl Default for Toolbar {
    /// A horizontal toolbar, as most are.
    fn default() -> Self {
        Self::new(Orientation::Horizontal, Dir::Ltr)
    }
}

impl Toolbar {
    #[must_use]
    pub fn new(orientation: Orientation, dir: Dir) -> Self {
        Self {
            orientation,
            dir,
            items: Items::default(),
            focused: None,
        }
    }

    fn tab_stop(&self) -> Option<&str> {
        self.focused
            .as_deref()
            .filter(|key| self.items.is_enabled(key))
            .or_else(|| self.items.first_enabled())
    }

    /// Tells the owner a button was pressed. The owner's answer is whether it acted.
    fn press(key: &str, source: Source, reason: Reason, gate: Gate) -> bool {
        gate(&ValueChange {
            value: vec![key.to_string()],
            source,
            reason,
        })
    }
}

impl PartOf for Part {
    type Primitive = Toolbar;
}

impl Primitive for Toolbar {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Button]
    }

    fn is_item(part: Part) -> bool {
        part == Part::Button
    }

    fn upsert_item(&mut self, item: Item) {
        self.items.upsert(item);
    }

    fn remove_item(&mut self, key: &str) {
        let neighbour = self.items.remove(key);
        if self.focused.as_deref() == Some(key) {
            self.focused = neighbour;
        }
    }

    fn set_order(&mut self, rendered: &[(String, String)]) {
        self.items.set_order(rendered);
    }

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match (part, item) {
            (Part::Button, Some(key)) => Semantics {
                role: Some(Role::Button),
                disabled: !self.items.is_enabled(key),
                focusable: if self.tab_stop() == Some(key) {
                    Focusable::TabStop
                } else {
                    Focusable::Programmatic
                },
                active: self.focused.as_deref() == Some(key),
                ..Default::default()
            },
            (Part::Button, None) => Semantics {
                role: Some(Role::Button),
                ..Default::default()
            },
            (Part::Root, _) => Semantics {
                role: Some(Role::Toolbar),
                orientation: Some(self.orientation),
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        if crate::roving::is_platform_arrow(&input) {
            return Reaction::default();
        }
        match input {
            Input::Focus {
                item: Some(key), ..
            } if self.items.is_enabled(&key) => {
                self.focused = Some(key);
                Reaction::default()
            }
            Input::Press {
                item: Some(key), ..
            } if self.items.is_enabled(&key) => {
                self.focused = Some(key.clone());
                Self::press(&key, Source::User, Reason::Press, gate);
                Reaction::to_item(Part::Button, Some(key))
            }
            Input::Key {
                key: Key::Enter | Key::Space,
                ..
            } => {
                let Some(key) = self.tab_stop().map(str::to_string) else {
                    return Reaction::default();
                };
                Self::press(&key, Source::User, Reason::Keyboard, gate);
                Reaction::handled()
            }
            Input::Key { key, .. } => {
                let Some(nav) = roving::nav_for_key(key, self.orientation, self.dir) else {
                    return Reaction::default();
                };
                let from = self.tab_stop().map(str::to_string);
                let target = self.items.step(from.as_deref(), nav, true);
                if let Some(key) = &target {
                    self.focused = Some(key.clone());
                }
                Reaction::to_item(Part::Button, target)
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        Vec::new()
    }

    fn set_value(&mut self, _: Vec<String>) {}

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
            name: "toolbar",
            label: label.to_string(),
            value: Vec::new(),
            expanded: None,
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        if action != "press" {
            return Err(ActError::UnknownAction {
                action: action.to_string(),
                valid: ACTIONS,
            });
        }
        let key = item.ok_or(ActError::MissingItem { action: "press" })?;
        let found = self
            .items
            .get(key)
            .ok_or_else(|| ActError::UnknownItem(key.to_string()))?;
        if found.disabled {
            return Err(ActError::Disabled(key.to_string()));
        }
        if Self::press(key, Source::Agent, Reason::Action, gate) {
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

    fn toolbar() -> Toolbar {
        let mut toolbar = Toolbar::new(Orientation::Horizontal, Dir::Ltr);
        for (key, disabled) in [("bold", false), ("italic", true), ("link", false)] {
            toolbar.upsert_item(Item {
                key: key.into(),
                label: key.into(),
                disabled,
            });
        }
        toolbar
    }

    fn key(toolbar: &mut Toolbar, key: Key, pressed: &mut Vec<String>) -> Reaction<Part> {
        toolbar.send(
            Input::Key {
                key,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |change| {
                pressed.extend(change.value.iter().cloned());
                true
            },
        )
    }

    #[test]
    fn arrows_move_between_buttons_and_only_one_is_a_tab_stop() {
        let mut toolbar = toolbar();
        let mut pressed = Vec::new();
        assert_eq!(
            key(&mut toolbar, Key::ArrowRight, &mut pressed).focus_item(),
            Some("link")
        );
        assert_eq!(
            key(&mut toolbar, Key::ArrowRight, &mut pressed).focus_item(),
            Some("bold")
        );
        assert!(pressed.is_empty(), "moving presses nothing");
        assert_eq!(
            toolbar.semantics(Part::Button, Some("bold")).focusable,
            Focusable::TabStop
        );
        assert_eq!(
            toolbar.semantics(Part::Button, Some("link")).focusable,
            Focusable::Programmatic
        );
    }

    #[test]
    fn enter_presses_the_focused_button_and_tells_the_owner_which() {
        let mut toolbar = toolbar();
        let mut pressed = Vec::new();
        key(&mut toolbar, Key::End, &mut pressed);
        key(&mut toolbar, Key::Enter, &mut pressed);
        assert_eq!(pressed, ["link"]);
        assert!(toolbar.value().is_empty(), "a toolbar keeps no value");
    }

    #[test]
    fn disabled_buttons_cannot_be_pressed_by_anyone() {
        let mut toolbar = toolbar();
        let press = Input::Press {
            part: Part::Button,
            item: Some("italic".into()),
            mods: Modifiers::default(),
        };
        let mut asked = false;
        toolbar.send(press, &mut |_| {
            asked = true;
            true
        });
        assert!(!asked);
        assert_eq!(
            toolbar.act("press", Some("italic"), &mut |_| true),
            Err(ActError::Disabled("italic".into()))
        );
    }

    #[test]
    fn it_conforms() {
        let items: Vec<Item> = ["bold", "italic", "link"]
            .iter()
            .map(|key| Item {
                key: key.to_string(),
                label: key.to_string(),
                disabled: *key == "italic",
            })
            .collect();
        crate::testing::assert_conforms(|| Toolbar::new(Orientation::Horizontal, Dir::Ltr), &items);
    }
}
