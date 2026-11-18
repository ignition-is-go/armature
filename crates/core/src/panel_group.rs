use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::convert::Cast;
use crate::keys::{Dir, Key, Orientation};
use crate::number::read_number;
use crate::panel_layout::{
    PanelConstraints, PanelSizes, constraints_of, end_delta, fit_layout, handle_bounds, home_delta,
    initial_layout, is_collapsed, resize_at, resize_panel, toggle_target,
};
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
        Self {
            orientation: Orientation::Horizontal,
            dir: Dir::Ltr,
            min: 10.0,
            step: 1.0,
            large_step: 10.0,
        }
    }
}

const ACTIONS: &[&str] = &["set"];

/// A divider being dragged.
#[derive(Clone, Debug)]
struct HandleDrag {
    handle: usize,
    /// The panel sizes when the drag began, which the movement is measured from.
    sizes_at_start: Vec<f64>,
    /// Where the pointer was along the group when it began.
    began: f64,
}

/// Any number of panels side by side with a divider between each pair.
///
/// Moving a divider takes from the panel on one side and gives to the other. The value
/// is each panel's share in percent, which always add up to a hundred.
#[derive(Clone, Debug)]
pub struct PanelGroup {
    opts: Options,
    sizes: Vec<f64>,
    /// Each panel's own limits.
    panels: Vec<PanelConstraints>,
    /// The dividers that cannot be moved.
    locked: Vec<bool>,
    /// Each panel's limits as written, when some are in pixels.
    written_sizes: Vec<PanelSizes>,
    /// The length of the group as last measured.
    space: f64,
    drag: Option<HandleDrag>,
    focused: usize,
}

impl Default for PanelGroup {
    fn default() -> Self {
        Self::new(Options::default(), &[25.0, 50.0, 25.0])
    }
}

impl PanelGroup {
    /// The size of one panel, or zero for a panel there is none of.
    fn size_at(&self, at: usize) -> f64 {
        self.sizes.get(at).copied().unwrap_or_default()
    }

    /// `sizes` are brought to a total of a hundred, keeping their proportions.
    #[must_use]
    pub fn new(opts: Options, sizes: &[f64]) -> Self {
        let total: f64 = sizes.iter().sum();
        let sizes: Vec<f64> = sizes
            .iter()
            .map(|size| {
                if total > 0.0 {
                    size / total * 100.0
                } else {
                    100.0 / sizes.len().cast::<f64>()
                }
            })
            .collect();
        let panels = sizes
            .iter()
            .map(|_| PanelConstraints {
                min: opts.min,
                ..Default::default()
            })
            .collect();
        Self {
            opts,
            sizes,
            panels,
            locked: Vec::new(),
            written_sizes: Vec::new(),
            space: 0.0,
            drag: None,
            focused: 0,
        }
    }

    /// Gives each panel its own limits: a least and greatest size, and whether it
    /// can shut. The sizes are brought within them, and a panel with a default
    /// size starts at it.
    #[must_use]
    pub fn with_panels(mut self, panels: &[PanelConstraints]) -> Self {
        if panels.len() == self.sizes.len() {
            self.panels = panels.to_vec();
            self.sizes = if panels.iter().any(|panel| panel.initial.is_some()) {
                initial_layout(panels)
            } else {
                fit_layout(&self.sizes, panels)
            };
        }
        self
    }

    /// Gives each panel its limits as written, which may be in pixels or rems.
    /// Those take effect once the group has been measured.
    #[must_use]
    pub fn with_sizes(mut self, sizes: &[PanelSizes]) -> Self {
        if sizes.len() == self.sizes.len() {
            self.written_sizes = sizes.to_vec();
            let limits: Vec<PanelConstraints> = sizes
                .iter()
                .map(|sizes| constraints_of(*sizes, 0.0, 16.0))
                .collect();
            self = self.with_panels(&limits);
        }
        self
    }

    /// Fixes dividers in place, by their index. Resizing does not reach past one.
    #[must_use]
    pub fn locking(mut self, dividers: &[usize]) -> Self {
        self.locked = (0..self.sizes.len())
            .map(|at| dividers.contains(&at))
            .collect();
        self
    }

