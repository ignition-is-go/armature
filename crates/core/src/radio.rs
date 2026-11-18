use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::items::Items;
use crate::keys::{Dir, Key};
use crate::primitive::{Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics};
use crate::roving::Nav;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    Option,
}

const ACTIONS: &[&str] = &["select"];

/// One choice from a few, always visible: a radio group, a segmented control, or a
/// star rating. Arrows move and choose in one step.
#[derive(Clone, Debug, Default)]
pub struct Radio {
    dir: Dir,
    items: Items,
    selected: Option<String>,
    /// Whether choosing the chosen option again clears it, as a rating allows.
    deselectable: bool,
}

impl Radio {
    #[must_use]
    pub fn new(dir: Dir) -> Self {
        Self {
            dir,
            ..Default::default()
        }
    }

    /// Whether an option is at or before the chosen one, which is how a rating fills
    /// its stars.
    #[must_use]
    pub fn is_filled(&self, key: &str) -> bool {
        match (
            self.items.index_of(key),
            self.selected
                .as_deref()
                .and_then(|s| self.items.index_of(s)),
        ) {
            (Some(at), Some(chosen)) => at <= chosen,
            _ => false,
        }
    }

    #[must_use]
    pub const fn deselectable(mut self) -> Self {
        self.deselectable = true;
        self
    }

    fn select(&mut self, key: &str, source: Source, reason: Reason, gate: Gate) -> bool {
        if self.selected.as_deref() == Some(key) {
            // Only a press clears it. The arrows pass over the chosen option freely.
            if self.deselectable && reason != Reason::Keyboard {
                let allowed = gate(&ValueChange {
                    value: Vec::new(),
                    source,
                    reason,
                });
                if allowed {
                    self.selected = None;
                }
                return allowed;
            }
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

    fn tab_stop(&self) -> Option<&str> {
        self.selected
            .as_deref()
            .filter(|key| self.items.is_enabled(key))
            .or_else(|| self.items.first_enabled())
    }
}

impl PartOf for Part {
    type Primitive = Radio;
}

impl Primitive for Radio {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Option]
    }

    fn is_item(part: Part) -> bool {
        part == Part::Option
    }

    fn upsert_item(&mut self, item: Item) {
        self.items.upsert(item);
    }

    fn remove_item(&mut self, key: &str) {
        self.items.remove(key);
        if self.selected.as_deref() == Some(key) {
            self.selected = None;
        }
    }

