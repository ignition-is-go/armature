use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::Key;
use crate::popup::{Align, Placement, Side};
use crate::primitive::{
    Focusable, Input, Item, PartOf, Primitive, Reaction, Relations, Role, Semantics,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Trigger,
    /// The floating panel.
    Content,
    /// Names the panel.
    Title,
    /// Explains the panel.
    Description,
    /// Points from the panel to the trigger. The adapter places it.
    Arrow,
    Close,
}

const ACTIONS: &[&str] = &["open", "close"];

/// A panel that floats beside its trigger and holds anything. Unlike a dialog it does
/// not block the page: a press elsewhere closes it.
#[derive(Clone, Debug, Default)]
pub struct Popover {
    open: bool,
    focused: Option<Part>,
    modal: bool,
}

impl Popover {
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// A popover that keeps the page behind it out of reach while it is open.
    #[must_use]
    pub const fn modal(mut self) -> Self {
        self.modal = true;
        self
    }

    /// Opens or closes, unless the owner refuses.
    fn change(&mut self, open: bool, source: Source, reason: Reason, gate: Gate) -> bool {
        if open == self.open {
            return true;
        }
        let value = if open {
            vec!["open".to_string()]
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

    fn dismiss(&mut self, reason: Reason, gate: Gate) -> Reaction<Part> {
        if self.change(false, Source::User, reason, gate) {
            Reaction::focusing(Part::Trigger)
        } else {
            Reaction::handled()
        }
    }
}

impl PartOf for Part {
    type Primitive = Popover;
}

impl Primitive for Popover {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[
            Part::Trigger,
            Part::Content,
            Part::Title,
            Part::Description,
            Part::Arrow,
            Part::Close,
        ]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn popup_of(part: Part) -> Option<Placement<Part>> {
        (part == Part::Content).then_some(Placement {
            anchor: Part::Trigger,
            side: Side::Bottom,
            align: Align::Start,
            gap: 8.0,
        })
    }

    fn relations(part: Part) -> Relations<Part> {
        match part {
            Part::Trigger => Relations {
                controls: Some(Part::Content),
                ..Default::default()
            },
            Part::Content => Relations {
                labelled_by: Some(Part::Title),
                described_by: Some(Part::Description),
                ..Default::default()
            },
            _ => Relations::default(),
        }
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, _: Option<&str>) -> Semantics {
        let hidden = !self.open;
        match part {
            Part::Trigger => Semantics {
                role: Some(Role::Button),
                expanded: Some(self.open),
                focusable: Focusable::TabStop,
                ..Default::default()
            },
            Part::Content => Semantics {
                role: Some(Role::Dialog),
                hidden,
                modal: self.modal,
                focusable: Focusable::Programmatic,
                ..Default::default()
            },
            Part::Close => Semantics {
                role: Some(Role::Button),
                hidden,
                focusable: if hidden {
                    Focusable::No
                } else {
                    Focusable::TabStop
                },
                ..Default::default()
            },
            Part::Title | Part::Description | Part::Arrow => Semantics {
                hidden,
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Focus { part, .. } => {
                self.focused = Some(part);
                Reaction::default()
            }
            Input::Key {
                key: Key::Enter | Key::Space,
                mods,
                ..
            } => match self.focused {
                Some(part @ (Part::Trigger | Part::Close)) => self.send(
                    Input::Press {
                        part,
                        item: None,
                        mods,
                    },
                    gate,
                ),
                _ => Reaction::default(),
            },
            Input::Press {
                part: Part::Trigger,
                ..
            } if !self.open => {
                if self.change(true, Source::User, Reason::Press, gate) {
                    Reaction::focusing(Part::Content)
                } else {
                    Reaction::handled()
                }
            }
            Input::Press {
                part: Part::Trigger | Part::Close,
                ..
            } => self.dismiss(Reason::Press, gate),
            Input::Key {
                key: Key::Escape, ..
            } if self.open => self.dismiss(Reason::Keyboard, gate),
            // A press elsewhere, or focus leaving, closes it without pulling focus back.
            Input::PressOutside | Input::Blur { .. } | Input::WindowBlur => {
                self.change(false, Source::User, Reason::Press, gate);
                Reaction::default()
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        if self.open {
            vec!["open".to_string()]
        } else {
            Vec::new()
        }
    }

    fn set_value(&mut self, value: Vec<String>) {
        self.open = value.iter().any(|entry| entry == "open");
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "popover",
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
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        if self.change(open, Source::Agent, Reason::Action, gate) {
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

    fn press(popover: &mut Popover, part: Part) -> Reaction<Part> {
        popover.send(
            Input::Press {
                part,
                item: None,
                mods: Modifiers::default(),
            },
            &mut |_| true,
        )
    }

    #[test]
    fn the_panel_is_named_and_described_by_its_own_parts() {
        let relations = Popover::relations(Part::Content);
        assert_eq!(
            (relations.labelled_by, relations.described_by),
            (Some(Part::Title), Some(Part::Description))
        );
    }

    #[test]
    fn opening_moves_focus_in_and_escape_returns_it() {
        let mut popover = Popover::default();
        assert_eq!(
            press(&mut popover, Part::Trigger).focus.unwrap().part,
            Part::Content
        );
        assert!(!popover.semantics(Part::Arrow, None).hidden);
        let closed = popover.send(
            Input::Key {
                key: Key::Escape,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        );
        assert_eq!(closed.focus.unwrap().part, Part::Trigger);
        assert!(popover.semantics(Part::Title, None).hidden);
    }

    #[test]
    fn a_press_elsewhere_closes_it_without_taking_focus() {
        let mut popover = Popover::default();
        press(&mut popover, Part::Trigger);
        assert_eq!(
            popover.send(Input::PressOutside, &mut |_| true),
            Reaction::default()
        );
        assert!(!popover.is_open());
    }

    #[test]
    fn switching_to_another_window_closes_it() {
        let mut popover = Popover::default();
        press(&mut popover, Part::Trigger);
        popover.send(Input::WindowBlur, &mut |_| true);
        assert!(!popover.is_open());
    }

    #[test]
    fn focus_leaving_closes_it() {
        let mut popover = Popover::default();
        press(&mut popover, Part::Trigger);
        popover.send(
            Input::Blur {
                part: Part::Content,
                item: None,
            },
            &mut |_| true,
        );
        assert!(!popover.is_open());
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(Popover::default, &[]);
    }
}
