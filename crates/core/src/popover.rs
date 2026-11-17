use crate::agent::{ActError, Snapshot};
use crate::change::Gate;
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
}

impl Popover {
    pub fn is_open(&self) -> bool {
        self.open
    }

    fn dismiss(&mut self) -> Reaction<Part> {
        self.open = false;
        Reaction::focusing(Part::Trigger)
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

    fn send(&mut self, input: Input<Part>, _: Gate) -> Reaction<Part> {
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
                    &mut |_| true,
                ),
                _ => Reaction::default(),
            },
            Input::Press {
                part: Part::Trigger,
                ..
            } if !self.open => {
                self.open = true;
                Reaction::focusing(Part::Content)
            }
            Input::Press {
                part: Part::Trigger | Part::Close,
                ..
            } => self.dismiss(),
            Input::Key {
                key: Key::Escape, ..
            } if self.open => self.dismiss(),
            // A press elsewhere, or focus leaving, closes it without pulling focus back.
            Input::PressOutside | Input::Blur { .. } => {
                self.open = false;
                Reaction::default()
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        Vec::new()
    }

    fn set_value(&mut self, _: Vec<String>) {}

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "popover",
            label: label.to_string(),
            value: Vec::new(),
            expanded: Some(self.open),
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, _: Option<&str>, _: Gate) -> Result<(), ActError> {
        self.open = match action {
            "open" => true,
            "close" => false,
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        Ok(())
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
