use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::{Dir, Key, Orientation};
use crate::number::NumberRange;
use crate::primitive::{
    Effect, Focusable, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive, Reaction,
    Role, Semantics, ValueRange,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Track,
    /// One grip, keyed by its place among the grips: `0`, `1` and so on.
    Thumb,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub range: NumberRange,
    pub orientation: Orientation,
    pub dir: Dir,
    /// How far `PageUp`, `PageDown` and Shift with an arrow move a grip.
    pub large_step: f64,
    /// The fewest steps two neighbouring grips may sit apart.
    pub min_steps_between: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            range: NumberRange::new(0.0, 100.0, 1.0),
            orientation: Orientation::Horizontal,
            dir: Dir::Ltr,
            large_step: 10.0,
            min_steps_between: 0.0,
        }
    }
}

const ACTIONS: &[&str] = &["set"];

/// A slider with several grips, as for a price range. A grip cannot pass its
/// neighbours. The value is each grip's number, lowest first.
#[derive(Clone, Debug)]
pub struct RangeSlider {
    opts: Options,
    values: Vec<f64>,
    /// The grip being dragged.
    dragging: Option<usize>,
    focused: usize,
}

impl Default for RangeSlider {
    fn default() -> Self {
        Self::new(Options::default(), &[25.0, 75.0])
    }
}

impl RangeSlider {
    pub fn new(opts: Options, values: &[f64]) -> Self {
        let mut values: Vec<f64> = values.iter().map(|value| opts.range.snap(*value)).collect();
        values.sort_by(f64::total_cmp);
        Self {
            opts,
            values,
            dragging: None,
            focused: 0,
        }
    }

    #[must_use]
    pub fn values(&self) -> &[f64] {
        &self.values
    }

    /// Where each grip sits along the track, from 0 to 1.
    #[must_use]
    pub fn fractions(&self) -> Vec<f64> {
        self.values
            .iter()
            .map(|value| self.opts.range.fraction(*value))
            .collect()
    }

    /// The least and most a grip may hold, given its neighbours.
    fn limits(&self, at: usize) -> (f64, f64) {
        let gap = self.opts.min_steps_between * self.opts.range.step;
        let low = at
            .checked_sub(1)
            .map_or(self.opts.range.min, |before| self.values[before] + gap);
        let high = self
            .values
            .get(at.saturating_add(1))
            .map_or(self.opts.range.max, |after| after - gap);
        (low, high.max(low))
    }

    fn written(values: &[f64]) -> Vec<String> {
        values.iter().map(f64::to_string).collect()
    }

    fn set(&mut self, at: usize, value: f64, source: Source, reason: Reason, gate: Gate) -> bool {
        if at >= self.values.len() {
            return false;
        }
        let (low, high) = self.limits(at);
        let value = self.opts.range.snap(value).clamp(low, high);
        if value == self.values[at] {
            return true;
        }
        let mut next = self.values.clone();
        next[at] = value;
        let allowed = gate(&ValueChange {
            value: Self::written(&next),
            source,
            reason,
        });
        if allowed {
            self.values = next;
        }
        allowed
    }

    fn value_at(&self, pointer: &Pointer<Part>, measure: Measure<Part>) -> Option<f64> {
        let track = measure(Part::Track, None)?;
        let fraction = match self.opts.orientation {
            Orientation::Horizontal => {
                let along = (pointer.at.x - track.x) / track.width.max(f64::MIN_POSITIVE);
                if self.opts.dir == Dir::Rtl {
                    1.0 - along
                } else {
                    along
                }
            }
            Orientation::Vertical => {
                1.0 - (pointer.at.y - track.y) / track.height.max(f64::MIN_POSITIVE)
            }
        };
        Some(self.opts.range.at_fraction(fraction))
    }

    fn index(item: Option<&str>) -> Option<usize> {
        item?.parse().ok()
    }
}

impl PartOf for Part {
    type Primitive = RangeSlider;
}