    /// Whether a panel that can shut is shut.
    #[must_use]
    pub fn is_collapsed(&self, panel: usize) -> bool {
        self.panels
            .get(panel)
            .is_some_and(|limits| is_collapsed(limits, self.size_at(panel)))
    }

    /// Each panel's share, in percent.
    #[must_use]
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
    /// `from`. A panel at its minimum passes the rest on to the next, and one
    /// that can shut does so past `threshold` of the way: half for a drag, all
    /// of it for a key.
    fn moved(&self, from: &[f64], at: usize, by: f64, threshold: f64) -> Vec<f64> {
        if self.locked.get(at).copied().unwrap_or(false) {
            return from.to_vec();
        }
        resize_at(from, &self.panels, at, by, threshold, &self.locked)
            .unwrap_or_else(|| from.to_vec())
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
        item?
            .parse::<usize>()
            .ok()
            .filter(|at| at.saturating_add(1) < self.sizes.len())
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

    fn measures(&self) -> bool {
        !self.written_sizes.is_empty()
    }

    /// Pixel limits are a share of the group's length, so they are worked out
    /// again whenever it changes, and the sizes brought back within them.
    fn measured(&mut self, measure: Measure<Part>) -> bool {
        let Some(rect) = measure(Part::Root, None) else {
            return false;
        };
        let space = if self.opts.orientation == Orientation::Horizontal {
            rect.width
        } else {
            rect.height
        };
        if (space - self.space).abs() < 0.5 {
            return false;
        }
        self.space = space;
        self.panels = self
            .written_sizes
            .iter()
            .map(|sizes| constraints_of(*sizes, space, 16.0))
            .collect();
        let next = fit_layout(&self.sizes, &self.panels);
        let changed = Self::written(&next) != Self::written(&self.sizes);
        self.sizes = next;
        changed
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
                let (least, most) = handle_bounds(&self.sizes, &self.panels, at, &self.locked);
                Semantics {
                    role: Some(Role::Separator),
                    range: Some(ValueRange {
                        now: self.size_at(at),
                        min: least,
                        max: most,
                    }),
                    // A divider between side-by-side panels stands upright.
                    orientation: Some(if self.opts.orientation == Orientation::Horizontal {
                        Orientation::Vertical
                    } else {
                        Orientation::Horizontal
                    }),
                    focusable: Focusable::TabStop,
                    active: self.drag.as_ref().is_some_and(|drag| drag.handle == at),
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
                if !(mods.ctrl || mods.alt)
                    && self.divider(Some(&self.focused.to_string())).is_some() =>
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
                    // As far as the panel before the divider goes, either way.
                    Key::Home => home_delta(&self.sizes, &self.panels, self.focused),
                    Key::End => end_delta(&self.sizes, &self.panels, self.focused),
                    // Enter shuts the panel beside the divider that can shut, or
                    // opens it again.
                    Key::Enter => {
                        let Some(panel) = toggle_target(&self.panels, self.focused) else {
                            return Reaction::default();
                        };
                        let limits = self.panels.get(panel).copied().unwrap_or_default();
                        let target = if self.is_collapsed(panel) {
                            limits.min
                        } else {
                            limits.collapsed
                        };
                        if let Some(next) = resize_panel(
                            &self.sizes,
                            &self.panels,
                            panel,
                            target,
                            1.0,
                            &self.locked,
                        ) {
                            self.commit(next, Source::User, Reason::Keyboard, gate);
                        }
                        return Reaction::handled();
                    }
                    _ => return Reaction::default(),
                };
                let next = self.moved(&self.sizes, self.focused, by, 1.0);
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
                self.drag = Some(HandleDrag {
                    handle: at,
                    sizes_at_start: self.sizes.clone(),
                    began: along,
                });
                self.focused = at;
                Reaction::to_item(Part::Handle, pointer.item).with(Effect::CapturePointer)
            }
            PointerPhase::Move => {
                let Some(HandleDrag {
                    handle,
                    sizes_at_start,
                    began,
                }) = self.drag.clone()
                else {
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
                let next = self.moved(&sizes_at_start, handle, by, 0.5);
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
        let numbers: Vec<f64> = value.iter().filter_map(|text| read_number(text)).collect();
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
            .and_then(|(at, share)| Some((at.parse::<usize>().ok()?, read_number(share)?)));
        let (at, share) = parsed
            .filter(|(at, _)| at.saturating_add(1) < self.sizes.len())
            .ok_or_else(|| ActError::UnknownItem(given.to_string()))?;
        let next = self.moved(&self.sizes, at, share - self.size_at(at), 1.0);
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

    /// Presses a key on the divider after panel `divider`.
    fn key(group: &mut PanelGroup, divider: usize, key: Key) {
        group.send(
            Input::Focus {
                part: Part::Handle,
                item: Some(divider.to_string()),
            },
            &mut |_| true,
        );
        group.send(
            Input::Key {
                key,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        );
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
        drag(&mut group, "1", 300.0, 240.0);
        assert_eq!(
            group.value(),
            ["35", "25", "40"],
            "the first panel is untouched"
        );
        drag(&mut group, "1", 240.0, 0.0);
        assert_eq!(
            group.value(),
            ["10", "10", "80"],
            "a panel at its minimum passes the rest on to the one before"
        );
        let mut group = PanelGroup::default();
        drag(&mut group, "0", 100.0, 140.0);
        drag(&mut group, "1", 300.0, 180.0);
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
            80.0,
            "it can take from both panels after it, down to their minimum"
        );
        assert!(
            group.semantics(Part::Handle, Some("2")).hidden,
            "nothing follows the last panel"
        );
    }

    #[test]
    fn a_panel_with_its_own_limits_shuts_opens_and_passes_a_resize_on() {
        let sidebar = PanelConstraints {
            min: 20.0,
            collapsible: true,
            ..Default::default()
        };
        let plain = PanelConstraints::default();
        let mut group = PanelGroup::new(Options::default(), &[30.0, 40.0, 30.0]).with_panels(&[
            sidebar,
            PanelConstraints { min: 20.0, ..plain },
            plain,
        ]);
        // Enter on the first divider shuts the sidebar, and again opens it to its least size.
        key(&mut group, 0, Key::Enter);
        assert!(group.is_collapsed(0));
        assert_eq!(group.sizes(), [0.0, 70.0, 30.0]);
        key(&mut group, 0, Key::Enter);
        assert_eq!(group.sizes(), [20.0, 50.0, 30.0]);
        // End takes from the middle panel until it is at its least, then from the last.
        key(&mut group, 0, Key::End);
        assert_eq!(group.sizes(), [100.0 - 20.0, 20.0, 0.0]);
        let range = group.semantics(Part::Handle, Some("0")).range.unwrap();
        assert_eq!((range.min, range.max), (0.0, 80.0));
    }

    #[test]
    fn pixel_limits_hold_as_the_group_is_measured_and_shrinks() {
        use crate::panel_layout::PanelSize;
        let sidebar = PanelSizes {
            min_size: Some(PanelSize::Px(200.0)),
            ..Default::default()
        };
        let mut group = PanelGroup::new(Options::default(), &[10.0, 90.0])
            .with_sizes(&[sidebar, PanelSizes::default()]);
        assert!(group.measures());
        assert_eq!(
            group.sizes(),
            [10.0, 90.0],
            "pixels mean nothing until the group is measured"
        );
        let wide = |_: Part, _: Option<&str>| Some(Rect::new(0.0, 0.0, 1000.0, 100.0));
        assert!(group.measured(&wide));
        assert_eq!(group.sizes(), [20.0, 80.0], "200 of 1000 pixels");
        assert!(!group.measured(&wide));
        let narrow = |_: Part, _: Option<&str>| Some(Rect::new(0.0, 0.0, 500.0, 100.0));
        assert!(group.measured(&narrow));
        assert_eq!(group.sizes(), [40.0, 60.0], "200 of 500 pixels");
    }

    #[test]
    fn a_locked_divider_does_not_move_and_stops_a_resize_reaching_past_it() {
        let limits = [PanelConstraints::default(); 3];
        let mut group = PanelGroup::new(Options::default(), &[20.0, 30.0, 50.0])
            .with_panels(&limits)
            .locking(&[1]);
        key(&mut group, 0, Key::End);
        assert_eq!(
            group.sizes(),
            [50.0, 0.0, 50.0],
            "the last panel keeps its size"
        );
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(PanelGroup::default, &[]);
    }
}
