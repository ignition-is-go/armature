use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::items::Items;
use crate::keys::{Dir, Key, Orientation};
use crate::primitive::{Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics};
use crate::roving;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    /// One button that stays down while it is on, rendered per item.
    Toggle,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Whether several toggles can be down at once.
    pub multiple: bool,
    /// Whether pressing the one toggle that is down lets it up, leaving none.
    pub deselectable: bool,
    pub orientation: Orientation,
    pub dir: Dir,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            multiple: false,
            deselectable: true,
            orientation: Orientation::Horizontal,
            dir: Dir::Ltr,
        }
    }
}

const ACTIONS: &[&str] = &["press", "release", "toggle"];

/// Buttons that stay down, such as bold and italic, or a choice of alignment. One
/// toggle on its own is a group of one. The value is the keys of the toggles that
/// are down, in the order they appear.
#[derive(Clone, Debug, Default)]
pub struct ToggleGroup {
    opts: Options,
    items: Items,
    pressed: Vec<String>,
    focused: Option<String>,
}

impl ToggleGroup {
    #[must_use]
    pub fn new(opts: Options) -> Self {
        Self {
            opts,
            ..Default::default()
        }
    }

    #[must_use]
    pub fn is_pressed(&self, key: &str) -> bool {
        self.pressed.iter().any(|p| p == key)
    }

    fn tab_stop(&self) -> Option<&str> {
        let focused = self
            .focused
            .as_deref()
            .filter(|key| self.items.is_enabled(key));
        let down = self
            .pressed
            .iter()
            .map(String::as_str)
            .find(|key| self.items.is_enabled(key));
        focused.or(down).or(self.items.first_enabled())
    }

    fn toggle(&mut self, key: &str, source: Source, reason: Reason, gate: Gate) -> bool {
        if !self.items.is_enabled(key) {
            return false;
        }
        let down = self.is_pressed(key);
        let mut next = match (self.opts.multiple, down) {
            (true, true) => self.pressed.iter().filter(|p| *p != key).cloned().collect(),
            (true, false) => self
                .pressed
                .iter()
                .cloned()
                .chain([key.to_string()])
                .collect(),
            (false, true) if self.opts.deselectable => Vec::new(),
            (false, true) => return false,
            (false, false) => vec![key.to_string()],
        };
        next.sort_by_key(|key: &String| self.items.index_of(key));
        let allowed = gate(&ValueChange {
            value: next.clone(),
            source,
            reason,
        });
        if allowed {
            self.pressed = next;
        }
        allowed
    }
}

impl PartOf for Part {
    type Primitive = ToggleGroup;
}

