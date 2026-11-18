use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::Key;
use crate::popup::{Align, Placement, Side};
use crate::primitive::{
    Effect, Focusable, Input, Item, PartOf, Primitive, Reaction, Relations, Role, Semantics,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

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
    /// A card of richer content that the pointer can move into and use, such as a
    /// preview of a link, in place of a short description.
    pub card: bool,
    /// A disabled tooltip ignores the pointer and focus. An agent or the owner can
    /// still show it.
    pub disabled: bool,
}

/// Shared by tooltips that sit together, such as a toolbar's. Once one has been
/// shown, moving to the next shows it at once instead of waiting again.
#[derive(Clone, Debug, Default)]
pub struct Warmth(Arc<AtomicU64>);

impl Warmth {
    /// How long after one tooltip hides the next still opens at once.
    pub const WINDOW_MS: u64 = 300;

    fn closed(&self, now_ms: u64) {
        self.0.store(now_ms.saturating_add(1), Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_warm(&self, now_ms: u64) -> bool {
        let closed = self.0.load(Ordering::Relaxed);
        closed > 0
            && now_ms.saturating_add(1) >= closed
            && (now_ms.saturating_add(1)).saturating_sub(closed) <= Self::WINDOW_MS
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            open_delay_ms: 500,
            close_delay_ms: 0,
            card: false,
            disabled: false,
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
    warmth: Option<Warmth>,
}

impl Tooltip {
    #[must_use]
    pub fn new(opts: Options) -> Self {
        Self {
            opts,
            ..Default::default()
        }
    }

    /// A hover card: it waits longer to open and lingers after the pointer leaves,
    /// so there is time to move into it.
    #[must_use]
    pub fn hover_card() -> Self {
        Self::new(Options {
            open_delay_ms: 600,
            close_delay_ms: 300,
            card: true,
            disabled: false,
        })
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// Joins the tooltips that share `warmth`.
    #[must_use]
    pub fn sharing(mut self, warmth: Warmth) -> Self {
        self.warmth = Some(warmth);
        self
    }

    /// Shows or hides now, unless the owner refuses.
    fn change(
        &mut self,
        open: bool,
        source: Source,
        reason: Reason,
        now_ms: Option<u64>,
        gate: Gate,
    ) -> bool {
        self.due = None;
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
            if let (false, Some(warmth), Some(now_ms)) = (open, &self.warmth, now_ms) {
                warmth.closed(now_ms);
            }
        }
        allowed
    }

    /// Shows or hides after a delay, or at once when there is none.
    fn schedule(&mut self, open: bool, now_ms: u64, gate: Gate) -> Reaction<Part> {
        let delay = if open
            && self
                .warmth
                .as_ref()
                .is_some_and(|warmth| warmth.is_warm(now_ms))
        {
            0
        } else if open {
            self.opts.open_delay_ms
        } else {
            self.opts.close_delay_ms
        };
        if open == self.open {
            self.due = None;
            return Reaction::default();
        }
        if delay == 0 {
            self.change(open, Source::User, Reason::Hover, Some(now_ms), gate);
            return Reaction::default();
        }
        let at_ms = now_ms.saturating_add(delay);
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
                role: Some(if self.opts.card {
                    Role::Dialog
                } else {
                    Role::Tooltip
                }),
                hidden: !self.open,
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Hover { inside: true, .. } | Input::Focus { .. } if self.opts.disabled => {
                Reaction::default()
            }
            Input::Hover { inside, now_ms, .. } => self.schedule(inside, now_ms, gate),
            // Keyboard focus shows it at once: there is no pointer to wait for.
            Input::Focus {
                part: Part::Trigger,
                ..
            } => {
                self.change(true, Source::User, Reason::Keyboard, None, gate);
                Reaction::default()
            }
            Input::Tick { now_ms } => {
                if let Some((_, open)) = self.due.filter(|(at_ms, _)| now_ms >= *at_ms) {
                    self.change(open, Source::User, Reason::Hover, Some(now_ms), gate);
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
                self.change(false, Source::User, Reason::Press, None, gate);
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

    fn act(&mut self, action: &str, _: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let open = match action {
            "show" => true,
            "hide" => false,
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        if self.change(open, Source::Agent, Reason::Action, None, gate) {
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
            ..Default::default()
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

    #[test]
    fn a_hover_card_opens_late_and_lingers_so_the_pointer_can_reach_it() {
        let mut card = Tooltip::hover_card();
        let hover = |card: &mut Tooltip, part, inside, now_ms| {
            card.send(
                Input::Hover {
                    part,
                    item: None,
                    inside,
                    now_ms,
                },
                &mut |_| true,
            );
            card.send(Input::Tick { now_ms }, &mut |_| true);
        };
        hover(&mut card, Part::Trigger, true, 0);
        card.send(Input::Tick { now_ms: 500 }, &mut |_| true);
        assert!(!card.is_open());
        card.send(Input::Tick { now_ms: 600 }, &mut |_| true);
        assert!(card.is_open());
        hover(&mut card, Part::Trigger, false, 700);
        hover(&mut card, Part::Content, true, 850);
        card.send(Input::Tick { now_ms: 1000 }, &mut |_| true);
        assert!(
            card.is_open(),
            "the pointer reached the card before it closed"
        );
        assert_eq!(card.semantics(Part::Content, None).role, Some(Role::Dialog));
    }
}
