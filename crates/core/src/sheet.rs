use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::gesture::{DECELERATION, VelocityTracker, project_distance, rubber_band};
use crate::keys::Key;
use crate::primitive::{
    Effect, Focusable, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive, Reaction,
    Relations, Role, Semantics, ValueRange,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Trigger,
    /// Covers the page behind the sheet. Pressing it dismisses the sheet.
    Backdrop,
    /// The sheet. Size it to `height`.
    Content,
    /// The grip at the sheet's top edge. Dragging it resizes the sheet.
    Handle,
    /// Optional. A scrolling box whose scroll position is the sheet's height, so the
    /// platform's own scrolling, with its momentum and touch handling, moves the
    /// sheet. Fill the screen with it and give it content one screen taller than the
    /// tallest height. The sheet settles on a height once scrolling rests.
    Scroller,
}

/// How long scrolling must pause before a scrolled sheet settles.
const REST_MS: u64 = 120;

const ACTIONS: &[&str] = &["open", "close", "snap"];

/// A panel that slides up from the bottom edge and rests at one of several heights.
/// Dragging its grip moves it between them, and dragging it well below the lowest
/// dismisses it. The value is the index of the height it rests at, or empty when
/// closed.
#[derive(Clone, Debug)]
pub struct Sheet {
    /// The heights it can rest at, lowest first.
    detents: Vec<f64>,
    dismissible: bool,
    /// The detent it rests at, when open.
    at: Option<usize>,
    /// Where the pointer went down and how tall the sheet was then.
    grabbed: Option<(f64, f64)>,
    height: f64,
    speed: VelocityTracker,
    /// Whether the trigger holds focus, so Enter and Space can press it.
    on_trigger: bool,
    /// Where scrolling has the sheet, and when it settles if nothing more moves it.
    scrolled: Option<(f64, u64)>,
}

impl Default for Sheet {
    fn default() -> Self {
        Sheet::new(vec![200.0, 480.0], true)
    }
}

impl Sheet {
    pub fn new(mut detents: Vec<f64>, dismissible: bool) -> Self {
        detents.sort_by(f64::total_cmp);
        Sheet {
            detents,
            dismissible,
            at: None,
            grabbed: None,
            height: 0.0,
            speed: VelocityTracker::default(),
            on_trigger: false,
            scrolled: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.at.is_some()
    }

    /// How tall to draw the sheet now: its resting height, or wherever a drag has it.
    pub fn height(&self) -> f64 {
        if let (Some((offset, _)), Some(_)) = (self.scrolled, self.at) {
            return offset;
        }
        match (self.grabbed, self.at) {
            (Some(_), _) => self.height,
            (None, Some(at)) => self.detents[at],
            (None, None) => 0.0,
        }
    }

    fn written(at: Option<usize>) -> Vec<String> {
        at.iter().map(|index| index.to_string()).collect()
    }

    fn settle(&mut self, at: Option<usize>, source: Source, reason: Reason, gate: Gate) -> bool {
        if at == self.at {
            return true;
        }
        let allowed = gate(&ValueChange {
            value: Self::written(at),
            source,
            reason,
        });
        if allowed {
            self.at = at;
        }
        allowed
    }

    /// Where a sheet released at `height` moving at `velocity` comes to rest: the
    /// detent nearest to where it would coast, or closed if that is well below the
    /// lowest one.
    fn landing(&self, height: f64, velocity: f64) -> Option<usize> {
        let coasted = height + project_distance(velocity, DECELERATION);
        let lowest = self.detents.first().copied().unwrap_or(0.0);
        if self.dismissible && coasted < lowest / 2.0 {
            return None;
        }
        let distance = |detent: &f64| (detent - coasted).abs();
        (0..self.detents.len())
            .min_by(|a, b| distance(&self.detents[*a]).total_cmp(&distance(&self.detents[*b])))
    }

    fn dismiss(&mut self, reason: Reason, gate: Gate) -> Reaction<Part> {
        if self.dismissible && self.settle(None, Source::User, reason, gate) {
            Reaction::focusing(Part::Trigger)
        } else {
            Reaction::handled()
        }
    }
}

impl PartOf for Part {
    type Primitive = Sheet;
}

impl Primitive for Sheet {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[
            Part::Trigger,
            Part::Backdrop,
            Part::Content,
            Part::Handle,
            Part::Scroller,
        ]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn tracks_pointer(part: Part) -> bool {
        matches!(part, Part::Backdrop | Part::Handle)
    }

