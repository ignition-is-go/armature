use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::{Dir, Key};
use crate::number::read_number;
use crate::primitive::{
    Effect, Focusable, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive, Reaction,
    Role, Semantics, ValueRange,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    /// One star, keyed by its number from 1. Fill it by `fill`.
    Star,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// How many stars there are.
    pub max: u32,
    /// Whether half a star can be given.
    pub halves: bool,
    /// Whether pressing the rating already given clears it.
    pub deselectable: bool,
    pub dir: Dir,
    /// Whether it only shows a score. It is then a picture, and takes no input.
    pub read_only: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max: 5,
            halves: false,
            deselectable: true,
            dir: Dir::Ltr,
            read_only: false,
        }
    }
}

const ACTIONS: &[&str] = &["set", "clear"];

/// A rating in stars, whole or half. Pressing the left half of a star gives half
/// of it. The value is the number of stars, or empty when none is given.
#[derive(Clone, Debug, Default)]
pub struct Rating {
    opts: Options,
    value: f64,
    /// What the pointer would give if pressed, for showing a preview.
    hover: Option<f64>,
}

impl Rating {
    #[must_use]
    pub const fn new(opts: Options) -> Self {
        Self {
            opts,
            value: 0.0,
            hover: None,
        }
    }

    const fn step(&self) -> f64 {
        if self.opts.halves { 0.5 } else { 1.0 }
    }

    /// How much of a star to fill: 0, a half or 1. It follows the pointer's preview
    /// while there is one.
    #[must_use]
    pub fn fill(&self, star: u32) -> f64 {
        let shown = self.hover.unwrap_or(self.value);
        (shown - (f64::from(star) - 1.0)).clamp(0.0, 1.0)
    }

    /// A rating on the nearest whole or half star, from none to the most.
    fn snapped(&self, value: f64) -> f64 {
        ((value / self.step()).round() * self.step()).clamp(0.0, f64::from(self.opts.max))
    }

    fn written(value: f64) -> Vec<String> {
        if value > 0.0 {
            vec![value.to_string()]
        } else {
            Vec::new()
        }
    }

    fn set(&mut self, value: f64, source: Source, reason: Reason, gate: Gate) -> bool {
        let value = self.snapped(value);
        if (value - self.value).abs() < f64::EPSILON {
            return true;
        }
        let allowed = gate(&ValueChange {
            value: Self::written(value),
            source,
            reason,
        });
        if allowed {
            self.value = value;
        }
        allowed
    }

    /// What a pointer over a star would give: the star, or half of it on its first half.
    fn at(&self, pointer: &Pointer<Part>, measure: Measure<Part>) -> Option<f64> {
        let star = read_number(pointer.item.as_deref()?)?;
        let rect = measure(Part::Star, pointer.item.as_deref())?;
        let along = (pointer.at.x - rect.x) / rect.width.max(1.0);
        let first_half = if self.opts.dir == Dir::Rtl {
            along > 0.5
        } else {
            along < 0.5
        };
        Some(if self.opts.halves && first_half {
            star - 0.5
        } else {
            star
        })
    }
}

impl PartOf for Part {
    type Primitive = Rating;
}

