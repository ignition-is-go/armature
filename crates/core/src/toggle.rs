use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::Key;
use crate::primitive::{Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
}

const ACTIONS: &[&str] = &["toggle", "check", "uncheck"];
const ON: &str = "on";

/// A two-state switch. Its value is `["on"]` when on and empty when off.
#[derive(Clone, Debug, Default)]
pub struct Toggle {
    on: bool,
}

impl Toggle {
    pub fn new(on: bool) -> Self {
        Toggle { on }
    }

    pub fn is_on(&self) -> bool {
        self.on
    }

    fn set(&mut self, on: bool, source: Source, reason: Reason, gate: Gate) -> bool {
        if on == self.on {
            return true;
        }
        let value = if on { vec![ON.to_string()] } else { Vec::new() };
        let allowed = gate(&ValueChange {
            value,
            source,
            reason,
        });
        if allowed {
            self.on = on;
        }
        allowed
    }
}

impl PartOf for Part {
    type Primitive = Toggle;
}

impl Primitive for Toggle {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, _: Part, _: Option<&str>) -> Semantics {
        Semantics {
            role: Some(Role::Switch),
            checked: Some(self.on),
            focusable: Focusable::TabStop,
            ..Default::default()
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        let reason = match input {
            Input::Press { .. } => Reason::Press,
            Input::Key {
                key: Key::Enter | Key::Space,
                ..
            } => Reason::Keyboard,
            _ => return Reaction::default(),
        };
        self.set(!self.on, Source::User, reason, gate);
        Reaction::handled()
    }

    fn value(&self) -> Vec<String> {
        if self.on {
            vec![ON.to_string()]
        } else {
            Vec::new()
        }
    }

    fn set_value(&mut self, value: Vec<String>) {
        self.on = value.iter().any(|v| v == ON);
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "switch",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, _: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let on = match action {
            "toggle" => !self.on,
            "check" => true,
            "uncheck" => false,
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        if self.set(on, Source::Agent, Reason::Action, gate) {
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
    fn a_press_or_space_flips_it() {
        let mut toggle = Toggle::default();
        toggle.send(
            Input::Press {
                part: Part::Root,
                item: None,
                mods: Modifiers::default(),
            },
            &mut |_| true,
        );
        assert_eq!(toggle.value(), ["on"]);
        assert_eq!(toggle.semantics(Part::Root, None).checked, Some(true));
        toggle.send(
            Input::Key {
                key: Key::Space,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        );
        assert!(!toggle.is_on());
    }

    #[test]
    fn the_owner_can_refuse_and_an_agent_can_set_it_outright() {
        let mut toggle = Toggle::default();
        toggle.send(
            Input::Press {
                part: Part::Root,
                item: None,
                mods: Modifiers::default(),
            },
            &mut |_| false,
        );
        assert!(!toggle.is_on());
        toggle.act("check", None, &mut |_| true).unwrap();
        toggle.act("check", None, &mut |_| false).unwrap();
        assert!(toggle.is_on(), "already on, so nothing to refuse");
        assert_eq!(
            toggle.act("uncheck", None, &mut |_| false),
            Err(ActError::Refused)
        );
    }
}
