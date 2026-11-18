use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::Key;
use crate::primitive::{Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
}

const ACTIONS: &[&str] = &["press"];
const PRESSED: &str = "pressed";

/// Whether a button takes presses, and whether a disabled one keeps its place in the
/// Tab order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Availability {
    #[default]
    Enabled,
    Disabled,
    /// Disabled, but focusable, so it can hold focus as it becomes disabled and carry a
    /// tooltip saying why.
    DisabledButFocusable,
}

/// A button.
///
/// A press is offered to the gate, which runs the action. It keeps no value, except
/// that the owner may mark it pending with `set_value(["pending"])` while its action
/// runs: it then ignores presses and says so to a screen reader.
#[derive(Clone, Debug)]
pub struct Button {
    availability: Availability,
    pending: bool,
    /// What a screen reader hears when a pending button is pressed.
    pending_message: String,
}

impl Default for Button {
    fn default() -> Self {
        Self {
            availability: Availability::Enabled,
            pending: false,
            pending_message: "Loading".into(),
        }
    }
}

impl Button {
    #[must_use]
    pub const fn disabled(mut self) -> Self {
        self.availability = Availability::Disabled;
        self
    }

    /// Disabled, but kept in the Tab order, so it can hold focus as it becomes
    /// disabled and carry a tooltip saying why.
    #[must_use]
    pub const fn disabled_but_focusable(mut self) -> Self {
        self.availability = Availability::DisabledButFocusable;
        self
    }

    const fn is_disabled(&self) -> bool {
        !matches!(self.availability, Availability::Enabled)
    }

    #[must_use]
    pub fn pending_message(mut self, message: &str) -> Self {
        self.pending_message = message.to_string();
        self
    }

    #[must_use]
    pub const fn is_pending(&self) -> bool {
        self.pending
    }

    fn press(source: Source, reason: Reason, gate: Gate) -> bool {
        gate(&ValueChange {
            value: vec![PRESSED.to_string()],
            source,
            reason,
        })
    }
}

impl PartOf for Part {
    type Primitive = Button;
}

impl Primitive for Button {
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
            role: Some(Role::Button),
            disabled: self.is_disabled() || self.pending,
            focusable: if self.availability == Availability::Disabled {
                Focusable::No
            } else {
                Focusable::TabStop
            },
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
        if self.is_disabled() {
            return Reaction::handled();
        }
        if self.pending {
            return if self.pending_message.is_empty() {
                Reaction::handled()
            } else {
                Reaction::handled().announcing(self.pending_message.clone())
            };
        }
        Self::press(Source::User, reason, gate);
        Reaction::handled()
    }

    fn value(&self) -> Vec<String> {
        if self.pending {
            vec!["pending".to_string()]
        } else {
            Vec::new()
        }
    }

    fn set_value(&mut self, value: Vec<String>) {
        self.pending = value.iter().any(|entry| entry == "pending");
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "button",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, _: Option<&str>, gate: Gate) -> Result<(), ActError> {
        if action != "press" {
            return Err(ActError::UnknownAction {
                action: action.to_string(),
                valid: ACTIONS,
            });
        }
        if self.is_disabled() || self.pending {
            return Err(ActError::Refused);
        }
        if Self::press(Source::Agent, Reason::Action, gate) {
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
    use crate::primitive::Effect;

    fn press(button: &mut Button) -> (bool, Reaction<Part>) {
        let mut ran = false;
        let reaction = button.send(
            Input::Press {
                part: Part::Root,
                item: None,
                mods: Modifiers::default(),
            },
            &mut |_| {
                ran = true;
                true
            },
        );
        (ran, reaction)
    }

    #[test]
    fn a_press_runs_the_action_and_a_disabled_button_does_nothing() {
        assert!(press(&mut Button::default()).0);
        let mut off = Button::default().disabled();
        assert!(!press(&mut off).0);
        assert_eq!(off.semantics(Part::Root, None).focusable, Focusable::No);
        assert_eq!(
            Button::default()
                .disabled_but_focusable()
                .semantics(Part::Root, None)
                .focusable,
            Focusable::TabStop
        );
    }

    #[test]
    fn a_pending_button_ignores_presses_and_says_it_is_loading() {
        let mut button = Button::default();
        button.set_value(vec!["pending".into()]);
        let (ran, reaction) = press(&mut button);
        assert!(!ran);
        assert_eq!(
            reaction.effects,
            [Effect::Announce {
                message: "Loading".into(),
                assertive: false
            }]
        );
        assert_eq!(
            button.act("press", None, &mut |_| true),
            Err(ActError::Refused)
        );
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(Button::default, &[]);
    }
}