impl Primitive for Rating {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Star]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn tracks_pointer(part: Part) -> bool {
        part == Part::Star
    }

    fn hovers(part: Part) -> bool {
        part == Part::Root
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match part {
            // One control with a number, so a half is as easy to reach as a whole.
            Part::Root if self.opts.read_only => Semantics {
                role: Some(Role::Image),
                ..Default::default()
            },
            Part::Root => Semantics {
                role: Some(Role::Slider),
                range: Some(ValueRange {
                    now: self.value,
                    min: 0.0,
                    max: f64::from(self.opts.max),
                }),
                focusable: Focusable::TabStop,
                ..Default::default()
            },
            Part::Star => {
                let star: Option<u32> = item.and_then(|key| key.parse().ok());
                Semantics {
                    hidden: item.is_some()
                        && !star.is_some_and(|star| (1..=self.opts.max).contains(&star)),
                    active: star.is_some_and(|star| self.fill(star) > 0.0),
                    ..Default::default()
                }
            }
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        if self.opts.read_only {
            return Reaction::default();
        }
        match input {
            Input::Hover { inside: false, .. } => {
                self.hover = None;
                Reaction::default()
            }
            Input::Key { key, mods, .. } if !mods.ctrl && !mods.alt => {
                let forward = if self.opts.dir == Dir::Rtl { -1.0 } else { 1.0 };
                let next = match key {
                    Key::ArrowUp => self.value + self.step(),
                    Key::ArrowDown => self.value - self.step(),
                    Key::ArrowRight => self.step().mul_add(forward, self.value),
                    Key::ArrowLeft => self.step().mul_add(-forward, self.value),
                    Key::Home => 0.0,
                    Key::End => f64::from(self.opts.max),
                    Key::Char(digit) if digit.is_ascii_digit() => {
                        f64::from(digit.to_digit(10).unwrap_or(0))
                    }
                    _ => return Reaction::default(),
                };
                self.set(next, Source::User, Reason::Keyboard, gate);
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
        if self.opts.read_only {
            return Reaction::default();
        }
        let Some(given) = self.at(&pointer, measure) else {
            return Reaction::default();
        };
        match pointer.phase {
            PointerPhase::Move => {
                self.hover = Some(given);
                Reaction::default()
            }
            PointerPhase::Down => {
                let next = if self.opts.deselectable && (given - self.value).abs() < f64::EPSILON {
                    0.0
                } else {
                    given
                };
                self.set(next, Source::User, Reason::Press, gate);
                self.hover = None;
                Reaction::focusing(Part::Root).with(Effect::CapturePointer)
            }
            PointerPhase::Up | PointerPhase::Cancel => Reaction::handled(),
        }
    }

    fn value(&self) -> Vec<String> {
        Self::written(self.value)
    }

    fn set_value(&mut self, value: Vec<String>) {
        let number = value
            .first()
            .and_then(|text| read_number(text))
            .unwrap_or(0.0);
        self.value = self.snapped(number);
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "rating",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let value = match action {
            "clear" => 0.0,
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
        if self.set(value, Source::Agent, Reason::Action, gate) {
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

    fn press(rating: &mut Rating, star: &str, x: f64) {
        let stars = |_: Part, item: Option<&str>| {
            let at: f64 = item?.parse().ok()?;
            Some(Rect::new((at - 1.0) * 20.0, 0.0, 20.0, 20.0))
        };
        let pointer = Pointer {
            phase: PointerPhase::Down,
            part: Part::Star,
            item: Some(star.into()),
            at: Point::new(x, 5.0),
            mods: Modifiers::default(),
            now_ms: 0,
        };
        rating.pointer(pointer, &stars, &mut |_| true);
    }

    #[test]
    fn the_first_half_of_a_star_gives_half_of_it() {
        let mut rating = Rating::new(Options {
            halves: true,
            ..Default::default()
        });
        press(&mut rating, "3", 44.0);
        assert_eq!(rating.value(), ["2.5"]);
        assert_eq!(
            [rating.fill(2), rating.fill(3), rating.fill(4)],
            [1.0, 0.5, 0.0]
        );
        press(&mut rating, "3", 56.0);
        assert_eq!(rating.value(), ["3"]);
        press(&mut rating, "3", 56.0);
        assert!(
            rating.value().is_empty(),
            "pressing what was given clears it"
        );
    }

    #[test]
    fn whole_stars_only_unless_halves_are_allowed_and_keys_step() {
        let mut rating = Rating::default();
        press(&mut rating, "2", 22.0);
        assert_eq!(rating.value(), ["2"]);
        let key = |rating: &mut Rating, key| {
            rating.send(
                Input::Key {
                    key,
                    mods: Modifiers::default(),
                    now_ms: 0,
                },
                &mut |_| true,
            )
        };
        key(&mut rating, Key::ArrowRight);
        key(&mut rating, Key::End);
        assert_eq!(rating.value(), ["5"]);
        key(&mut rating, Key::Char('4'));
        assert_eq!(rating.semantics(Part::Root, None).range.unwrap().now, 4.0);
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(Rating::default, &[]);
        crate::testing::assert_conforms(
            || {
                Rating::new(Options {
                    halves: true,
                    ..Default::default()
                })
            },
            &[],
        );
    }
}