    fn scrolls(part: Part) -> bool {
        part == Part::Scroller
    }

    fn wanted_scroll(&self) -> Option<f64> {
        Some(self.at.map_or(0.0, |at| self.detents[at]))
    }

    fn relations(part: Part) -> Relations<Part> {
        match part {
            Part::Trigger => Relations {
                controls: Some(Part::Content),
                ..Default::default()
            },
            _ => Relations::default(),
        }
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, _: Option<&str>) -> Semantics {
        let hidden = !self.is_open();
        match part {
            Part::Trigger => Semantics {
                role: Some(Role::Button),
                expanded: Some(!hidden),
                focusable: Focusable::TabStop,
                ..Default::default()
            },
            Part::Backdrop | Part::Scroller => Semantics {
                hidden,
                ..Default::default()
            },
            Part::Content => Semantics {
                role: Some(Role::Dialog),
                hidden,
                modal: true,
                focusable: Focusable::Programmatic,
                ..Default::default()
            },
            Part::Handle => Semantics {
                role: Some(Role::Slider),
                hidden,
                range: Some(ValueRange {
                    now: self.height(),
                    min: self.detents.first().copied().unwrap_or(0.0),
                    max: self.detents.last().copied().unwrap_or(0.0),
                }),
                focusable: if hidden {
                    Focusable::No
                } else {
                    Focusable::TabStop
                },
                active: self.grabbed.is_some(),
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        let last = self.detents.len().saturating_sub(1);
        match (input, self.at) {
            (Input::Focus { part, .. }, _) => {
                self.on_trigger = part == Part::Trigger;
                Reaction::default()
            }
            // Scrolling carries the sheet. Each movement puts the settling off again.
            (Input::Scrolled { offset, now_ms, .. }, Some(at)) => {
                let moved = self
                    .scrolled
                    .map_or(offset != self.detents[at], |(was, _)| was != offset);
                if !moved {
                    return Reaction::default();
                }
                let at_ms = now_ms + REST_MS;
                self.scrolled = Some((offset, at_ms));
                Reaction::default().with(Effect::Wake { at_ms })
            }
            (Input::Tick { now_ms }, Some(_)) => {
                let Some((offset, _)) = self.scrolled.filter(|(_, at_ms)| now_ms >= *at_ms) else {
                    return Reaction::default();
                };
                self.scrolled = None;
                let landing = self.landing(offset, 0.0);
                let settled = self.settle(landing, Source::User, Reason::Drag, gate);
                let rest = Effect::ScrollTo {
                    offset: self.at.map_or(0.0, |at| self.detents[at]) as u32,
                };
                if settled && landing.is_none() {
                    return Reaction::focusing(Part::Trigger).with(rest);
                }
                Reaction::default().with(rest)
            }
            // The trigger is a button, so Enter and Space press it.
            (
                Input::Key {
                    key: Key::Enter | Key::Space,
                    mods,
                    ..
                },
                _,
            ) if self.on_trigger => self.send(
                Input::Press {
                    part: Part::Trigger,
                    item: None,
                    mods,
                },
                gate,
            ),
            (
                Input::Press {
                    part: Part::Trigger,
                    ..
                },
                None,
            ) => {
                if !self.detents.is_empty()
                    && self.settle(Some(0), Source::User, Reason::Press, gate)
                {
                    Reaction::focusing(Part::Handle)
                } else {
                    Reaction::handled()
                }
            }
            (
                Input::Press {
                    part: Part::Trigger,
                    ..
                },
                Some(_),
            ) => self.dismiss(Reason::Press, gate),
            (
                Input::Key {
                    key: Key::Escape, ..
                },
                Some(_),
            ) => self.dismiss(Reason::Keyboard, gate),
            (Input::Key { key, .. }, Some(at)) => {
                let target = match key {
                    Key::ArrowUp => (at + 1).min(last),
                    Key::ArrowDown if at == 0 => return self.dismiss(Reason::Keyboard, gate),
                    Key::ArrowDown => at - 1,
                    Key::Home => 0,
                    Key::End => last,
                    _ => return Reaction::default(),
                };
                self.settle(Some(target), Source::User, Reason::Keyboard, gate);
                Reaction::handled()
            }
            _ => Reaction::default(),
        }
    }

    fn pointer(&mut self, pointer: Pointer<Part>, _: Measure<Part>, gate: Gate) -> Reaction<Part> {
        let Some(at) = self.at else {
            return Reaction::default();
        };
        match (pointer.phase, pointer.part) {
            (PointerPhase::Down, Part::Backdrop) => self.dismiss(Reason::Press, gate),
            (PointerPhase::Down, Part::Handle) => {
                self.height = self.detents[at];
                self.grabbed = Some((pointer.at.y, self.height));
                self.speed = VelocityTracker::default();
                Reaction::focusing(Part::Handle).with(Effect::CapturePointer)
            }
            (PointerPhase::Move, _) => {
                let Some((from, started)) = self.grabbed else {
                    return Reaction::default();
                };
                // Dragging up makes the sheet taller. Past the tallest height it resists.
                let wanted = (started + from - pointer.at.y).max(0.0);
                let tallest = self.detents.last().copied().unwrap_or(0.0);
                self.height = if wanted > tallest {
                    tallest + rubber_band(wanted - tallest, tallest.max(1.0), 0.55)
                } else {
                    wanted
                };
                self.speed.push(pointer.now_ms as f64, self.height);
                Reaction::handled()
            }
            (PointerPhase::Up, _) => {
                if self.grabbed.take().is_none() {
                    return Reaction::default();
                }
                let landing =
                    self.landing(self.height, self.speed.velocity_at(pointer.now_ms as f64));
                if self.settle(landing, Source::User, Reason::Drag, gate) && landing.is_none() {
                    return Reaction::focusing(Part::Trigger);
                }
                Reaction::handled()
            }
            (PointerPhase::Cancel, _) => {
                self.grabbed = None;
                Reaction::handled()
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        Self::written(self.at)
    }

    fn set_value(&mut self, value: Vec<String>) {
        self.at = value
            .first()
            .and_then(|text| text.parse::<usize>().ok())
            .filter(|index| *index < self.detents.len());
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "sheet",
            label: label.to_string(),
            value: self.value(),
            expanded: Some(self.is_open()),
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let target = match action {
            "open" => (!self.detents.is_empty()).then_some(self.at.unwrap_or(0)),
            "close" => None,
            "snap" => {
                let text = item.ok_or(ActError::MissingItem { action: "snap" })?;
                let index = text
                    .parse::<usize>()
                    .ok()
                    .filter(|index| *index < self.detents.len());
                Some(index.ok_or_else(|| ActError::UnknownItem(text.to_string()))?)
            }
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        if self.settle(target, Source::Agent, Reason::Action, gate) {
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

    fn opened() -> Sheet {
        let mut sheet = Sheet::default();
        sheet.send(
            Input::Press {
                part: Part::Trigger,
                item: None,
                mods: Modifiers::default(),
            },
            &mut |_| true,
        );
        sheet
    }

    fn pointer(
        sheet: &mut Sheet,
        phase: PointerPhase,
        part: Part,
        y: f64,
        now_ms: u64,
    ) -> Reaction<Part> {
        let pointer = Pointer {
            phase,
            part,
            item: None,
            at: Point::new(0.0, y),
            mods: Modifiers::default(),
            now_ms,
        };
        sheet.pointer(pointer, &|_, _| None, &mut |_| true)
    }

    /// Drags the grip up by `rise` over a second, slowly, and lets go.
    fn drag(sheet: &mut Sheet, rise: f64) {
        pointer(sheet, PointerPhase::Down, Part::Handle, 600.0, 0);
        pointer(
            sheet,
            PointerPhase::Move,
            Part::Handle,
            600.0 - rise * 0.9,
            900,
        );
        pointer(sheet, PointerPhase::Move, Part::Handle, 600.0 - rise, 1000);
        pointer(sheet, PointerPhase::Up, Part::Handle, 600.0 - rise, 1000);
    }

    #[test]
    fn it_opens_at_its_lowest_height_with_focus_on_the_grip() {
        let mut sheet = Sheet::default();
        let opened = sheet.send(
            Input::Press {
                part: Part::Trigger,
                item: None,
                mods: Modifiers::default(),
            },
            &mut |_| true,
        );
        assert_eq!(sheet.height(), 200.0);
        assert_eq!(opened.focus.unwrap().part, Part::Handle);
        assert_eq!(sheet.value(), ["0"]);
    }

    #[test]
    fn a_drag_follows_the_pointer_then_rests_at_the_nearest_height() {
        let mut sheet = opened();
        pointer(&mut sheet, PointerPhase::Down, Part::Handle, 600.0, 0);
        pointer(&mut sheet, PointerPhase::Move, Part::Handle, 500.0, 500);
        assert_eq!(sheet.height(), 300.0);
        pointer(&mut sheet, PointerPhase::Cancel, Part::Handle, 500.0, 500);
        drag(&mut sheet, 180.0);
        assert_eq!(sheet.value(), ["1"], "380 is nearer the tall height");
        assert_eq!(sheet.height(), 480.0);
    }

    #[test]
    fn a_flick_upward_carries_it_to_the_tall_height() {
        let mut sheet = opened();
        pointer(&mut sheet, PointerPhase::Down, Part::Handle, 600.0, 0);
        pointer(&mut sheet, PointerPhase::Move, Part::Handle, 590.0, 10);
        pointer(&mut sheet, PointerPhase::Move, Part::Handle, 560.0, 40);
        pointer(&mut sheet, PointerPhase::Up, Part::Handle, 560.0, 40);
        assert_eq!(sheet.value(), ["1"], "only 40 up, but moving fast");
    }

    #[test]
    fn dragging_well_below_the_lowest_height_dismisses_it() {
        let mut sheet = opened();
        drag(&mut sheet, -150.0);
        assert!(!sheet.is_open());
        let mut pinned = Sheet::new(vec![200.0, 480.0], false);
        pinned.set_value(vec!["0".into()]);
        drag(&mut pinned, -150.0);
        assert_eq!(
            pinned.value(),
            ["0"],
            "one that cannot be dismissed springs back"
        );
    }

    #[test]
    fn past_its_tallest_height_it_resists() {
        let mut sheet = opened();
        pointer(&mut sheet, PointerPhase::Down, Part::Handle, 600.0, 0);
        pointer(&mut sheet, PointerPhase::Move, Part::Handle, 0.0, 500);
        assert!(sheet.height() > 480.0 && sheet.height() < 800.0);
    }

    #[test]
    fn arrows_step_between_heights_and_down_from_the_lowest_dismisses() {
        let mut sheet = opened();
        let key = |sheet: &mut Sheet, key| {
            sheet.send(
                Input::Key {
                    key,
                    mods: Modifiers::default(),
                    now_ms: 0,
                },
                &mut |_| true,
            )
        };
        key(&mut sheet, Key::ArrowUp);
        assert_eq!(sheet.value(), ["1"]);
        key(&mut sheet, Key::ArrowDown);
        let closed = key(&mut sheet, Key::ArrowDown);
        assert!(!sheet.is_open());
        assert_eq!(closed.focus.unwrap().part, Part::Trigger);
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(Sheet::default, &[]);
    }

    #[test]
    fn scrolling_carries_the_sheet_and_it_settles_on_the_nearest_height_at_rest() {
        let mut sheet = Sheet::default();
        sheet.act("open", None, &mut |_| true).unwrap();
        assert_eq!(sheet.wanted_scroll(), Some(200.0));
        let scroll = |sheet: &mut Sheet, offset: f64, now_ms: u64| {
            sheet.send(
                Input::Scrolled {
                    part: Part::Scroller,
                    offset,
                    viewport: 800.0,
                    now_ms,
                },
                &mut |_| true,
            )
        };
        assert!(
            scroll(&mut sheet, 200.0, 0).effects.is_empty(),
            "already there"
        );
        assert_eq!(
            scroll(&mut sheet, 300.0, 0).effects,
            [Effect::Wake { at_ms: 120 }]
        );
        scroll(&mut sheet, 390.0, 60);
        assert_eq!(sheet.height(), 390.0, "the sheet is where scrolling has it");
        assert!(
            sheet
                .send(Input::Tick { now_ms: 120 }, &mut |_| true)
                .effects
                .is_empty(),
            "still moving"
        );
        let rested = sheet.send(Input::Tick { now_ms: 180 }, &mut |_| true);
        assert_eq!(rested.effects, [Effect::ScrollTo { offset: 480 }]);
        assert_eq!(sheet.value(), ["1"]);

        scroll(&mut sheet, 40.0, 500);
        let closed = sheet.send(Input::Tick { now_ms: 620 }, &mut |_| true);
        assert!(!sheet.is_open(), "scrolled well below the lowest height");
        assert_eq!(closed.focus.unwrap().part, Part::Trigger);
    }
}
