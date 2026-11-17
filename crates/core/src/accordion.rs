use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::items::Items;
use crate::keys::{Dir, Key, Orientation};
use crate::primitive::{
    Focusable, Input, Item, PartOf, Primitive, Reaction, Relations, Role, Semantics,
};
use crate::roving;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The header button of a section.
    Trigger,
    /// The content of a section. Takes the section's key as its item.
    Panel,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// Whether several sections may be open at once.
    pub multiple: bool,
    /// Whether the last open section may be closed.
    pub collapsible: bool,
    /// Whether find-in-page searches closed sections and opens the one it matches.
    pub findable: bool,
}

const ACTIONS: &[&str] = &["open", "close", "toggle"];

/// Sections that expand and collapse. The value is the keys of the open sections.
#[derive(Clone, Debug, Default)]
pub struct Accordion {
    opts: Options,
    items: Items,
    open: Vec<String>,
    focused: Option<String>,
}

impl Accordion {
    pub fn new(opts: Options) -> Self {
        Accordion {
            opts,
            ..Default::default()
        }
    }

    fn is_open(&self, key: &str) -> bool {
        self.open.iter().any(|k| k == key)
    }

    fn toggled(&self, key: &str, open: bool) -> Vec<String> {
        match (open, self.opts.multiple) {
            (true, true) => {
                let wanted = |item: &&Item| item.key == key || self.is_open(&item.key);
                self.items
                    .iter()
                    .filter(wanted)
                    .map(|item| item.key.clone())
                    .collect()
            }
            (true, false) => vec![key.to_string()],
            (false, _) if self.open.len() == 1 && !self.opts.collapsible => self.open.clone(),
            (false, _) => self.open.iter().filter(|k| *k != key).cloned().collect(),
        }
    }

    fn set(&mut self, key: &str, open: bool, source: Source, reason: Reason, gate: Gate) -> bool {
        let next = self.toggled(key, open);
        if next == self.open {
            return true;
        }
        let allowed = gate(&ValueChange {
            value: next.clone(),
            source,
            reason,
        });
        if allowed {
            self.open = next;
        }
        allowed
    }
}

impl PartOf for Part {
    type Primitive = Accordion;
}

