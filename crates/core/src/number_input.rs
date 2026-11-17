use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::Key;
use crate::number::{NumberRange, parse_number};
use crate::primitive::{
    Effect, Focusable, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive, Reaction,
    Role, Semantics, ValueRange,
};
use crate::timing::Repeat;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The text field.
    Input,
    /// Steps up once when pressed and keeps stepping while held.
    Increment,
    Decrement,
}

const ACTIONS: &[&str] = &["set", "increment", "decrement"];

/// A number typed into a field or stepped with buttons and arrow keys.
#[derive(Clone, Debug)]
pub struct NumberInput {
    range: NumberRange,
    value: f64,
    /// What has been typed and not yet committed.
    draft: Option<String>,
    repeat: Repeat,
    /// Which way the held button steps.
    held: f64,
}

impl Default for NumberInput {
    fn default() -> Self {
        NumberInput::new(NumberRange::new(0.0, 100.0, 1.0), 0.0)
    }
}

impl NumberInput {
    pub fn new(range: NumberRange, value: f64) -> Self {
        NumberInput {
            range,
            value: range.snap(value),
            draft: None,
            repeat: Repeat::default(),
            held: 0.0,
        }
    }

    pub fn number(&self) -> f64 {
        self.value
    }

    fn set(&mut self, value: f64, source: Source, reason: Reason, gate: Gate) -> bool {
        let value = self.range.snap(value);
        self.draft = None;
        if value == self.value {
            return true;
        }
        let allowed = gate(&ValueChange {
            value: vec![value.to_string()],
            source,
            reason,
        });
        if allowed {
            self.value = value;
        }
        allowed
    }

    /// Commits what was typed. Text that is not a number is discarded.
    fn commit(&mut self, gate: Gate) {
        if let Some(typed) = self.draft.take().and_then(|draft| parse_number(&draft)) {
            self.set(typed, Source::User, Reason::Text, gate);
        }
    }

    fn step(&mut self, steps: f64, reason: Reason, gate: Gate) {
        let from = self
            .draft
            .as_deref()
            .and_then(parse_number)
            .unwrap_or(self.value);
        self.set(
            self.range.nudge(self.range.clamp(from), steps),
            Source::User,
            reason,
            gate,
        );
    }
}

impl PartOf for Part {
    type Primitive = NumberInput;
}

