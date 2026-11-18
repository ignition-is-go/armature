use crate::agent::{ActError, Snapshot};
use crate::change::Gate;
use crate::convert::Cast;
use crate::keys::Orientation;
use crate::primitive::{
    Effect, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive, Reaction, Role,
    Semantics, ValueRange,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    /// The box that scrolls. Hide the platform's own scrollbar on it.
    Viewport,
    /// The track down the side. A press on it jumps a page towards the press.
    Scrollbar,
    /// The grip. Place it at `thumb` along the track, and drag it to scroll.
    Thumb,
}

/// How long after the last movement the scrollbar still counts as in use.
const IDLE_MS: u64 = 1000;

/// A scrolling box with a scrollbar you draw yourself. The platform still does the
/// scrolling, so wheels, touch and the keyboard behave as they do everywhere. It
/// scrolls down the page. It has no value.
#[derive(Clone, Debug)]
pub struct ScrollArea {
    offset: f64,
    viewport: f64,
    content: f64,
    /// Where on the grip a drag took hold, and how long the track was.
    drag: Option<(f64, f64)>,
    hovered: bool,
    idle_at: Option<u64>,
    scrolling: bool,
    axis: Orientation,
    /// Whether to stay at the end as content is added, as a chat or a log does,
    /// for as long as the person has not scrolled away from it.
    stick_to_end: bool,
}

impl Default for ScrollArea {
    fn default() -> Self {
        Self {
            offset: 0.0,
            viewport: 0.0,
            content: 0.0,
            drag: None,
            hovered: false,
            idle_at: None,
            scrolling: false,
            axis: Orientation::Vertical,
            stick_to_end: false,
        }
    }
}

impl ScrollArea {
    /// A box that scrolls across the page. The default scrolls down it. For both,
    /// put one inside the other.
    #[must_use]
    pub fn horizontal() -> Self {
        Self {
            axis: Orientation::Horizontal,
            ..Self::default()
        }
    }

    #[must_use]
    pub const fn stick_to_end(mut self) -> Self {
        self.stick_to_end = true;
        self
    }

    /// Whether there is more content than fits.
    #[must_use]
    pub fn overflows(&self) -> bool {
        self.content > self.viewport + 0.5
    }

    /// Whether the scrollbar is in use: being scrolled, dragged or pointed at. For
    /// scrollbars that fade away between uses.
    #[must_use]
    pub const fn in_use(&self) -> bool {
        self.scrolling || self.hovered || self.drag.is_some()
    }

    /// Where the grip starts and how long it is, on a track of `track` length.
    #[must_use]
    pub fn thumb(&self, track: f64) -> (f64, f64) {
        if !self.overflows() {
            return (0.0, track);
        }
        let metrics = crate::scroll_model::AxisMetrics {
            position: self.offset,
            client: self.viewport,
            scroll: self.content,
        };
        let (length, offset) = crate::scroll_model::thumb_geometry(metrics, track);
        (offset, length)
    }

    /// The scroll position that puts the grip's start at `at` on the track.
    fn offset_for(&self, at: f64, track: f64) -> f64 {
        let (_, length) = self.thumb(track);
        let room = (track - length).max(f64::MIN_POSITIVE);
        ((at / room).clamp(0.0, 1.0) * (self.content - self.viewport)).max(0.0)
    }
}

impl PartOf for Part {
    type Primitive = ScrollArea;
}