impl Primitive for Accordion {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Trigger, Part::Panel]
    }

    fn is_item(part: Part) -> bool {
        part == Part::Trigger
    }

    fn relations(part: Part) -> Relations<Part> {
        match part {
            Part::Trigger => Relations {
                controls: Some(Part::Panel),
                ..Default::default()
            },
            Part::Panel => Relations {
                labelled_by: Some(Part::Trigger),
                ..Default::default()
            },
        }
    }

    fn upsert_item(&mut self, item: Item) {
        self.items.upsert(item);
    }

    fn remove_item(&mut self, key: &str) {
        self.items.remove(key);
        self.open.retain(|k| k != key);
    }

    fn set_order(&mut self, rendered: &[(String, String)]) {
        self.items.set_order(rendered);
    }

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        let open = item.is_some_and(|key| self.is_open(key));
        match part {
            Part::Trigger => {
                let enabled = item.is_some_and(|key| self.items.is_enabled(key));
                Semantics {
                    role: Some(Role::Button),
                    expanded: Some(open),
                    disabled: !enabled,
                    focusable: if enabled {
                        Focusable::TabStop
                    } else {
                        Focusable::No
                    },
                    ..Default::default()
                }
            }
            Part::Panel => Semantics {
                role: Some(Role::Region),
                hidden: !open,
                findable: self.opts.findable,
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        if crate::roving::is_platform_arrow(&input) {
            return Reaction::default();
        }
        match input {
            Input::Press {
                part: Part::Trigger,
                item: Some(key),
                ..
            } if self.items.is_enabled(&key) => {
                self.set(&key, !self.is_open(&key), Source::User, Reason::Press, gate);
                Reaction::item(Part::Trigger, true, Some(key))
            }
            Input::Found {
                part: Part::Panel,
                item: Some(key),
            } if self.opts.findable => {
                self.set(&key, true, Source::User, Reason::Keyboard, gate);
                Reaction::handled()
            }
            Input::Focus { item, .. } => {
                self.focused = item;
                Reaction::default()
            }
            // A header is a button, so Enter and Space press it.
            Input::Key {
                key: Key::Enter | Key::Space,
                ..
            } => match self.focused.clone() {
                Some(key) if self.items.is_enabled(&key) => {
                    self.set(
                        &key,
                        !self.is_open(&key),
                        Source::User,
                        Reason::Keyboard,
                        gate,
                    );
                    Reaction::handled()
                }
                _ => Reaction::default(),
            },
            // Arrows travel between headers.
            Input::Key { key, .. } => {
                let nav = roving::nav_for_key(key, Orientation::Vertical, Dir::Ltr);
                let target =
                    nav.and_then(|nav| self.items.step(self.focused.as_deref(), nav, true));
                if let Some(key) = &target {
                    self.focused = Some(key.clone());
                }
                Reaction::item(Part::Trigger, target.is_some(), target)
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        self.open.clone()
    }

    fn set_value(&mut self, value: Vec<String>) {
        let limit = if self.opts.multiple { usize::MAX } else { 1 };
        self.open = self
            .items
            .iter()
            .filter(|item| value.contains(&item.key))
            .map(|item| item.key.clone())
            .take(limit)
            .collect();
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .items
            .iter()
            .map(|item| SnapshotItem {
                key: item.key.clone(),
                label: item.label.clone(),
                disabled: item.disabled,
                selected: self.is_open(&item.key),
            })
            .collect();
        Snapshot {
            name: "accordion",
            label: label.to_string(),
            value: self.open.clone(),
            expanded: None,
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let (action, open): (&'static str, Option<bool>) = match action {
            "open" => ("open", Some(true)),
            "close" => ("close", Some(false)),
            "toggle" => ("toggle", None),
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        let key = item.ok_or(ActError::MissingItem { action })?;
        let found = self
            .items
            .get(key)
            .ok_or_else(|| ActError::UnknownItem(key.to_string()))?;
        if found.disabled {
            return Err(ActError::Disabled(key.to_string()));
        }
        let open = open.unwrap_or(!self.is_open(key));
        if self.set(key, open, Source::Agent, Reason::Action, gate) {
            Ok(())
        } else {
            Err(ActError::Refused)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{Key, Modifiers};

    fn accordion(opts: Options) -> Accordion {
        let mut accordion = Accordion::new(opts);
        for (key, disabled) in [("shipping", false), ("returns", false), ("legal", true)] {
            accordion.upsert_item(Item {
                key: key.into(),
                label: key.into(),
                disabled,
            });
        }
        accordion
    }

    fn press(accordion: &mut Accordion, key: &str) {
        let input = Input::Press {
            part: Part::Trigger,
            item: Some(key.into()),
            mods: Modifiers::default(),
        };
        accordion.send(input, &mut |_| true);
    }

    #[test]
    fn opening_one_section_closes_the_other_by_default() {
        let mut accordion = accordion(Options::default());
        press(&mut accordion, "shipping");
        press(&mut accordion, "returns");
        assert_eq!(accordion.value(), ["returns"]);
        assert!(accordion.semantics(Part::Panel, Some("shipping")).hidden);
        assert_eq!(
            accordion.semantics(Part::Trigger, Some("returns")).expanded,
            Some(true)
        );
    }

    #[test]
    fn several_sections_can_be_open_when_allowed_and_keep_document_order() {
        let mut accordion = accordion(Options {
            multiple: true,
            collapsible: true,
            findable: false,
        });
        press(&mut accordion, "returns");
        press(&mut accordion, "shipping");
        assert_eq!(accordion.value(), ["shipping", "returns"]);
        press(&mut accordion, "shipping");
        assert_eq!(accordion.value(), ["returns"]);
    }

    #[test]
    fn the_last_open_section_stays_open_unless_collapsible() {
        let mut fixed = accordion(Options::default());
        press(&mut fixed, "shipping");
        press(&mut fixed, "shipping");
        assert_eq!(fixed.value(), ["shipping"]);
        let mut loose = accordion(Options {
            collapsible: true,
            ..Default::default()
        });
        press(&mut loose, "shipping");
        press(&mut loose, "shipping");
        assert!(loose.value().is_empty());
    }

    #[test]
    fn disabled_sections_ignore_presses_and_agents() {
        let mut accordion = accordion(Options::default());
        press(&mut accordion, "legal");
        assert!(accordion.value().is_empty());
        assert_eq!(
            accordion.act("open", Some("legal"), &mut |_| true),
            Err(ActError::Disabled("legal".into()))
        );
        accordion
            .act("toggle", Some("returns"), &mut |_| true)
            .unwrap();
        assert_eq!(accordion.value(), ["returns"]);
    }

    #[test]
    fn a_find_in_page_match_inside_a_closed_section_opens_it() {
        let mut accordion = accordion(Options {
            findable: true,
            ..Default::default()
        });
        assert!(accordion.semantics(Part::Panel, Some("returns")).findable);
        accordion.send(
            Input::Found {
                part: Part::Panel,
                item: Some("returns".into()),
            },
            &mut |_| true,
        );
        assert_eq!(accordion.value(), ["returns"]);
    }

    #[test]
    fn arrows_travel_between_headers_past_disabled_ones() {
        let mut accordion = accordion(Options::default());
        accordion.send(
            Input::Focus {
                part: Part::Trigger,
                item: Some("returns".into()),
            },
            &mut |_| true,
        );
        let key = |accordion: &mut Accordion, key| {
            accordion.send(
                Input::Key {
                    key,
                    mods: Modifiers::default(),
                    now_ms: 0,
                },
                &mut |_| true,
            )
        };
        assert_eq!(
            key(&mut accordion, Key::ArrowDown).focus_item(),
            Some("shipping")
        );
        assert!(key(&mut accordion, Key::Enter).handled);
        assert_eq!(
            accordion.value(),
            ["shipping"],
            "Enter presses the focused header"
        );
    }
}