    fn set_order(&mut self, rendered: &[(String, String)]) {
        self.items.set_order(rendered);
    }

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match (part, item) {
            (Part::Option, Some(key)) => Semantics {
                role: Some(Role::Radio),
                checked: Some(self.selected.as_deref() == Some(key)),
                disabled: !self.items.is_enabled(key),
                focusable: if self.tab_stop() == Some(key) {
                    Focusable::TabStop
                } else {
                    Focusable::Programmatic
                },
                active: self.is_filled(key),
                ..Default::default()
            },
            (Part::Option, None) => Semantics {
                role: Some(Role::Radio),
                ..Default::default()
            },
            (Part::Root, _) => Semantics {
                role: Some(Role::RadioGroup),
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        if crate::roving::is_platform_arrow(&input) {
            return Reaction::default();
        }
        match input {
            Input::Key { key, .. } => {
                let forward = if self.dir == Dir::Rtl {
                    Nav::Prev
                } else {
                    Nav::Next
                };
                let back = if self.dir == Dir::Rtl {
                    Nav::Next
                } else {
                    Nav::Prev
                };
                let nav = match key {
                    Key::ArrowDown => Nav::Next,
                    Key::ArrowUp => Nav::Prev,
                    Key::ArrowRight => forward,
                    Key::ArrowLeft => back,
                    Key::Space => {
                        if let Some(key) = self.tab_stop().map(str::to_string) {
                            self.select(&key, Source::User, Reason::Keyboard, gate);
                        }
                        return Reaction::handled();
                    }
                    _ => return Reaction::default(),
                };
                let from = self.tab_stop().map(str::to_string);
                let target = self.items.step(from.as_deref(), nav, true);
                if let Some(key) = &target {
                    self.select(key, Source::User, Reason::Keyboard, gate);
                }
                Reaction::item(Part::Option, true, target)
            }
            Input::Press {
                item: Some(key), ..
            } if self.items.is_enabled(&key) => {
                self.select(&key, Source::User, Reason::Press, gate);
                Reaction::item(Part::Option, true, Some(key))
            }
            _ => Reaction::default(),
        }
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
            .map(|item| SnapshotItem {
                key: item.key.clone(),
                label: item.label.clone(),
                disabled: item.disabled,
                selected: self.selected.as_deref() == Some(item.key.as_str()),
            })
            .collect();
        Snapshot {
            name: "radio",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items,
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

    fn radio() -> Radio {
        let mut radio = Radio::default();
        for (key, disabled) in [("1", false), ("2", false), ("3", true), ("4", false)] {
            radio.upsert_item(Item {
                key: key.into(),
                label: format!("{key} stars"),
                disabled,
            });
        }
        radio
    }

    fn key(radio: &mut Radio, key: Key) -> Reaction<Part> {
        radio.send(
            Input::Key {
                key,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        )
    }

    #[test]
    fn arrows_move_and_choose_in_one_step_and_wrap() {
        let mut radio = radio();
        assert_eq!(key(&mut radio, Key::ArrowRight).focus_item(), Some("2"));
        assert_eq!(radio.value(), ["2"]);
        key(&mut radio, Key::ArrowDown);
        assert_eq!(radio.value(), ["4"], "skipped the disabled option");
        key(&mut radio, Key::ArrowDown);
        assert_eq!(radio.value(), ["1"]);
    }

    #[test]
    fn nothing_is_chosen_until_the_person_chooses() {
        let mut radio = radio();
        assert!(radio.value().is_empty());
        assert_eq!(
            radio.semantics(Part::Option, Some("1")).focusable,
            Focusable::TabStop
        );
        key(&mut radio, Key::Space);
        assert_eq!(radio.value(), ["1"]);
    }

    #[test]
    fn a_rating_fills_every_option_up_to_the_chosen_one() {
        let mut radio = radio();
        radio.set_value(vec!["2".into()]);
        assert!(radio.is_filled("1") && radio.is_filled("2"));
        assert!(!radio.is_filled("4"));
        assert_eq!(radio.semantics(Part::Option, Some("2")).checked, Some(true));
        assert_eq!(
            radio.semantics(Part::Option, Some("1")).checked,
            Some(false)
        );
    }

    #[test]
    fn horizontal_arrows_flip_in_rtl() {
        let mut radio = Radio::new(Dir::Rtl);
        for key in ["a", "b"] {
            radio.upsert_item(Item {
                key: key.into(),
                label: key.into(),
                disabled: false,
            });
        }
        radio.set_value(vec!["a".into()]);
        key(&mut radio, Key::ArrowLeft);
        assert_eq!(radio.value(), ["b"]);
    }

    #[test]
    fn a_rating_can_be_cleared_by_pressing_the_chosen_star_again() {
        let mut rating = Radio::new(Dir::Ltr).deselectable();
        for key in ["1", "2", "3"] {
            rating.upsert_item(Item {
                key: key.into(),
                label: key.into(),
                disabled: false,
            });
        }
        let press = |rating: &mut Radio| {
            rating.send(
                Input::Press {
                    part: Part::Option,
                    item: Some("2".into()),
                    mods: Modifiers::default(),
                },
                &mut |_| true,
            );
        };
        press(&mut rating);
        assert_eq!(rating.value(), ["2"]);
        press(&mut rating);
        assert!(rating.value().is_empty());
    }
}
