use crate::agent::{ActError, Snapshot};
use crate::area::Area;
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::color::{Hsv, Rgb};
use crate::keys::{Dir, Key};
use crate::number::NumberRange;
use crate::primitive::{
    Effect, Focusable, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive, Reaction,
    Role, Semantics, ValueRange,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The square: saturation across, brightness up.
    Area,
    AreaThumb,
    /// The strip of hues.
    Hue,
    HueThumb,
    /// The colour as hex text.
    Input,
}

const ACTIONS: &[&str] = &["set"];

/// A colour picker: a saturation and brightness square, a hue strip and a hex field.
/// The value is the colour as `#rrggbb`.
#[derive(Clone, Debug)]
pub struct ColorPicker {
    /// Kept as hue, saturation and brightness so the hue survives passing through
    /// grey or black.
    color: Hsv,
    draft: Option<String>,
    focused: Option<Part>,
    dragging: Option<Part>,
}

impl Default for ColorPicker {
    fn default() -> Self {
        Self::new(Rgb::new(59, 111, 224))
    }
}

const fn pad() -> Area {
    Area {
        x: NumberRange::new(0.0, 100.0, 1.0),
        y: NumberRange::new(0.0, 100.0, 1.0),
    }
}

impl ColorPicker {
    #[must_use]
    pub fn new(color: Rgb) -> Self {
        Self {
            color: color.to_hsv(),
            draft: None,
            focused: None,
            dragging: None,
        }
    }

    #[must_use]
    pub fn color(&self) -> Rgb {
        self.color.to_rgb()
    }

    /// The pure hue behind the square, for painting it.
    #[must_use]
    pub fn hue_color(&self) -> Rgb {
        Hsv {
            h: self.color.h,
            s: 1.0,
            v: 1.0,
        }
        .to_rgb()
    }

    /// Where the square's thumb sits, as fractions from its left and top edges.
    #[must_use]
    pub fn area_position(&self) -> (f64, f64) {
        (self.color.s, 1.0 - self.color.v)
    }

    /// Where the hue thumb sits along its strip, from 0 to 1.
    #[must_use]
    pub fn hue_position(&self) -> f64 {
        self.color.h / 360.0
    }

    fn set(&mut self, color: Hsv, source: Source, reason: Reason, gate: Gate) -> bool {
        if color.to_rgb() == self.color.to_rgb() && (color.h - self.color.h).abs() < f64::EPSILON {
            self.color = color;
            return true;
        }
        let allowed = gate(&ValueChange {
            value: vec![color.to_rgb().to_hex()],
            source,
            reason,
        });
        if allowed {
            self.color = color;
            self.draft = None;
        }
        allowed
    }

    fn drag(&mut self, part: Part, pointer: &Pointer<Part>, measure: Measure<Part>, gate: Gate) {
        let color = match part {
            Part::Area | Part::AreaThumb => measure(Part::Area, None).map(|bounds| {
                let (s, v) = pad().value_at(bounds, pointer.at, Dir::Ltr);
                Hsv {
                    s: s / 100.0,
                    v: v / 100.0,
                    ..self.color
                }
            }),
            _ => measure(Part::Hue, None).map(|bounds| {
                let along = ((pointer.at.x - bounds.x) / bounds.width.max(f64::MIN_POSITIVE))
                    .clamp(0.0, 1.0);
                Hsv {
                    h: (along * 360.0).round().min(359.0),
                    ..self.color
                }
            }),
        };
        if let Some(color) = color {
            self.set(color, Source::User, Reason::Drag, gate);
        }
    }
}

impl PartOf for Part {
    type Primitive = ColorPicker;
}

impl Primitive for ColorPicker {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[
            Part::Area,
            Part::AreaThumb,
            Part::Hue,
            Part::HueThumb,
            Part::Input,
        ]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn tracks_pointer(part: Part) -> bool {
        part != Part::Input
    }

