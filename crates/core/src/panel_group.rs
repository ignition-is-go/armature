use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::{Dir, Key, Orientation};
use crate::primitive::{
    Effect, Focusable, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive, Reaction,
    Role, Semantics, ValueRange,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    /// One panel, keyed by its place: `0`, `1` and so on. Size it to `sizes`.
    Panel,
    /// The divider after a panel, keyed like that panel.
    Handle,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub orientation: Orientation,
    pub dir: Dir,
    /// The smallest share any panel may have, in percent.
    pub min: f64,
    /// How far an arrow moves a divider, in percent.
    pub step: f64,
    /// How far Shift with an arrow moves it.
    pub large_step: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            orientation: Orientation::Horizontal,
            dir: Dir::Ltr,
            min: 10.0,
            step: 1.0,
            large_step: 10.0,
        }
    }
}

const ACTIONS: &[&str] = &["set"];

/// Any number of panels side by side with a divider between each pair. Moving a
/// divider takes from the panel on one side and gives to the other. The value is
/// each panel's share in percent, which always add up to a hundred.
#[derive(Clone, Debug)]
pub struct PanelGroup {
    opts: Options,
    sizes: Vec<f64>,
    /// The divider being dragged, the sizes when the drag began, and where it began.
    drag: Option<(usize, Vec<f64>, f64)>,
    focused: usize,
}

impl Default for PanelGroup {
    fn default() -> Self {
        PanelGroup::new(Options::default(), &[25.0, 50.0, 25.0])
    }
}

impl PanelGroup {
    /// `sizes` are brought to a total of a hundred, keeping their proportions.
    pub fn new(opts: Options, sizes: &[f64]) -> Self {
        let total: f64 = sizes.iter().sum();
        let sizes = sizes
            .iter()
            .map(|size| {
                if total > 0.0 {
                    size / total * 100.0
                } else {
                    100.0 / sizes.len() as f64
                }
            })
            .collect();
        PanelGroup {
            opts,
            sizes,
            drag: None,
            focused: 0,
        }
    }

    /// Each panel's share, in percent.
    pub fn sizes(&self) -> &[f64] {
        &self.sizes
    }

    fn written(sizes: &[f64]) -> Vec<String> {
        sizes
            .iter()
            .map(|size| ((size * 100.0).round() / 100.0).to_string())
            .collect()
    }

    /// The sizes after the divider following panel `at` moves by `by` percent, from
    /// `from`. Neither panel beside it goes below the minimum.
    fn moved(&self, from: &[f64], at: usize, by: f64) -> Vec<f64> {
        let (before, after) = (from[at], from[at + 1]);
        let by = by.clamp(self.opts.min - before, after - self.opts.min);
        let mut next = from.to_vec();
        next[at] = before + by;
        next[at + 1] = after - by;
        next
    }

    fn commit(&mut self, next: Vec<f64>, source: Source, reason: Reason, gate: Gate) -> bool {
        if Self::written(&next) == Self::written(&self.sizes) {
            return true;
        }
        let allowed = gate(&ValueChange {
            value: Self::written(&next),
            source,
            reason,
        });
        if allowed {
            self.sizes = next;
        }
        allowed
    }

    fn divider(&self, item: Option<&str>) -> Option<usize> {
        item?.parse().ok().filter(|at| at + 1 < self.sizes.len())
    }
}

impl PartOf for Part {
    type Primitive = PanelGroup;
}