impl Primitive for ScrollArea {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Viewport, Part::Scrollbar, Part::Thumb]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn scrolls(part: Part) -> bool {
        part == Part::Viewport
    }

    fn scroll_axis(&self) -> Orientation {
        self.axis
    }

    fn tracks_pointer(part: Part) -> bool {
        matches!(part, Part::Scrollbar | Part::Thumb)
    }

    fn hovers(part: Part) -> bool {
        part == Part::Root
    }

    fn named_part(&self) -> Option<Part> {
        Some(Part::Viewport)
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, _: Option<&str>) -> Semantics {
        match part {
            Part::Root => Semantics::default(),
            Part::Viewport => Semantics {
                role: Some(Role::Region),
                ..Default::default()
            },
            // The platform's scrolling is what the keyboard and assistive technology
            // use. The drawn scrollbar is for the pointer, so it takes no focus.
            Part::Scrollbar => Semantics {
                role: Some(Role::ScrollBar),
                orientation: Some(self.axis),
                hidden: !self.overflows(),
                range: Some(ValueRange {
                    now: self.offset,
                    min: 0.0,
                    max: (self.content - self.viewport).max(0.0),
                }),
                active: self.in_use(),
                ..Default::default()
            },
            Part::Thumb => Semantics {
                hidden: !self.overflows(),
                active: self.drag.is_some(),
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, _: Gate) -> Reaction<Part> {
        match input {
            Input::Scrolled {
                offset,
                viewport,
                content,
                now_ms,
                ..
            } => {
                let moved = (offset - self.offset).abs() > f64::EPSILON;
                // At the end before, and the content has grown: follow it down.
                let was_at_end = self.content - self.viewport - self.offset <= 24.0;
                let grew = content > self.content && self.content > 0.0;
                (self.offset, self.viewport, self.content) = (offset, viewport, content);
                if self.stick_to_end && was_at_end && grew && !moved {
                    let end = (content - viewport).max(0.0);
                    return Reaction::default().with(Effect::ScrollTo {
                        offset: end.round().cast::<u32>(),
                    });
                }
                if !moved {
                    return Reaction::default();
                }
                self.scrolling = true;
                let at_ms = now_ms.saturating_add(IDLE_MS);
                self.idle_at = Some(at_ms);
                Reaction::default().with(Effect::Wake { at_ms })
            }
            Input::Tick { now_ms } => {
                if self.idle_at.is_some_and(|at| now_ms >= at) {
                    self.scrolling = false;
                    self.idle_at = None;
                }
                Reaction::default()
            }
            Input::Hover {
                part: Part::Root,
                inside,
                ..
            } => {
                self.hovered = inside;
                Reaction::default()
            }
            _ => Reaction::default(),
        }
    }

    fn pointer(
        &mut self,
        pointer: Pointer<Part>,
        measure: Measure<Part>,
        _: Gate,
    ) -> Reaction<Part> {
        let Some(track) = measure(Part::Scrollbar, None) else {
            return Reaction::default();
        };
        let across = self.axis == Orientation::Horizontal;
        let (along, length) = if across {
            (pointer.at.x - track.x, track.width)
        } else {
            (pointer.at.y - track.y, track.height)
        };
        let (start, _) = self.thumb(length);
        let scroll_to = |offset: f64| {
            Reaction::handled().with(Effect::ScrollTo {
                offset: offset.round().cast::<u32>(),
            })
        };
        match (pointer.phase, pointer.part) {
            (PointerPhase::Down, Part::Thumb) => {
                self.drag = Some((along - start, length));
                Reaction::handled().with(Effect::CapturePointer)
            }
            // A press on the track pages towards it, as a platform scrollbar does.
            (PointerPhase::Down, Part::Scrollbar) => {
                let page = if along < start {
                    -self.viewport
                } else {
                    self.viewport
                };
                let limit = (self.content - self.viewport).max(0.0);
                scroll_to((self.offset + page).clamp(0.0, limit))
            }
            (PointerPhase::Move, _) => match self.drag {
                Some((grip, track)) => scroll_to(self.offset_for(along - grip, track)),
                None => Reaction::default(),
            },
            (PointerPhase::Up | PointerPhase::Cancel, _) => {
                self.drag = None;
                Reaction::default()
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        Vec::new()
    }

    fn set_value(&mut self, _: Vec<String>) {}

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "scroll-area",
            label: label.to_string(),
            value: Vec::new(),
            expanded: None,
            actions: &[],
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, _: Option<&str>, _: Gate) -> Result<(), ActError> {
        Err(ActError::UnknownAction {
            action: action.to_string(),
            valid: &[],
        })
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

    fn area(offset: f64) -> ScrollArea {
        let mut area = ScrollArea::default();
        area.send(
            Input::Scrolled {
                part: Part::Viewport,
                offset,
                viewport: 200.0,
                content: 1000.0,
                now_ms: 0,
            },
            &mut |_| true,
        );
        area
    }

    fn pointer(area: &mut ScrollArea, phase: PointerPhase, part: Part, y: f64) -> Reaction<Part> {
        let pointer = Pointer {
            phase,
            part,
            item: None,
            at: Point::new(0.0, y),
            mods: Modifiers::default(),
            now_ms: 0,
        };
        area.pointer(
            pointer,
            &|_, _| Some(Rect::new(0.0, 0.0, 10.0, 200.0)),
            &mut |_| true,
        )
    }

    #[test]
    fn the_grip_is_as_long_as_the_share_in_view_and_sits_where_the_scroll_is() {
        assert_eq!(area(0.0).thumb(200.0), (0.0, 40.0));
        assert_eq!(area(400.0).thumb(200.0), (80.0, 40.0));
        assert_eq!(area(800.0).thumb(200.0), (160.0, 40.0));
        let mut long = ScrollArea::default();
        long.send(
            Input::Scrolled {
                part: Part::Viewport,
                offset: 0.0,
                viewport: 100.0,
                content: 100_000.0,
                now_ms: 0,
            },
            &mut |_| true,
        );
        assert_eq!(long.thumb(100.0).1, 16.0, "never too small to grab");
        let mut short = ScrollArea::default();
        short.send(
            Input::Scrolled {
                part: Part::Viewport,
                offset: 0.0,
                viewport: 100.0,
                content: 80.0,
                now_ms: 0,
            },
            &mut |_| true,
        );
        assert!(
            short.semantics(Part::Scrollbar, None).hidden,
            "nothing to scroll"
        );
    }

    #[test]
    fn dragging_the_grip_scrolls_in_proportion_and_a_press_on_the_track_pages() {
        let mut area = area(0.0);
        pointer(&mut area, PointerPhase::Down, Part::Thumb, 10.0);
        let dragged = pointer(&mut area, PointerPhase::Move, Part::Thumb, 90.0);
        assert_eq!(
            dragged.effects,
            [Effect::ScrollTo { offset: 400 }],
            "half the track's free room is half the way"
        );
        pointer(&mut area, PointerPhase::Up, Part::Thumb, 90.0);
        let paged = pointer(&mut area, PointerPhase::Down, Part::Scrollbar, 150.0);
        assert_eq!(paged.effects, [Effect::ScrollTo { offset: 200 }]);
    }

    #[test]
    fn the_scrollbar_is_in_use_while_scrolling_and_for_a_moment_after() {
        let mut area = area(0.0);
        assert!(!area.in_use());
        let moved = area.send(
            Input::Scrolled {
                part: Part::Viewport,
                offset: 50.0,
                viewport: 200.0,
                content: 1000.0,
                now_ms: 100,
            },
            &mut |_| true,
        );
        assert_eq!(moved.effects, [Effect::Wake { at_ms: 1100 }]);
        assert!(area.in_use());
        area.send(Input::Tick { now_ms: 1100 }, &mut |_| true);
        assert!(!area.in_use());
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(ScrollArea::default, &[]);
    }

    #[test]
    fn a_sideways_area_reads_the_pointer_and_the_track_along_its_own_axis() {
        let mut area = ScrollArea::horizontal();
        assert_eq!(area.scroll_axis(), Orientation::Horizontal);
        area.send(
            Input::Scrolled {
                part: Part::Viewport,
                offset: 0.0,
                viewport: 200.0,
                content: 1000.0,
                now_ms: 0,
            },
            &mut |_| true,
        );
        let wide = |_: Part, _: Option<&str>| Some(Rect::new(0.0, 0.0, 200.0, 10.0));
        let at = |x: f64, phase| Pointer {
            phase,
            part: Part::Thumb,
            item: None,
            at: Point::new(x, 0.0),
            mods: Modifiers::default(),
            now_ms: 0,
        };
        area.pointer(at(10.0, PointerPhase::Down), &wide, &mut |_| true);
        let dragged = area.pointer(at(90.0, PointerPhase::Move), &wide, &mut |_| true);
        assert_eq!(dragged.effects, [Effect::ScrollTo { offset: 400 }]);
    }

    #[test]
    fn a_box_that_sticks_to_the_end_follows_new_content_until_scrolled_away() {
        let mut log = ScrollArea::default().stick_to_end();
        let report = |log: &mut ScrollArea, offset: f64, content: f64| {
            log.send(
                Input::Scrolled {
                    part: Part::Viewport,
                    offset,
                    viewport: 200.0,
                    content,
                    now_ms: 0,
                },
                &mut |_| true,
            )
        };
        report(&mut log, 800.0, 1000.0);
        assert_eq!(
            report(&mut log, 800.0, 1100.0).effects,
            [Effect::ScrollTo { offset: 900 }]
        );
        report(&mut log, 300.0, 1100.0);
        let effects = report(&mut log, 300.0, 1200.0).effects;
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::ScrollTo { .. })),
            "reading further up, so left alone"
        );
    }
}