    /// The hex field is what names the colour.
    fn named_part(&self) -> Option<Part> {
        Some(Part::Input)
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, _: Option<&str>) -> Semantics {
        match part {
            Part::Area | Part::Hue => Semantics::default(),
            Part::AreaThumb => Semantics {
                role: Some(Role::Slider),
                range: Some(ValueRange {
                    now: (self.color.s * 100.0).round(),
                    min: 0.0,
                    max: 100.0,
                }),
                focusable: Focusable::TabStop,
                active: matches!(self.dragging, Some(Part::Area | Part::AreaThumb)),
                ..Default::default()
            },
            Part::HueThumb => Semantics {
                role: Some(Role::Slider),
                range: Some(ValueRange {
                    now: self.color.h.round(),
                    min: 0.0,
                    max: 359.0,
                }),
                focusable: Focusable::TabStop,
                active: matches!(self.dragging, Some(Part::Hue | Part::HueThumb)),
                ..Default::default()
            },
            Part::Input => Semantics {
                role: Some(Role::TextBox),
                focusable: Focusable::TabStop,
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
            Input::Text { value, .. } => {
                self.draft = Some(value);
                Reaction::handled()
            }
            Input::Key {
                key: Key::Enter, ..
            } if self.focused == Some(Part::Input) => {
                if let Some(color) = self.draft.take().and_then(|text| Rgb::from_hex(&text)) {
                    self.set(color.to_hsv(), Source::User, Reason::Text, gate);
                }
                Reaction::handled()
            }
            Input::Key { key, mods, .. } if self.focused == Some(Part::AreaThumb) => {
                let now = (self.color.s * 100.0, self.color.v * 100.0);
                let Some((s, v)) = pad().nudge(now, key, mods, Dir::Ltr) else {
                    return Reaction::default();
                };
                self.set(
                    Hsv {
                        s: s / 100.0,
                        v: v / 100.0,
                        ..self.color
                    },
                    Source::User,
                    Reason::Keyboard,
                    gate,
                );
                Reaction::handled()
            }
            Input::Key { key, mods, .. } if self.focused == Some(Part::HueThumb) => {
                let step = if mods.shift { 10.0 } else { 1.0 };
                let hue = match key {
                    Key::ArrowRight | Key::ArrowUp => self.color.h + step,
                    Key::ArrowLeft | Key::ArrowDown => self.color.h - step,
                    Key::Home => 0.0,
                    Key::End => 359.0,
                    _ => return Reaction::default(),
                };
                self.set(
                    Hsv {
                        h: hue.clamp(0.0, 359.0),
                        ..self.color
                    },
                    Source::User,
                    Reason::Keyboard,
                    gate,
                );
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
                self.dragging = Some(pointer.part);
                self.drag(pointer.part, &pointer, measure, gate);
                let thumb = if matches!(pointer.part, Part::Area | Part::AreaThumb) {
                    Part::AreaThumb
                } else {
                    Part::HueThumb
                };
                self.focused = Some(thumb);
                Reaction::focusing(thumb).with(Effect::CapturePointer)
            }
            PointerPhase::Move => self.dragging.map_or_else(Reaction::default, |part| {
                self.drag(part, &pointer, measure, gate);
                Reaction::handled()
            }),
            PointerPhase::Up | PointerPhase::Cancel => {
                self.dragging = None;
                Reaction::handled()
            }
        }
    }

    fn text(&self, part: Part, _: Option<&str>) -> Option<String> {
        (part == Part::Input).then(|| self.draft.clone().unwrap_or_else(|| self.color().to_hex()))
    }

    fn value(&self) -> Vec<String> {
        vec![self.color().to_hex()]
    }

    fn set_value(&mut self, value: Vec<String>) {
        if let Some(color) = value.first().and_then(|text| Rgb::from_hex(text)) {
            self.color = color.to_hsv();
            self.draft = None;
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "color",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        if action != "set" {
            return Err(ActError::UnknownAction {
                action: action.to_string(),
                valid: ACTIONS,
            });
        }
        let text = item.ok_or(ActError::MissingItem { action: "set" })?;
        let color = Rgb::from_hex(text).ok_or_else(|| ActError::UnknownItem(text.to_string()))?;
        if self.set(color.to_hsv(), Source::Agent, Reason::Action, gate) {
            Ok(())
        } else {
            Err(ActError::Refused)
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::too_many_lines,
    reason = "a test fails by panicking, and its arithmetic is on values it wrote itself"
)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Rect};
    use crate::keys::Modifiers;

    fn boxes(part: Part, _: Option<&str>) -> Option<Rect> {
        match part {
            Part::Area => Some(Rect::new(0.0, 0.0, 200.0, 100.0)),
            Part::Hue => Some(Rect::new(0.0, 120.0, 360.0, 10.0)),
            _ => None,
        }
    }

    fn pointer(
        picker: &mut ColorPicker,
        phase: PointerPhase,
        part: Part,
        x: f64,
        y: f64,
    ) -> Reaction<Part> {
        let pointer = Pointer {
            phase,
            part,
            item: None,
            at: Point::new(x, y),
            mods: Modifiers::default(),
            now_ms: 0,
        };
        picker.pointer(pointer, &boxes, &mut |_| true)
    }

    #[test]
    fn the_square_sets_saturation_across_and_brightness_up() {
        let mut picker = ColorPicker::new(Rgb::new(255, 0, 0));
        pointer(&mut picker, PointerPhase::Down, Part::Area, 200.0, 0.0);
        assert_eq!(picker.value(), ["#ff0000"]);
        pointer(&mut picker, PointerPhase::Move, Part::Area, 0.0, 0.0);
        assert_eq!(picker.value(), ["#ffffff"]);
        pointer(&mut picker, PointerPhase::Move, Part::Area, 100.0, 100.0);
        assert_eq!(picker.value(), ["#000000"]);
    }

    #[test]
    fn the_hue_survives_a_trip_through_black() {
        let mut picker = ColorPicker::new(Rgb::new(0, 0, 255));
        pointer(&mut picker, PointerPhase::Down, Part::Area, 200.0, 100.0);
        assert_eq!(picker.value(), ["#000000"]);
        pointer(&mut picker, PointerPhase::Move, Part::Area, 200.0, 0.0);
        assert_eq!(picker.value(), ["#0000ff"], "still blue, not red");
    }

    #[test]
    fn the_strip_sets_the_hue_and_focuses_its_thumb() {
        let mut picker = ColorPicker::new(Rgb::new(255, 0, 0));
        let pressed = pointer(&mut picker, PointerPhase::Down, Part::Hue, 120.0, 125.0);
        assert_eq!(picker.value(), ["#00ff00"]);
        assert_eq!(pressed.focus.unwrap().part, Part::HueThumb);
        assert_eq!(picker.hue_position(), 120.0 / 360.0);
    }

    #[test]
    fn arrows_adjust_whichever_thumb_has_focus() {
        let mut picker = ColorPicker::new(Rgb::new(255, 0, 0));
        let key = |picker: &mut ColorPicker, key| {
            picker.send(
                Input::Key {
                    key,
                    mods: Modifiers::default(),
                    now_ms: 0,
                },
                &mut |_| true,
            )
        };
        picker.send(
            Input::Focus {
                part: Part::HueThumb,
                item: None,
            },
            &mut |_| true,
        );
        key(&mut picker, Key::ArrowRight);
        assert_eq!(
            picker.semantics(Part::HueThumb, None).range.unwrap().now,
            1.0
        );
        picker.send(
            Input::Focus {
                part: Part::AreaThumb,
                item: None,
            },
            &mut |_| true,
        );
        key(&mut picker, Key::ArrowLeft);
        assert_eq!(
            picker.semantics(Part::AreaThumb, None).range.unwrap().now,
            99.0
        );
    }

    #[test]
    fn hex_text_is_committed_on_enter_and_nonsense_is_dropped() {
        let mut picker = ColorPicker::default();
        picker.send(
            Input::Focus {
                part: Part::Input,
                item: None,
            },
            &mut |_| true,
        );
        let typed = |picker: &mut ColorPicker, text: &str| {
            picker.send(
                Input::Text {
                    part: Part::Input,
                    item: None,
                    value: text.into(),
                },
                &mut |_| true,
            );
            picker.send(
                Input::Key {
                    key: Key::Enter,
                    mods: Modifiers::default(),
                    now_ms: 0,
                },
                &mut |_| true,
            );
        };
        typed(&mut picker, "#f80");
        assert_eq!(picker.value(), ["#ff8800"]);
        typed(&mut picker, "orange");
        assert_eq!(picker.text(Part::Input, None).as_deref(), Some("#ff8800"));
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(ColorPicker::default, &[]);
    }
}