impl Primitive for RangeSlider {
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

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match (part, Self::index(item).filter(|at| *at < self.values.len())) {
            (Part::Track, _) => Semantics {
                role: Some(Role::Group),
                ..Default::default()
            },
            (Part::Thumb, Some(at)) => {
                let (min, max) = self.limits(at);
                Semantics {
                    role: Some(Role::Slider),
                    range: Some(ValueRange {
                        now: self.values[at],
                        min,
                        max,
                    }),
                    orientation: Some(self.opts.orientation),
                    // Each grip is a slider of its own, so each is a Tab stop.
                    focusable: Focusable::TabStop,
                    active: self.dragging == Some(at),
                    ..Default::default()
                }
            }
            (Part::Thumb, None) => Semantics {
                role: Some(Role::Slider),
                hidden: item.is_some(),
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Focus {
                part: Part::Thumb,
                item,
            } => {
                self.focused = Self::index(item.as_deref()).unwrap_or(self.focused);
                Reaction::default()
            }
            Input::Key { key, mods, .. } if !mods.ctrl && !mods.alt => {
                let Some(now) = self.values.get(self.focused).copied() else {
                    return Reaction::default();
                };
                let step = if mods.shift {
                    self.opts.large_step
                } else {
                    self.opts.range.step
                };
                let forward = if self.opts.dir == Dir::Rtl { -1.0 } else { 1.0 };
                let next = match key {
                    Key::ArrowUp => now + step,
                    Key::ArrowDown => now - step,
                    Key::ArrowRight => f64::mul_add(step, forward, now),
                    Key::ArrowLeft => f64::mul_add(step, -forward, now),
                    Key::PageUp => now + self.opts.large_step,
                    Key::PageDown => now - self.opts.large_step,
                    // As far as the neighbours allow.
                    Key::Home => self.limits(self.focused).0,
                    Key::End => self.limits(self.focused).1,
                    _ => return Reaction::default(),
                };
                self.set(self.focused, next, Source::User, Reason::Keyboard, gate);
                Reaction::handled()
            }
            _ => Reaction::default(),
        }
    }

    fn pointer(
        &mut self,
        pointer: Pointer<Part>,
        measure: Measure<Part>,
        gate: Gate,
    ) -> Reaction<Part> {
        match pointer.phase {
            PointerPhase::Down => {
                let Some(value) = self.value_at(&pointer, measure) else {
                    return Reaction::default();
                };
                // A press on a grip takes that grip. A press on the track takes the nearest.
                let pressed = if pointer.part == Part::Thumb {
                    Self::index(pointer.item.as_deref())
                } else {
                    None
                };
                // Where grips sit on top of one another, a press beyond them takes the
                // last of the stack and a press before them the first.
                let distance = |at: usize| (self.values[at] - value).abs();
                let nearest =
                    (0..self.values.len()).min_by(|a, b| distance(*a).total_cmp(&distance(*b)));
                let nearest = nearest.map(|first| {
                    let stack = (first..self.values.len())
                        .take_while(|at| self.values[*at] == self.values[first])
                        .last()
                        .unwrap_or(first);
                    if value > self.values[first] {
                        stack
                    } else {
                        first
                    }
                });
                let Some(at) = pressed.or(nearest) else {
                    return Reaction::default();
                };
                self.dragging = Some(at);
                self.focused = at;
                if pointer.part == Part::Track {
                    self.set(at, value, Source::User, Reason::Drag, gate);
                }
                Reaction::item(Part::Thumb, true, Some(at.to_string())).with(Effect::CapturePointer)
            }
            PointerPhase::Move => {
                let (Some(at), Some(value)) = (self.dragging, self.value_at(&pointer, measure))
                else {
                    return Reaction::default();
                };
                self.set(at, value, Source::User, Reason::Drag, gate);
                Reaction::handled()
            }
            PointerPhase::Up | PointerPhase::Cancel => {
                self.dragging = None;
                Reaction::handled()
            }
        }
    }

    fn value(&self) -> Vec<String> {
        Self::written(&self.values)
    }

