use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::geometry::Point;
use crate::keys::Key;
use crate::primitive::{
    Effect, Focusable, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive, Reaction,
    Role, Semantics,
};
use crate::timing::Hold;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
}

const ACTIONS: &[&str] = &["confirm"];
const CONFIRMED: &str = "confirmed";

/// A button that acts only once it has been held down, for things that should not
/// happen by accident. Letting go early, or drifting off it, does nothing. From the
/// keyboard, Enter and Space confirm at once: holding a key is not asked of anyone.
#[derive(Clone, Debug)]
pub struct HoldButton {
    hold: Hold,
    delay_ms: u64,
    /// When and where the press began.
    pressed: Option<(u64, Point)>,
}

impl Default for HoldButton {
    fn default() -> Self {
        Self::new(800)
    }
}

impl HoldButton {
    #[must_use]
    pub fn new(delay_ms: u64) -> Self {
        Self {
            hold: Hold::new(delay_ms),
            delay_ms,
            pressed: None,
        }
    }

    /// How far through the hold the press is, from 0 to 1, for drawing progress.
    #[must_use]
    pub fn progress(&self, now_ms: u64) -> f64 {
        match self.pressed {
            Some((since, _)) => {
                (now_ms.saturating_sub(since) as f64 / self.delay_ms.max(1) as f64).min(1.0)
            }
            None => 0.0,
        }
    }

    fn confirm(&self, source: Source, reason: Reason, gate: Gate) -> bool {
        gate(&ValueChange {
            value: vec![CONFIRMED.to_string()],
            source,
            reason,
        })
    }
}

impl PartOf for Part {
    type Primitive = HoldButton;
}

impl Primitive for HoldButton {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn tracks_pointer(_: Part) -> bool {
        true
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, _: Part, _: Option<&str>) -> Semantics {
        Semantics {
            role: Some(Role::Button),
            focusable: Focusable::TabStop,
            active: self.pressed.is_some(),
            ..Default::default()
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Key {
                key: Key::Enter | Key::Space,
                ..
            }
            | Input::Press { .. } => {
                self.confirm(Source::User, Reason::Keyboard, gate);
                Reaction::handled()
            }
            Input::Tick { now_ms } => {
                if self.hold.tick(now_ms) {
                    self.pressed = None;
                    self.confirm(Source::User, Reason::Press, gate);
                }
                Reaction::default()
            }
            _ => Reaction::default(),
        }
    }

    fn pointer(&mut self, pointer: Pointer<Part>, _: Measure<Part>, _: Gate) -> Reaction<Part> {
        match pointer.phase {
            PointerPhase::Down => {
                self.pressed = Some((pointer.now_ms, pointer.at));
                let at_ms = self.hold.start(pointer.now_ms);
                Reaction::focusing(Part::Root)
                    .with(Effect::CapturePointer)
                    .with(Effect::Wake { at_ms })
            }
            PointerPhase::Move => {
                if let Some((_, from)) = self.pressed {
                    self.hold.moved(pointer.at.distance(from));
                }
                Reaction::default()
            }
            PointerPhase::Up | PointerPhase::Cancel => {
                self.hold.release();
                self.pressed = None;
                Reaction::handled()
            }
        }
    }

    fn value(&self) -> Vec<String> {
        Vec::new()
    }

    fn set_value(&mut self, _: Vec<String>) {}

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "hold-button",
            label: label.to_string(),
            value: Vec::new(),
            expanded: None,
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, _: Option<&str>, gate: Gate) -> Result<(), ActError> {
        if action != "confirm" {
            return Err(ActError::UnknownAction {
                action: action.to_string(),
                valid: ACTIONS,
            });
        }
        if self.confirm(Source::Agent, Reason::Action, gate) {
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

    fn pointer(
        button: &mut HoldButton,
        phase: PointerPhase,
        x: f64,
        now_ms: u64,
    ) -> Reaction<Part> {
        let pointer = Pointer {
            phase,
            part: Part::Root,
            item: None,
            at: Point::new(x, 0.0),
            mods: Modifiers::default(),
            now_ms,
        };
        button.pointer(pointer, &|_, _| None, &mut |_| true)
    }

    fn tick(button: &mut HoldButton, now_ms: u64) -> bool {
        let mut confirmed = false;
        button.send(Input::Tick { now_ms }, &mut |_| {
            confirmed = true;
            true
        });
        confirmed
    }

    #[test]
    fn it_confirms_once_the_press_has_lasted() {
        let mut button = HoldButton::default();
        let pressed = pointer(&mut button, PointerPhase::Down, 0.0, 1000);
        assert!(pressed.effects.contains(&Effect::Wake { at_ms: 1800 }));
        assert_eq!(button.progress(1400), 0.5);
        assert!(!tick(&mut button, 1700));
        assert!(tick(&mut button, 1800));
        assert!(!tick(&mut button, 3000), "once per press");
    }

    #[test]
    fn letting_go_early_does_nothing() {
        let mut button = HoldButton::default();
        pointer(&mut button, PointerPhase::Down, 0.0, 0);
        pointer(&mut button, PointerPhase::Up, 0.0, 300);
        assert!(!tick(&mut button, 800));
        assert_eq!(button.progress(800), 0.0);
    }

    #[test]
    fn drifting_off_cancels_the_hold() {
        let mut button = HoldButton::default();
        pointer(&mut button, PointerPhase::Down, 0.0, 0);
        pointer(&mut button, PointerPhase::Move, 40.0, 100);
        assert!(!tick(&mut button, 800));
    }

    #[test]
    fn the_keyboard_confirms_without_a_hold() {
        let mut button = HoldButton::default();
        let mut confirmed = false;
        button.send(
            Input::Key {
                key: Key::Enter,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| {
                confirmed = true;
                true
            },
        );
        assert!(confirmed);
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(HoldButton::default, &[]);
    }
}
