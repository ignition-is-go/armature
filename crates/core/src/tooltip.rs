use crate::agent::{ActError, Snapshot};
use crate::change::Gate;
use crate::keys::Key;
use crate::popup::{Align, Placement, Side};
use crate::primitive::{
    Effect, Focusable, Input, Item, PartOf, Primitive, Reaction, Relations, Role, Semantics,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// What the tooltip describes. Give it your own role and behavior.
    Trigger,
    Content,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// How long the pointer rests on the trigger before the tooltip shows.
    pub open_delay_ms: u64,
    /// How long after the pointer leaves before it hides.
    pub close_delay_ms: u64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            open_delay_ms: 500,
            close_delay_ms: 0,
        }
    }
}

const ACTIONS: &[&str] = &["show", "hide"];

/// A short description shown while the pointer rests on its trigger or the trigger
/// has keyboard focus. Moving onto the tooltip itself keeps it open.
#[derive(Clone, Debug, Default)]
pub struct Tooltip {
    opts: Options,
    open: bool,
    /// When a pending show or hide takes effect, and which it is.
    due: Option<(u64, bool)>,
}

impl Tooltip {
    pub fn new(opts: Options) -> Self {
        Tooltip {
            opts,
            ..Default::default()
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Shows or hides after a delay, or at once when there is none.
    fn schedule(&mut self, open: bool, now_ms: u64) -> Reaction<Part> {
        let delay = if open {
            self.opts.open_delay_ms
        } else {
            self.opts.close_delay_ms
        };
        if open == self.open {
            self.due = None;
            return Reaction::default();
        }
        if delay == 0 {
            self.due = None;
            self.open = open;
            return Reaction::default();
        }
        let at_ms = now_ms + delay;
        self.due = Some((at_ms, open));
        Reaction::default().with(Effect::Wake { at_ms })
    }
}

impl PartOf for Part {
    type Primitive = Tooltip;
}

impl Primitive for Tooltip {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Trigger, Part::Content]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn hovers(_: Part) -> bool {
        true
    }

    fn popup_of(part: Part) -> Option<Placement<Part>> {
        (part == Part::Content).then_some(Placement {
            anchor: Part::Trigger,
            side: Side::Top,
            align: Align::Start,
            gap: 6.0,
        })
    }

    fn relations(part: Part) -> Relations<Part> {
        match part {
            Part::Trigger => Relations {
                described_by: Some(Part::Content),
                ..Default::default()
            },
            Part::Content => Relations::default(),
        }
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, _: Option<&str>) -> Semantics {
        match part {
            Part::Trigger => Semantics {
                focusable: Focusable::TabStop,
                ..Default::default()
            },
            Part::Content => Semantics {
                role: Some(Role::Tooltip),
                hidden: !self.open,
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, _: Gate) -> Reaction<Part> {
        match input {
            Input::Hover { inside, now_ms, .. } => self.schedule(inside, now_ms),
            // Keyboard focus shows it at once: there is no pointer to wait for.
            Input::Focus {
                part: Part::Trigger,
                ..
            } => {
                self.due = None;
                self.open = true;
                Reaction::default()
            }
            Input::Tick { now_ms } => {
                if let Some((_, open)) = self.due.filter(|(at_ms, _)| now_ms >= *at_ms) {
                    self.open = open;
                    self.due = None;
                }
                Reaction::default()
            }
            // Escape hides it but is left for whatever else is listening.
            Input::Key {
                key: Key::Escape, ..
            }
            | Input::Press { .. }
            | Input::PressOutside
            | Input::WindowBlur => {
                self.due = None;
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
            name: "tooltip",
            label: label.to_string(),
            value: Vec::new(),
            expanded: Some(self.open),
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, _: Option<&str>, _: Gate) -> Result<(), ActError> {
        self.open = match action {
            "show" => true,
            "hide" => false,
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        self.due = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Modifiers;

    fn hover(tooltip: &mut Tooltip, part: Part, inside: bool, now_ms: u64) -> Reaction<Part> {
        tooltip.send(
            Input::Hover {
                part,
                item: None,
                inside,
                now_ms,
            },
            &mut |_| true,
        )
    }

    fn tick(tooltip: &mut Tooltip, now_ms: u64) {
        tooltip.send(Input::Tick { now_ms }, &mut |_| true);
    }

    #[test]
    fn it_shows_once_the_pointer_has_rested_long_enough() {
        let mut tooltip = Tooltip::default();
        let waiting = hover(&mut tooltip, Part::Trigger, true, 1000);
        assert_eq!(waiting.effects, [Effect::Wake { at_ms: 1500 }]);
        tick(&mut tooltip, 1400);
        assert!(!tooltip.is_open());
        tick(&mut tooltip, 1500);
        assert!(tooltip.is_open());
    }

    #[test]
    fn leaving_before_it_shows_cancels_it() {
        let mut tooltip = Tooltip::default();
        hover(&mut tooltip, Part::Trigger, true, 0);
        hover(&mut tooltip, Part::Trigger, false, 100);
        tick(&mut tooltip, 500);
        assert!(!tooltip.is_open());
    }

    #[test]
    fn moving_from_the_trigger_onto_the_tooltip_keeps_it_open() {
        let mut tooltip = Tooltip::new(Options {
            open_delay_ms: 0,
            close_delay_ms: 100,
        });
        hover(&mut tooltip, Part::Trigger, true, 0);
        hover(&mut tooltip, Part::Trigger, false, 10);
        hover(&mut tooltip, Part::Content, true, 50);
        tick(&mut tooltip, 200);
        assert!(tooltip.is_open());
        hover(&mut tooltip, Part::Content, false, 300);
        tick(&mut tooltip, 400);
        assert!(!tooltip.is_open());
    }

    #[test]
    fn keyboard_focus_shows_it_at_once_and_escape_hides_it() {
        let mut tooltip = Tooltip::default();
        tooltip.send(
            Input::Focus {
                part: Part::Trigger,
                item: None,
            },
            &mut |_| true,
        );
        assert!(tooltip.is_open());
        let escaped = tooltip.send(
            Input::Key {
                key: Key::Escape,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        );
        assert!(!tooltip.is_open());
        assert!(!escaped.handled, "Escape still reaches a dialog around it");
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(Tooltip::default, &[]);
    }
}