    fn set_value(&mut self, value: Vec<String>) {
        let mut numbers: Vec<f64> = value
            .iter()
            .filter_map(|text| text.parse().ok())
            .map(|number: f64| self.opts.range.snap(number))
            .collect();
        if numbers.len() == self.values.len() {
            numbers.sort_by(f64::total_cmp);
            self.values = numbers;
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "range-slider",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    /// `set` takes the grip and its number as `index=number`.
    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        if action != "set" {
            return Err(ActError::UnknownAction {
                action: action.to_string(),
                valid: ACTIONS,
            });
        }
        let given = item.ok_or(ActError::MissingItem { action: "set" })?;
        let parsed = given.split_once('=').and_then(|(at, number)| {
            Some((at.parse::<usize>().ok()?, number.parse::<f64>().ok()?))
        });
        let (at, number) = parsed
            .filter(|(at, _)| *at < self.values.len())
            .ok_or_else(|| ActError::UnknownItem(given.to_string()))?;
        if self.set(at, number, Source::Agent, Reason::Action, gate) {
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

    fn pointer(
        slider: &mut RangeSlider,
        phase: PointerPhase,
        part: Part,
        item: Option<&str>,
        x: f64,
    ) {
        let pointer = Pointer {
            phase,
            part,
            item: item.map(str::to_string),
            at: Point::new(x, 5.0),
            mods: Modifiers::default(),
            now_ms: 0,
        };
        slider.pointer(
            pointer,
            &|_, _| Some(Rect::new(0.0, 0.0, 200.0, 10.0)),
            &mut |_| true,
        );
    }

    #[test]
    fn a_press_on_the_track_moves_the_nearest_grip() {
        let mut slider = RangeSlider::default();
        pointer(&mut slider, PointerPhase::Down, Part::Track, None, 20.0);
        assert_eq!(slider.value(), ["10", "75"]);
        pointer(&mut slider, PointerPhase::Up, Part::Track, None, 20.0);
        pointer(&mut slider, PointerPhase::Down, Part::Track, None, 180.0);
        assert_eq!(slider.value(), ["10", "90"]);
        assert_eq!(slider.fractions(), [0.1, 0.9]);
    }

    #[test]
    fn a_grip_stops_at_its_neighbour_and_keeps_its_distance() {
        let mut slider = RangeSlider::new(
            Options {
                min_steps_between: 5.0,
                ..Default::default()
            },
            &[25.0, 75.0],
        );
        pointer(
            &mut slider,
            PointerPhase::Down,
            Part::Thumb,
            Some("0"),
            50.0,
        );
        pointer(
            &mut slider,
            PointerPhase::Move,
            Part::Thumb,
            Some("0"),
            190.0,
        );
        assert_eq!(
            slider.value(),
            ["70", "75"],
            "five steps short of the other grip"
        );
        assert_eq!(
            slider.semantics(Part::Thumb, Some("1")).range.unwrap().min,
            75.0
        );
    }

    #[test]
    fn keys_move_the_focused_grip_and_end_goes_as_far_as_it_may() {
        let mut slider = RangeSlider::default();
        let key = |slider: &mut RangeSlider, key, shift| {
            slider.send(
                Input::Key {
                    key,
                    mods: Modifiers {
                        shift,
                        ..Default::default()
                    },
                    now_ms: 0,
                },
                &mut |_| true,
            );
        };
        key(&mut slider, Key::ArrowRight, false);
        key(&mut slider, Key::ArrowRight, true);
        assert_eq!(slider.value(), ["36", "75"]);
        key(&mut slider, Key::End, false);
        assert_eq!(slider.value(), ["75", "75"]);
        slider.send(
            Input::Focus {
                part: Part::Thumb,
                item: Some("1".into()),
            },
            &mut |_| true,
        );
        key(&mut slider, Key::PageUp, false);
        assert_eq!(slider.value(), ["75", "85"]);
        slider.act("set", Some("1=200"), &mut |_| true).unwrap();
        assert_eq!(slider.value(), ["75", "100"]);
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(RangeSlider::default, &[]);
    }
}
