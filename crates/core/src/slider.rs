use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::geometry::{Point, Rect};
use crate::keys::{Dir, Key, Orientation};
use crate::number::{NumberRange, read_number};
use crate::primitive::{
    Effect, Focusable, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive, Reaction,
    Role, Semantics, ValueRange,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The full length the thumb travels. Pressing it moves the thumb there.
    Track,
    Thumb,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub range: NumberRange,
    pub orientation: Orientation,
    pub dir: Dir,
    /// Steps taken by `PageUp`, `PageDown` and Shift with an arrow.
    pub large_step: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            range: NumberRange::new(0.0, 100.0, 1.0),
            orientation: Orientation::Horizontal,
            dir: Dir::Ltr,
            large_step: 10.0,
        }
    }
}

/// How far along a track a point is, from 0 at its start to 1 at its end. A
/// sideways track starts at the right in a right-to-left page, and an upright one
/// starts at the bottom.
pub(crate) fn fraction_along(track: Rect, at: Point, orientation: Orientation, dir: Dir) -> f64 {
    match orientation {
        Orientation::Horizontal => {
            let along = (at.x - track.x) / track.width.max(f64::MIN_POSITIVE);
            if dir == Dir::Rtl { 1.0 - along } else { along }
        }
        Orientation::Vertical => 1.0 - (at.y - track.y) / track.height.max(f64::MIN_POSITIVE),
    }
}

const ACTIONS: &[&str] = &["set", "increment", "decrement"];

/// A single-thumb slider.
#[derive(Clone, Debug)]
pub struct Slider {
    opts: Options,
    value: f64,
    dragging: bool,
}

impl Default for Slider {
    fn default() -> Self {
        Self::new(Options::default(), 0.0)
    }
}

impl Slider {
    #[must_use]
    pub fn new(opts: Options, value: f64) -> Self {
        Self {
            opts,
            value: opts.range.snap(value),
            dragging: false,
        }
    }

    #[must_use]
    pub const fn number(&self) -> f64 {
        self.value
    }

    /// How far along the track the thumb sits, from 0 to 1, for positioning it.
    #[must_use]
    pub fn fraction(&self) -> f64 {
        self.opts.range.fraction(self.value)
    }

