use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::Key;
use crate::primitive::{
    Focusable, Input, Item, PartOf, Primitive, Reaction, Relations, Role, Semantics,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Trigger,
    Panel,
}

const ACTIONS: &[&str] = &["open", "close", "toggle"];
const OPEN: &str = "open";

/// One section that opens and closes. The value is `["open"]` or empty.
#[derive(Clone, Debug, Default)]
pub struct Collapsible {
    open: bool,
    disabled: bool,
    /// Whether find-in-page searches the closed panel and opens it on a match.
    findable: bool,
}

impl Collapsible {
    pub fn new(open: bool) -> Self {
        Collapsible {
            open,
            ..Default::default()
        }
    }

    pub fn findable(mut self) -> Self {
        self.findable = true;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    fn set(&mut self, open: bool, source: Source, reason: Reason, gate: Gate) -> bool {
        if open == self.open {
            return true;
        }
        let value = if open {
            vec![OPEN.to_string()]
        } else {
            Vec::new()
        };
        let allowed = gate(&ValueChange {
            value,
            source,
            reason,
        });
        if allowed {
            self.open = open;
        }
        allowed
    }
}

impl PartOf for Part {
    type Primitive = Collapsible;
}

impl Primitive for Collapsible {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Trigger, Part::Panel]
    }

    fn is_item(_: Part) -> bool {
        false
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

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, _: Option<&str>) -> Semantics {
        match part {
            Part::Trigger => Semantics {
                role: Some(Role::Button),
                expanded: Some(self.open),
                disabled: self.disabled,
                // A disabled trigger can still be reached, so it can be read and explained.
                focusable: Focusable::TabStop,
                ..Default::default()
            },
            Part::Panel => Semantics {
                role: Some(Role::Region),
                hidden: !self.open,
                findable: self.findable,
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        if self.disabled {
            return Reaction::default();
        }
        let reason = match input {
            Input::Press {
                part: Part::Trigger,
                ..
            } => Reason::Press,
            Input::Key {
                key: Key::Enter | Key::Space,
                ..
            } => Reason::Keyboard,
            Input::Found {
                part: Part::Panel, ..
            } if self.findable => {
                self.set(true, Source::User, Reason::Keyboard, gate);
                return Reaction::handled();
            }
            _ => return Reaction::default(),
        };
        self.set(!self.open, Source::User, reason, gate);
        Reaction::handled()
    }

    fn value(&self) -> Vec<String> {
        if self.open {
            vec![OPEN.to_string()]
        } else {
            Vec::new()
        }
    }

    fn set_value(&mut self, value: Vec<String>) {
        self.open = value.iter().any(|entry| entry == OPEN);
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "collapsible",
            label: label.to_string(),
            value: self.value(),
            expanded: Some(self.open),
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, _: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let open = match action {
            "open" => true,
            "close" => false,
            "toggle" => !self.open,
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        if self.disabled {
            return Err(ActError::Refused);
        }
        if self.set(open, Source::Agent, Reason::Action, gate) {
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

    #[test]
    fn the_trigger_opens_and_closes_the_panel() {
        let mut section = Collapsible::default();
        assert!(section.semantics(Part::Panel, None).hidden);
        section.send(
            Input::Press {
                part: Part::Trigger,
                item: None,
                mods: Modifiers::default(),
            },
            &mut |_| true,
        );
        assert_eq!(section.value(), ["open"]);
        assert_eq!(section.semantics(Part::Trigger, None).expanded, Some(true));
        section.send(
            Input::Key {
                key: Key::Enter,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        );
        assert!(!section.is_open());
    }

    #[test]
    fn find_in_page_opens_a_findable_panel() {
        let mut section = Collapsible::default().findable();
        section.send(
            Input::Found {
                part: Part::Panel,
                item: None,
            },
            &mut |_| true,
        );
        assert!(section.is_open());
    }

    #[test]
    fn a_disabled_one_stays_as_it_is() {
        let mut section = Collapsible::default().disabled();
        section.send(
            Input::Press {
                part: Part::Trigger,
                item: None,
                mods: Modifiers::default(),
            },
            &mut |_| true,
        );
        assert!(!section.is_open());
        assert_eq!(
            section.act("open", None, &mut |_| true),
            Err(ActError::Refused)
        );
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(Collapsible::default, &[]);
    }
}