impl Primitive for NumberInput {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Input, Part::Increment, Part::Decrement]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn tracks_pointer(part: Part) -> bool {
        part != Part::Input
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, _: Option<&str>) -> Semantics {
        match part {
            Part::Input => Semantics {
                role: Some(Role::SpinButton),
                range: Some(ValueRange {
                    now: self.value,
                    min: self.range.min,
                    max: self.range.max,
                }),
                focusable: Focusable::TabStop,
                ..Default::default()
            },
            Part::Increment => Semantics {
                role: Some(Role::Button),
                disabled: self.value >= self.range.max,
                ..Default::default()
            },
            Part::Decrement => Semantics {
                role: Some(Role::Button),
                disabled: self.value <= self.range.min,
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Text { value, .. } => {
                self.draft = Some(value);
                Reaction::handled()
            }
            // PageUp, PageDown and Shift with an arrow go ten steps at a time.
            Input::Key {
                key: Key::PageUp, ..
            } => {
                self.step(10.0, Reason::Keyboard, gate);
                Reaction::handled()
            }
            Input::Key {
                key: Key::PageDown, ..
            } => {
                self.step(-10.0, Reason::Keyboard, gate);
                Reaction::handled()
            }
            Input::Key {
                key: Key::ArrowUp,
                mods,
                ..
            } => {
                self.step(if mods.shift { 10.0 } else { 1.0 }, Reason::Keyboard, gate);
                Reaction::handled()
            }
            Input::Key {
                key: Key::ArrowDown,
                mods,
                ..
            } => {
                self.step(
                    if mods.shift { -10.0 } else { -1.0 },
                    Reason::Keyboard,
                    gate,
                );
                Reaction::handled()
            }
            Input::Key {
                key: Key::Enter, ..
            } => {
                self.commit(gate);
                Reaction::handled()
            }
            // Leaving the field commits what was typed, as a native number field does.
            Input::Key { key: Key::Tab, .. } => {
                self.commit(gate);
                Reaction::default()
            }
            Input::Tick { now_ms } => {
                let (steps, next) = self.repeat.tick(now_ms);
                if steps > 0 {
                    self.step(self.held * steps as f64, Reason::Press, gate);
                }
                match next {
                    Some(at_ms) => Reaction::handled().with(Effect::Wake { at_ms }),
                    None => Reaction::default(),
                }
            }
            _ => Reaction::default(),
        }
    }

    fn pointer(&mut self, pointer: Pointer<Part>, _: Measure<Part>, gate: Gate) -> Reaction<Part> {
        match pointer.phase {
            PointerPhase::Down => {
                self.held = if pointer.part == Part::Increment {
                    1.0
                } else {
                    -1.0
                };
                self.step(self.held, Reason::Press, gate);
                let at_ms = self.repeat.start(pointer.now_ms);
                Reaction::focusing(Part::Input)
                    .with(Effect::CapturePointer)
                    .with(Effect::Wake { at_ms })
            }
            PointerPhase::Up | PointerPhase::Cancel => {
                self.repeat.release();
                Reaction::handled()
            }
            PointerPhase::Move => Reaction::default(),
        }
    }

    fn text(&self, part: Part, _: Option<&str>) -> Option<String> {
        (part == Part::Input).then(|| self.draft.clone().unwrap_or_else(|| self.value.to_string()))
    }

    fn value(&self) -> Vec<String> {
        vec![self.value.to_string()]
    }

    fn set_value(&mut self, value: Vec<String>) {
        if let Some(number) = value.first().and_then(|text| parse_number(text)) {
            self.value = self.range.snap(number);
            self.draft = None;
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "number",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let target = match action {
            "increment" => self.range.nudge(self.value, 1.0),
            "decrement" => self.range.nudge(self.value, -1.0),
            "set" => {
                let text = item.ok_or(ActError::MissingItem { action: "set" })?;
                parse_number(text).ok_or_else(|| ActError::UnknownItem(text.to_string()))?
            }
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        if self.set(target, Source::Agent, Reason::Action, gate) {
            Ok(())
        } else {
            Err(ActError::Refused)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;
    use crate::keys::Modifiers;

    fn typed(input: &mut NumberInput, text: &str) {
        input.send(
            Input::Text {
                part: Part::Input,
                item: None,
                value: text.into(),
            },
            &mut |_| true,
        );
    }

    fn key(input: &mut NumberInput, key: Key) {
        input.send(
            Input::Key {
                key,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        );
    }

    fn pointer(
        input: &mut NumberInput,
        phase: PointerPhase,
        part: Part,
        now_ms: u64,
    ) -> Reaction<Part> {
        let pointer = Pointer {
            phase,
            part,
            item: None,
            at: Point::default(),
            mods: Modifiers::default(),
            now_ms,
        };
        input.pointer(pointer, &|_, _| None, &mut |_| true)
    }

    #[test]
    fn typed_text_is_shown_as_typed_and_committed_on_enter() {
        let mut input = NumberInput::default();
        typed(&mut input, "4");
        typed(&mut input, "42.6");
        assert_eq!(input.text(Part::Input, None).as_deref(), Some("42.6"));
        assert_eq!(input.number(), 0.0);
        key(&mut input, Key::Enter);
        assert_eq!(input.number(), 43.0);
        assert_eq!(input.text(Part::Input, None).as_deref(), Some("43"));
    }

    #[test]
    fn text_that_is_not_a_number_is_discarded() {
        let mut input = NumberInput::new(NumberRange::new(0.0, 10.0, 1.0), 5.0);
        typed(&mut input, "lots");
        key(&mut input, Key::Enter);
        assert_eq!(input.number(), 5.0);
        assert_eq!(input.text(Part::Input, None).as_deref(), Some("5"));
    }

    #[test]
    fn arrows_step_from_what_is_typed_and_stay_in_range() {
        let mut input = NumberInput::new(NumberRange::new(0.0, 10.0, 1.0), 5.0);
        typed(&mut input, "9");
        key(&mut input, Key::ArrowUp);
        key(&mut input, Key::ArrowUp);
        assert_eq!(input.number(), 10.0);
        assert!(input.semantics(Part::Increment, None).disabled);
    }

    #[test]
    fn a_held_button_steps_once_then_repeats_until_released() {
        let mut input = NumberInput::default();
        let pressed = pointer(&mut input, PointerPhase::Down, Part::Increment, 1000);
        assert_eq!(input.number(), 1.0);
        assert!(pressed.effects.contains(&Effect::Wake { at_ms: 1400 }));
        let tick = input.send(Input::Tick { now_ms: 1400 }, &mut |_| true);
        assert_eq!(input.number(), 2.0);
        assert!(tick.effects.contains(&Effect::Wake { at_ms: 1460 }));
        input.send(Input::Tick { now_ms: 1580 }, &mut |_| true);
        assert_eq!(input.number(), 5.0, "a late tick catches up");
        pointer(&mut input, PointerPhase::Up, Part::Increment, 1600);
        let after = input.send(Input::Tick { now_ms: 5000 }, &mut |_| true);
        assert_eq!(input.number(), 5.0);
        assert!(after.effects.is_empty());
    }

    #[test]
    fn the_decrement_button_steps_down() {
        let mut input = NumberInput::new(NumberRange::new(0.0, 10.0, 1.0), 5.0);
        pointer(&mut input, PointerPhase::Down, Part::Decrement, 0);
        assert_eq!(input.number(), 4.0);
    }

    #[test]
    fn page_keys_and_shift_go_ten_steps_at_a_time() {
        let mut input = NumberInput::new(NumberRange::new(0.0, 100.0, 1.0), 5.0);
        key(&mut input, Key::PageUp);
        assert_eq!(input.value(), ["15"]);
        input.send(
            Input::Key {
                key: Key::ArrowDown,
                mods: Modifiers {
                    shift: true,
                    ..Default::default()
                },
                now_ms: 0,
            },
            &mut |_| true,
        );
        assert_eq!(input.value(), ["5"]);
    }
}