impl Primitive for PanelGroup {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Panel, Part::Handle]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn tracks_pointer(part: Part) -> bool {
        part == Part::Handle
    }

    fn named_part(&self) -> Option<Part> {
        Some(Part::Root)
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match (part, self.divider(item)) {
            (Part::Root, _) => Semantics {
                role: Some(Role::Group),
                ..Default::default()
            },
            (Part::Panel, _) => Semantics::default(),
            (Part::Handle, Some(at)) => {
                let pair = self.sizes[at] + self.sizes[at + 1];
                Semantics {
                    role: Some(Role::Separator),
                    range: Some(ValueRange {
                        now: self.sizes[at],
                        min: self.opts.min,
                        max: pair - self.opts.min,
                    }),
                    // A divider between side-by-side panels stands upright.
                    orientation: Some(if self.opts.orientation == Orientation::Horizontal {
                        Orientation::Vertical
                    } else {
                        Orientation::Horizontal
                    }),
                    focusable: Focusable::TabStop,
                    active: self.drag.as_ref().is_some_and(|(held, _, _)| *held == at),
                    ..Default::default()
                }
            }
            (Part::Handle, None) => Semantics {
                role: Some(Role::Separator),
                hidden: item.is_some(),
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Focus {
                part: Part::Handle,
                item,
            } => {
                self.focused = self.divider(item.as_deref()).unwrap_or(self.focused);
                Reaction::default()
            }
            Input::Key { key, mods, .. }
                if !mods.ctrl && !mods.alt && self.focused + 1 < self.sizes.len() =>
            {
                let step = if mods.shift {
                    self.opts.large_step
                } else {
                    self.opts.step
                };
                let across = self.opts.orientation == Orientation::Horizontal;
                let forward = if self.opts.dir == Dir::Rtl { -1.0 } else { 1.0 };
                let by = match key {
                    Key::ArrowRight if across => step * forward,
                    Key::ArrowLeft if across => -step * forward,
                    Key::ArrowDown if !across => step,
                    Key::ArrowUp if !across => -step,
                    // All the way to one side or the other.
                    Key::Home => -100.0,
                    Key::End => 100.0,
                    _ => return Reaction::default(),
                };
                let next = self.moved(&self.sizes, self.focused, by);
                self.commit(next, Source::User, Reason::Keyboard, gate);
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
        let across = self.opts.orientation == Orientation::Horizontal;
        let along = if across { pointer.at.x } else { pointer.at.y };
        match pointer.phase {
            PointerPhase::Down => {
                let Some(at) = self.divider(pointer.item.as_deref()) else {
                    return Reaction::default();
                };
                self.drag = Some((at, self.sizes.clone(), along));
                self.focused = at;
                Reaction::item(Part::Handle, true, pointer.item).with(Effect::CapturePointer)
            }
            PointerPhase::Move => {
                let Some((at, from, began)) = self.drag.clone() else {
                    return Reaction::default();
                };
                let Some(group) = measure(Part::Root, None) else {
                    return Reaction::default();
                };
                let length = if across { group.width } else { group.height };
                let mut by = (along - began) / length.max(1.0) * 100.0;
                if across && self.opts.dir == Dir::Rtl {
                    by = -by;
                }
                let next = self.moved(&from, at, by);
                self.commit(next, Source::User, Reason::Drag, gate);
                Reaction::handled()
            }
            PointerPhase::Up | PointerPhase::Cancel => {
                self.drag = None;
                Reaction::handled()
            }
        }
    }

    fn value(&self) -> Vec<String> {
        Self::written(&self.sizes)
    }

    fn set_value(&mut self, value: Vec<String>) {
        let numbers: Vec<f64> = value.iter().filter_map(|text| text.parse().ok()).collect();
        let total: f64 = numbers.iter().sum();
        if numbers.len() == self.sizes.len() && total > 0.0 {
            self.sizes = numbers.iter().map(|size| size / total * 100.0).collect();
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "panel-group",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    /// `set` takes a panel and its share as `index=percent`. The panel after it
    /// gives up or takes on the difference.
    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        if action != "set" {
            return Err(ActError::UnknownAction {
                action: action.to_string(),
                valid: ACTIONS,
            });
        }
        let given = item.ok_or(ActError::MissingItem { action: "set" })?;
        let parsed = given
            .split_once('=')
            .and_then(|(at, share)| Some((at.parse::<usize>().ok()?, share.parse::<f64>().ok()?)));
        let (at, share) = parsed
            .filter(|(at, _)| at + 1 < self.sizes.len())
            .ok_or_else(|| ActError::UnknownItem(given.to_string()))?;
        let next = self.moved(&self.sizes, at, share - self.sizes[at]);
        if self.commit(next, Source::Agent, Reason::Action, gate) {
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

    fn drag(group: &mut PanelGroup, item: &str, from: f64, to: f64) {
        for (phase, x) in [
            (PointerPhase::Down, from),
            (PointerPhase::Move, to),
            (PointerPhase::Up, to),
        ] {
            let pointer = Pointer {
                phase,
                part: Part::Handle,
                item: Some(item.into()),
                at: Point::new(x, 5.0),
                mods: Modifiers::default(),
                now_ms: 0,
            };
            group.pointer(
                pointer,
                &|_, _| Some(Rect::new(0.0, 0.0, 400.0, 100.0)),
                &mut |_| true,
            );
        }
    }

    #[test]
    fn a_divider_moves_share_between_the_two_panels_beside_it() {
        let mut group = PanelGroup::default();
        drag(&mut group, "0", 100.0, 140.0);
        assert_eq!(
            group.value(),
            ["35", "40", "25"],
            "the third panel is untouched"
        );
        drag(&mut group, "1", 300.0, 0.0);
        assert_eq!(
            group.value(),
            ["35", "10", "55"],
            "the middle panel keeps its minimum"
        );
        assert_eq!(group.sizes().iter().sum::<f64>(), 100.0);
    }

    #[test]
    fn arrows_nudge_the_focused_divider_and_home_and_end_go_all_the_way() {
        let mut group = PanelGroup::default();
        let key = |group: &mut PanelGroup, key, shift| {
            group.send(
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
        key(&mut group, Key::ArrowRight, false);
        key(&mut group, Key::ArrowRight, true);
        assert_eq!(group.value(), ["36", "39", "25"]);
        key(&mut group, Key::Home, false);
        assert_eq!(group.value(), ["10", "65", "25"]);
        group.act("set", Some("1=30"), &mut |_| true).unwrap();
        assert_eq!(group.value(), ["10", "30", "60"]);
    }

    #[test]
    fn sizes_given_in_any_units_are_shared_out_of_a_hundred() {
        let group = PanelGroup::new(Options::default(), &[1.0, 1.0, 2.0]);
        assert_eq!(group.value(), ["25", "25", "50"]);
        assert_eq!(
            group.semantics(Part::Handle, Some("0")).range.unwrap().max,
            40.0
        );
        assert!(
            group.semantics(Part::Handle, Some("2")).hidden,
            "nothing follows the last panel"
        );
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(PanelGroup::default, &[]);
    }
}