impl Primitive for ToggleGroup {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Toggle]
    }

    fn is_item(part: Part) -> bool {
        part == Part::Toggle
    }

    fn upsert_item(&mut self, item: Item) {
        self.items.upsert(item);
    }

    fn remove_item(&mut self, key: &str) {
        self.items.remove(key);
        self.pressed.retain(|p| p != key);
    }

    fn set_order(&mut self, rendered: &[(String, String)]) {
        self.items.set_order(rendered);
    }

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match (part, item) {
            (Part::Root, _) => Semantics {
                role: Some(Role::Group),
                orientation: Some(self.opts.orientation),
                ..Default::default()
            },
            (Part::Toggle, Some(key)) => Semantics {
                role: Some(Role::Button),
                pressed: Some(self.is_pressed(key)),
                disabled: !self.items.is_enabled(key),
                focusable: if self.tab_stop() == Some(key) {
                    Focusable::TabStop
                } else {
                    Focusable::Programmatic
                },
                active: self.is_pressed(key),
                ..Default::default()
            },
            (Part::Toggle, None) => Semantics {
                role: Some(Role::Button),
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        if roving::is_platform_arrow(&input) {
            return Reaction::default();
        }
        match input {
            Input::Focus { item, .. } => {
                self.focused = item;
                Reaction::default()
            }
            Input::Press {
                part: Part::Toggle,
                item: Some(key),
                ..
            } => {
                self.toggle(&key, Source::User, Reason::Press, gate);
                self.focused = Some(key.clone());
                Reaction::item(Part::Toggle, true, Some(key))
            }
            Input::Key {
                key: Key::Enter | Key::Space,
                ..
            } => match self.focused.clone().or(self.tab_stop().map(str::to_string)) {
                Some(key) => {
                    self.toggle(&key, Source::User, Reason::Keyboard, gate);
                    Reaction::handled()
                }
                None => Reaction::default(),
            },
            // Arrows only move. Unlike a radio group, moving presses nothing.
            Input::Key { key, .. } => {
                let Some(nav) = roving::nav_for_key(key, self.opts.orientation, self.opts.dir)
                else {
                    return Reaction::default();
                };
                let target = self.items.step(self.tab_stop(), nav, true);
                if let Some(key) = &target {
                    self.focused = Some(key.clone());
                }
                Reaction::item(Part::Toggle, true, target)
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        self.pressed.clone()
    }

    fn set_value(&mut self, value: Vec<String>) {
        let mut next: Vec<String> = value
            .into_iter()
            .filter(|key| self.items.get(key).is_some())
            .collect();
        next.sort_by_key(|key| self.items.index_of(key));
        if !self.opts.multiple {
            next.truncate(1);
        }
        self.pressed = next;
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .items
            .iter()
            .map(|item| SnapshotItem {
                key: item.key.clone(),
                label: item.label.clone(),
                disabled: item.disabled,
                selected: self.is_pressed(&item.key),
            })
            .collect();
        Snapshot {
            name: "toggle-group",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let action: &'static str = match action {
            "press" => "press",
            "release" => "release",
            "toggle" => "toggle",
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        let key = item.ok_or(ActError::MissingItem { action })?;
        match self.items.get(key).map(|item| item.disabled) {
            None => return Err(ActError::UnknownItem(key.to_string())),
            Some(true) => return Err(ActError::Disabled(key.to_string())),
            Some(false) => {}
        }
        let down = self.is_pressed(key);
        if (action == "press" && down) || (action == "release" && !down) {
            return Ok(());
        }
        if self.toggle(key, Source::Agent, Reason::Action, gate) {
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

    fn group(opts: Options) -> ToggleGroup {
        let mut group = ToggleGroup::new(opts);
        for key in ["left", "center", "right"] {
            group.upsert_item(Item {
                key: key.into(),
                label: key.into(),
                disabled: false,
            });
        }
        group
    }

    fn press(group: &mut ToggleGroup, key: &str) {
        group.send(
            Input::Press {
                part: Part::Toggle,
                item: Some(key.into()),
                mods: Modifiers::default(),
            },
            &mut |_| true,
        );
    }

    #[test]
    fn one_toggle_is_down_at_a_time_unless_several_are_allowed() {
        let mut single = group(Options::default());
        press(&mut single, "left");
        press(&mut single, "right");
        assert_eq!(single.value(), ["right"]);
        assert_eq!(
            single.semantics(Part::Toggle, Some("right")).pressed,
            Some(true)
        );
        let mut several = group(Options {
            multiple: true,
            ..Default::default()
        });
        press(&mut several, "right");
        press(&mut several, "left");
        assert_eq!(
            several.value(),
            ["left", "right"],
            "in the order they appear"
        );
    }

    #[test]
    fn the_last_toggle_stays_down_when_it_may_not_be_let_up() {
        let mut group = group(Options {
            deselectable: false,
            ..Default::default()
        });
        press(&mut group, "center");
        press(&mut group, "center");
        assert_eq!(group.value(), ["center"]);
    }

    #[test]
    fn arrows_move_without_pressing() {
        let mut group = group(Options::default());
        let moved = group.send(
            Input::Key {
                key: Key::ArrowRight,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        );
        assert_eq!(moved.focus_item(), Some("center"));
        assert!(group.value().is_empty());
    }

    #[test]
    fn it_conforms() {
        let items: Vec<Item> = ["a", "b", "c"]
            .iter()
            .map(|k| Item {
                key: k.to_string(),
                label: k.to_string(),
                disabled: *k == "b",
            })
            .collect();
        crate::testing::assert_conforms(ToggleGroup::default, &items);
        crate::testing::assert_conforms(
            || {
                ToggleGroup::new(Options {
                    multiple: true,
                    ..Default::default()
                })
            },
            &items,
        );
    }
}