    fn set(&mut self, value: f64, source: Source, reason: Reason, gate: Gate) -> bool {
        let value = self.opts.range.snap(value);
        if (value - self.value).abs() < f64::EPSILON {
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

    fn value_at(&self, pointer: &Pointer<Part>, measure: Measure<Part>) -> Option<f64> {
        let track = measure(Part::Track, None)?;
        let fraction = fraction_along(track, pointer.at, self.opts.orientation, self.opts.dir);
        Some(self.opts.range.at_fraction(fraction))
    }
}

impl PartOf for Part {
    type Primitive = Slider;
}

impl Primitive for Slider {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Track, Part::Thumb]
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

    fn semantics(&self, part: Part, _: Option<&str>) -> Semantics {
        match part {
            Part::Track => Semantics::default(),
            Part::Thumb => Semantics {
                role: Some(Role::Slider),
                orientation: Some(self.opts.orientation),
                range: Some(ValueRange {
                    now: self.value,
                    min: self.opts.range.min,
                    max: self.opts.range.max,
                }),
                focusable: Focusable::TabStop,
                active: self.dragging,
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        let Input::Key { key, mods, .. } = input else {
            return Reaction::default();
        };
        let forward = if self.opts.dir == Dir::Rtl { -1.0 } else { 1.0 };
        let steps = if mods.shift {
            self.opts.large_step
        } else {
            1.0
        };
        let range = self.opts.range;
        let target = match key {
            Key::ArrowRight => range.nudge(self.value, forward * steps),
            Key::ArrowLeft => range.nudge(self.value, -forward * steps),
            Key::ArrowUp => range.nudge(self.value, steps),
            Key::ArrowDown => range.nudge(self.value, -steps),
            Key::PageUp => range.nudge(self.value, self.opts.large_step),
            Key::PageDown => range.nudge(self.value, -self.opts.large_step),
            Key::Home => range.min,
            Key::End => range.max,
            _ => return Reaction::default(),
        };
        self.set(target, Source::User, Reason::Keyboard, gate);
        Reaction::handled()
    }

    fn pointer(
        &mut self,
        pointer: Pointer<Part>,
        measure: Measure<Part>,
        gate: Gate,
    ) -> Reaction<Part> {
        match pointer.phase {
            PointerPhase::Down => {
                self.dragging = true;
                if let Some(value) = self.value_at(&pointer, measure) {
                    self.set(value, Source::User, Reason::Drag, gate);
                }
                Reaction::focusing(Part::Thumb).with(Effect::CapturePointer)
            }
            PointerPhase::Move if self.dragging => {
                if let Some(value) = self.value_at(&pointer, measure) {
                    self.set(value, Source::User, Reason::Drag, gate);
                }
                Reaction::handled()
            }
            PointerPhase::Up | PointerPhase::Cancel => {
                self.dragging = false;
                Reaction::handled()
            }
            PointerPhase::Move => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        vec![self.value.to_string()]
    }

    fn set_value(&mut self, value: Vec<String>) {
        if let Some(number) = value.first().and_then(|text| read_number(text)) {
            self.value = self.opts.range.snap(number);
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "slider",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let target = match action {
            "increment" => self.opts.range.nudge(self.value, 1.0),
            "decrement" => self.opts.range.nudge(self.value, -1.0),
            "set" => {
                let text = item.ok_or(ActError::MissingItem { action: "set" })?;
                read_number(text).ok_or_else(|| ActError::UnknownItem(text.to_string()))?
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
    use crate::geometry::{Point, Rect};
    use crate::keys::Modifiers;

    const TRACK: Rect = Rect {
        x: 100.0,
        y: 0.0,
        width: 200.0,
        height: 20.0,
    };

    fn measure(part: Part, _: Option<&str>) -> Option<Rect> {
        (part == Part::Track).then_some(TRACK)
    }

    fn pointer(slider: &mut Slider, phase: PointerPhase, part: Part, x: f64) -> Reaction<Part> {
        let pointer = Pointer {
            phase,
            part,
            item: None,
            at: Point::new(x, 10.0),
            mods: Modifiers::default(),
            now_ms: 0,
        };
        slider.pointer(pointer, &measure, &mut |_| true)
    }

    fn key(slider: &mut Slider, key: Key, mods: Modifiers) {
        slider.send(
            Input::Key {
                key,
                mods,
                now_ms: 0,
            },
            &mut |_| true,
        );
    }

    #[test]
    fn pressing_the_track_moves_the_thumb_there_and_captures_the_pointer() {
        let mut slider = Slider::default();
        let pressed = pointer(&mut slider, PointerPhase::Down, Part::Track, 150.0);
        assert_eq!(slider.number(), 25.0);
        assert!(pressed.effects.contains(&Effect::CapturePointer));
        assert_eq!(pressed.focus.unwrap().part, Part::Thumb);
    }

    #[test]
    fn the_thumb_follows_a_drag_and_stops_at_the_ends() {
        let mut slider = Slider::default();
        pointer(&mut slider, PointerPhase::Down, Part::Thumb, 100.0);
        pointer(&mut slider, PointerPhase::Move, Part::Thumb, 201.0);
        assert_eq!(slider.number(), 51.0, "snapped to the step");
        pointer(&mut slider, PointerPhase::Move, Part::Thumb, 9000.0);
        assert_eq!(slider.number(), 100.0);
        pointer(&mut slider, PointerPhase::Up, Part::Thumb, 9000.0);
        pointer(&mut slider, PointerPhase::Move, Part::Thumb, 100.0);
        assert_eq!(slider.number(), 100.0, "a move after release is not a drag");
    }

    #[test]
    fn keys_step_jump_and_respect_direction() {
        let none = Modifiers::default();
        let shift = Modifiers {
            shift: true,
            ..none
        };
        let mut slider = Slider::new(Options::default(), 50.0);
        key(&mut slider, Key::ArrowRight, none);
        key(&mut slider, Key::ArrowUp, shift);
        assert_eq!(slider.number(), 61.0);
        key(&mut slider, Key::PageDown, none);
        assert_eq!(slider.number(), 51.0);
        key(&mut slider, Key::End, none);
        assert_eq!(slider.number(), 100.0);
        let mut rtl = Slider::new(
            Options {
                dir: Dir::Rtl,
                ..Default::default()
            },
            50.0,
        );
        key(&mut rtl, Key::ArrowRight, none);
        assert_eq!(rtl.number(), 49.0);
    }

    #[test]
    fn a_vertical_slider_grows_upwards() {
        let mut slider = Slider::new(
            Options {
                orientation: Orientation::Vertical,
                ..Default::default()
            },
            0.0,
        );
        let tall = |part: Part, _: Option<&str>| {
            (part == Part::Track).then_some(Rect::new(0.0, 0.0, 20.0, 200.0))
        };
        let press = Pointer {
            phase: PointerPhase::Down,
            part: Part::Track,
            item: None,
            at: Point::new(10.0, 50.0),
            mods: Modifiers::default(),
            now_ms: 0,
        };
        slider.pointer(press, &tall, &mut |_| true);
        assert_eq!(slider.number(), 75.0);
    }

    #[test]
    fn the_thumb_reports_its_value_and_limits() {
        let slider = Slider::new(Options::default(), 30.0);
        let thumb = slider.semantics(Part::Thumb, None);
        assert_eq!(
            thumb.range,
            Some(ValueRange {
                now: 30.0,
                min: 0.0,
                max: 100.0
            })
        );
        assert_eq!(slider.fraction(), 0.3);
    }

    #[test]
    fn a_refused_change_leaves_the_value_and_agents_can_set_it() {
        let mut slider = Slider::default();
        slider.send(
            Input::Key {
                key: Key::End,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| false,
        );
        assert_eq!(slider.number(), 0.0);
        slider.act("set", Some("42.4"), &mut |_| true).unwrap();
        assert_eq!(slider.value(), ["42"]);
        assert_eq!(
            slider.act("set", Some("lots"), &mut |_| true),
            Err(ActError::UnknownItem("lots".into()))
        );
    }
}
