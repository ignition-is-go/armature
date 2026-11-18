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
    /// The box holding both panels.
    Root,
    /// The divider that is dragged.
    Handle,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Which way the panels sit side by side.
    pub orientation: Orientation,
    pub dir: Dir,
    /// The smallest and largest share of the box the first panel may take, in percent.
    pub min: f64,
    pub max: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            orientation: Orientation::Horizontal,
            dir: Dir::Ltr,
            min: 10.0,
            max: 90.0,
        }
    }
}

const ACTIONS: &[&str] = &["set"];

/// Two panels with a divider between them. The value is the first panel's share of
/// the box, in percent.
#[derive(Clone, Debug)]
pub struct Resizable {
    opts: Options,
    percent: f64,
    dragging: bool,
}

impl Default for Resizable {
    fn default() -> Self {
        Self::new(Options::default(), 50.0)
    }
}

impl Resizable {
    #[must_use]
    pub const fn new(opts: Options, percent: f64) -> Self {
        Self {
            opts,
            percent: percent.clamp(opts.min, opts.max),
            dragging: false,
        }
    }

    /// The first panel's share of the box, in percent.
    #[must_use]
    pub const fn percent(&self) -> f64 {
        self.percent
    }

    const fn range(&self) -> NumberRange {
        NumberRange::new(self.opts.min, self.opts.max, 1.0)
    }

    fn set(&mut self, percent: f64, source: Source, reason: Reason, gate: Gate) -> bool {
        let percent = percent.clamp(self.opts.min, self.opts.max);
        if (percent - self.percent).abs() < f64::EPSILON {
            return true;
        }
        let allowed = gate(&ValueChange {
            value: vec![percent.to_string()],
            source,
            reason,
        });
        if allowed {
            self.percent = percent;
        }
        allowed
    }

    fn percent_at(&self, pointer: &Pointer<Part>, measure: Measure<Part>) -> Option<f64> {
        let bounds = measure(Part::Root, None)?;
        let share = match self.opts.orientation {
            Orientation::Horizontal => {
                let across = (pointer.at.x - bounds.x) / bounds.width.max(f64::MIN_POSITIVE);
                if self.opts.dir == Dir::Rtl {
                    1.0 - across
                } else {
                    across
                }
            }
            Orientation::Vertical => {
                (pointer.at.y - bounds.y) / bounds.height.max(f64::MIN_POSITIVE)
            }
        };
        Some((share * 1000.0).round() / 10.0)
    }
}

impl PartOf for Part {
    type Primitive = Resizable;
}

impl Primitive for Resizable {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Handle]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn tracks_pointer(part: Part) -> bool {
        part == Part::Handle
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, _: Option<&str>) -> Semantics {
        match part {
            Part::Root => Semantics::default(),
            // A divider between side-by-side panels is itself vertical, and the reverse.
            Part::Handle => Semantics {
                role: Some(Role::Separator),
                orientation: Some(match self.opts.orientation {
                    Orientation::Horizontal => Orientation::Vertical,
                    Orientation::Vertical => Orientation::Horizontal,
                }),
                range: Some(ValueRange {
                    now: self.percent,
                    min: self.opts.min,
                    max: self.opts.max,
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
        let steps = if mods.shift { 10.0 } else { 1.0 };
        let forward = if self.opts.dir == Dir::Rtl {
            -steps
        } else {
            steps
        };
        let target = match (key, self.opts.orientation) {
            (Key::ArrowRight, Orientation::Horizontal) => self.percent + forward,
            (Key::ArrowLeft, Orientation::Horizontal) => self.percent - forward,
            (Key::ArrowDown, Orientation::Vertical) => self.percent + steps,
            (Key::ArrowUp, Orientation::Vertical) => self.percent - steps,
            (Key::Home, _) => self.opts.min,
            (Key::End, _) => self.opts.max,
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
                Reaction::focusing(Part::Handle).with(Effect::CapturePointer)
            }
            PointerPhase::Move if self.dragging => {
                if let Some(percent) = self.percent_at(&pointer, measure) {
                    self.set(percent, Source::User, Reason::Drag, gate);
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
        vec![self.percent.to_string()]
    }

    fn set_value(&mut self, value: Vec<String>) {
        if let Some(percent) = value.first().and_then(|text| text.parse::<f64>().ok()) {
            self.percent = self.range().clamp(percent);
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "resizable",
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
        let percent = text
            .parse::<f64>()
            .map_err(|_| ActError::UnknownItem(text.to_string()))?;
        if self.set(percent, Source::Agent, Reason::Action, gate) {
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

    fn bounds(part: Part, _: Option<&str>) -> Option<Rect> {
        (part == Part::Root).then_some(Rect::new(100.0, 0.0, 400.0, 300.0))
    }

    fn pointer(panels: &mut Resizable, phase: PointerPhase, x: f64) {
        let pointer = Pointer {
            phase,
            part: Part::Handle,
            item: None,
            at: Point::new(x, 10.0),
            mods: Modifiers::default(),
            now_ms: 0,
        };
        panels.pointer(pointer, &bounds, &mut |_| true);
    }

    #[test]
    fn dragging_the_divider_sets_the_first_panels_share() {
        let mut panels = Resizable::default();
        pointer(&mut panels, PointerPhase::Down, 300.0);
        pointer(&mut panels, PointerPhase::Move, 200.0);
        assert_eq!(panels.percent(), 25.0);
        pointer(&mut panels, PointerPhase::Move, 9000.0);
        assert_eq!(panels.percent(), 90.0, "held at the maximum");
        pointer(&mut panels, PointerPhase::Up, 9000.0);
        pointer(&mut panels, PointerPhase::Move, 200.0);
        assert_eq!(panels.percent(), 90.0);
    }

    #[test]
    fn arrows_move_the_divider_and_home_and_end_go_to_the_limits() {
        let none = Modifiers::default();
        let key = |panels: &mut Resizable, key, mods| {
            panels.send(
                Input::Key {
                    key,
                    mods,
                    now_ms: 0,
                },
                &mut |_| true,
            )
        };
        let mut panels = Resizable::default();
        key(&mut panels, Key::ArrowRight, none);
        key(
            &mut panels,
            Key::ArrowLeft,
            Modifiers {
                shift: true,
                ..none
            },
        );
        assert_eq!(panels.percent(), 41.0);
        key(&mut panels, Key::Home, none);
        assert_eq!(panels.percent(), 10.0);
        assert!(
            !key(&mut panels, Key::ArrowDown, none).handled,
            "the other axis is left alone"
        );
    }

    #[test]
    fn the_divider_reports_its_position_and_runs_across_the_panels() {
        let panels = Resizable::default();
        let handle = panels.semantics(Part::Handle, None);
        assert_eq!(
            handle.range,
            Some(ValueRange {
                now: 50.0,
                min: 10.0,
                max: 90.0
            })
        );
        assert_eq!(handle.orientation, Some(Orientation::Vertical));
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(Resizable::default, &[]);
    }
}
