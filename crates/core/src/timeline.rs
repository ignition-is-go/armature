use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::Key;
use crate::primitive::{
    Effect, Focusable, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive, Reaction,
    Role, Semantics,
};
use crate::spans::{Span, SpanEdge, draw_span, move_span, resize_span, span_message, stack_spans};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The length spans sit on. Dragging on it draws a new span.
    Track,
    /// One span. Dragging it slides it along the track.
    Span,
    /// The trailing edge of a span. Dragging it changes the span's length.
    SpanEnd,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// What the two ends of the track stand for, such as 0 and 24 hours.
    pub range: Span,
    /// Spans snap to multiples of this.
    pub grid: f64,
    pub min_length: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            range: Span {
                start: 0.0,
                end: 24.0,
            },
            grid: 1.0,
            min_length: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Gesture {
    Draw {
        key: String,
        from: f64,
    },
    Slide {
        key: String,
        origin: Span,
        grabbed_at: f64,
    },
    Stretch {
        key: String,
    },
}

const ACTIONS: &[&str] = &["add", "remove", "clear"];

/// Ranges on a track, such as events in a day. Drag on the track to draw one, drag a
/// span to move it, drag its end to resize it. The value lists each as
/// `key:start-end`.
#[derive(Clone, Debug, Default)]
pub struct Timeline {
    opts: Options,
    spans: Vec<(String, Span)>,
    next_id: u32,
    gesture: Option<Gesture>,
    /// The spans as they were when the gesture began, to put back if it is refused.
    before: Vec<(String, Span)>,
    focused: Option<String>,
}

fn written(spans: &[(String, Span)]) -> Vec<String> {
    spans
        .iter()
        .map(|(key, span)| format!("{key}:{}-{}", span.start, span.end))
        .collect()
}

fn read(entry: &str) -> Option<(String, Span)> {
    let (key, range) = entry.split_once(':')?;
    let (start, end) = range.split_once('-')?;
    Some((
        key.to_string(),
        Span {
            start: start.parse().ok()?,
            end: end.parse().ok()?,
        },
    ))
}

impl Timeline {
    #[must_use]
    pub fn new(opts: Options) -> Self {
        Self {
            opts,
            ..Default::default()
        }
    }

    #[must_use]
    pub fn spans(&self) -> &[(String, Span)] {
        &self.spans
    }

    /// The lane each span sits in so that overlapping spans do not cover each other,
    /// in the same order as `spans`.
    #[must_use]
    pub fn lanes(&self) -> Vec<usize> {
        let spans: Vec<Span> = self.spans.iter().map(|(_, span)| *span).collect();
        stack_spans(&spans)
    }

    /// Where a span sits along the track, as fractions of its length.
    #[must_use]
    pub fn position_of(&self, span: Span) -> (f64, f64) {
        let length = (self.opts.range.end - self.opts.range.start).max(f64::MIN_POSITIVE);
        (
            (span.start - self.opts.range.start) / length,
            (span.end - self.opts.range.start) / length,
        )
    }

    fn get(&self, key: &str) -> Option<Span> {
        self.spans
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, span)| *span)
    }

    fn put(&mut self, key: &str, span: Span) {
        let held = Span {
            start: span.start.max(self.opts.range.start),
            end: span.end.min(self.opts.range.end),
        };
        if let Some((_, span)) = self.spans.iter_mut().find(|(k, _)| k == key) {
            *span = held;
        }
    }

    fn value_at(&self, pointer: &Pointer<Part>, measure: Measure<Part>) -> Option<f64> {
        let track = measure(Part::Track, None)?;
        let along = ((pointer.at.x - track.x) / track.width.max(f64::MIN_POSITIVE)).clamp(0.0, 1.0);
        Some(along.mul_add(
            self.opts.range.end - self.opts.range.start,
            self.opts.range.start,
        ))
    }

    /// Asks the owner about the spans as they now stand. A refusal puts back `before`.
    fn settle(
        &mut self,
        before: Vec<(String, Span)>,
        source: Source,
        reason: Reason,
        gate: Gate,
    ) -> bool {
        if self.spans == before {
            return true;
        }
        let allowed = gate(&ValueChange {
            value: written(&self.spans),
            source,
            reason,
        });
        if !allowed {
            self.spans = before;
        }
        allowed
    }

    fn edit(&mut self, key: &str, span: Span, reason: Reason, gate: Gate) -> Reaction<Part> {
        let before = self.spans.clone();
        self.put(key, span);
        if self.settle(before, Source::User, reason, gate) {
            Reaction::handled().announcing(span_message(key, self.get(key).unwrap_or(span)))
        } else {
            Reaction::handled()
        }
    }
}

impl PartOf for Part {
    type Primitive = Timeline;
}

impl Primitive for Timeline {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Track, Part::Span, Part::SpanEnd]
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
        let exists = item.is_some_and(|key| self.get(key).is_some());
        match part {
            Part::Track => Semantics {
                role: Some(Role::List),
                ..Default::default()
            },
            Part::Span => {
                let stop = self
                    .focused
                    .as_deref()
                    .filter(|key| self.get(key).is_some())
                    .or_else(|| self.spans.first().map(|(key, _)| key.as_str()));
                Semantics {
                    role: Some(Role::ListItem),
                    hidden: !exists,
                    focusable: if exists && stop == item {
                        Focusable::TabStop
                    } else if exists {
                        Focusable::Programmatic
                    } else {
                        Focusable::No
                    },
                    active: exists && self.focused.as_deref() == item,
                    ..Default::default()
                }
            }
            Part::SpanEnd => Semantics {
                hidden: !exists,
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Focus {
                part: Part::Span,
                item,
            } => {
                self.focused = item;
                Reaction::default()
            }
            Input::Key { key, mods, .. } => {
                let Some(focused) = self.focused.clone() else {
                    return Reaction::default();
                };
                let Some(span) = self.get(&focused) else {
                    return Reaction::default();
                };
                let step = self.opts.grid.max(f64::MIN_POSITIVE);
                let by = match key {
                    Key::ArrowRight => step,
                    Key::ArrowLeft => -step,
                    Key::Backspace => {
                        let before = self.spans.clone();
                        self.spans.retain(|(key, _)| *key != focused);
                        self.settle(before, Source::User, Reason::Keyboard, gate);
                        return Reaction::handled();
                    }
                    _ => return Reaction::default(),
                };
                // Shift moves the end alone, which changes the length.
                let moved = if mods.shift {
                    resize_span(
                        span,
                        SpanEdge::End,
                        span.end + by,
                        self.opts.min_length,
                        Some(self.opts.grid),
                    )
                } else {
                    move_span(span, by, self.opts.range, Some(self.opts.grid))
                };
                self.edit(&focused, moved, Reason::Keyboard, gate)
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
        let at = self.value_at(&pointer, measure);
        match (pointer.phase, at) {
            (PointerPhase::Down, Some(at)) => {
                self.before = self.spans.clone();
                self.gesture = match (
                    pointer.part,
                    pointer
                        .item
                        .as_deref()
                        .and_then(|key| Some((key, self.get(key)?))),
                ) {
                    (Part::Span, Some((key, origin))) => Some(Gesture::Slide {
                        key: key.to_string(),
                        origin,
                        grabbed_at: at,
                    }),
                    (Part::SpanEnd, Some((key, _))) => Some(Gesture::Stretch {
                        key: key.to_string(),
                    }),
                    (Part::Track, _) => {
                        self.next_id = self.next_id.saturating_add(1);
                        let key = format!("span-{}", self.next_id);
                        let span = draw_span(at, at, self.opts.min_length, Some(self.opts.grid));
                        self.spans.push((key.clone(), span));
                        self.put(&key, span);
                        Some(Gesture::Draw { key, from: at })
                    }
                    _ => None,
                };
                Reaction::handled().with(Effect::CapturePointer)
            }
            (PointerPhase::Move, Some(at)) => {
                let (grid, min) = (Some(self.opts.grid), self.opts.min_length);
                match self.gesture.clone() {
                    Some(Gesture::Draw { key, from }) => {
                        self.put(&key, draw_span(from, at, min, grid));
                    }
                    Some(Gesture::Slide {
                        key,
                        origin,
                        grabbed_at,
                    }) => {
                        self.put(
                            &key,
                            move_span(origin, at - grabbed_at, self.opts.range, grid),
                        );
                    }
                    Some(Gesture::Stretch { key }) => {
                        if let Some(span) = self.get(&key) {
                            self.put(&key, resize_span(span, SpanEdge::End, at, min, grid));
                        }
                    }
                    None => return Reaction::default(),
                }
                Reaction::handled()
            }
            (PointerPhase::Up, _) => {
                let Some(gesture) = self.gesture.take() else {
                    return Reaction::default();
                };
                let before = std::mem::take(&mut self.before);
                let (Gesture::Draw { key, .. }
                | Gesture::Slide { key, .. }
                | Gesture::Stretch { key }) = gesture;
                if self.settle(before, Source::User, Reason::Drag, gate) {
                    self.focused = Some(key.clone());
                    Reaction::item(Part::Span, true, Some(key))
                } else {
                    Reaction::handled()
                }
            }
            (PointerPhase::Cancel, _) => {
                if self.gesture.take().is_some() {
                    self.spans = std::mem::take(&mut self.before);
                }
                Reaction::handled()
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        written(&self.spans)
    }

    fn set_value(&mut self, value: Vec<String>) {
        self.spans = value.iter().filter_map(|entry| read(entry)).collect();
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .spans
            .iter()
            .map(|(key, span)| SnapshotItem {
                key: key.clone(),
                label: format!("{} to {}", span.start, span.end),
                disabled: false,
                selected: false,
            })
            .collect();
        Snapshot {
            name: "timeline",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let before = self.spans.clone();
        match action {
            "clear" => self.spans.clear(),
            "remove" => {
                let key = item.ok_or(ActError::MissingItem { action: "remove" })?;
                if self.get(key).is_none() {
                    return Err(ActError::UnknownItem(key.to_string()));
                }
                self.spans.retain(|(k, _)| k != key);
            }
            "add" => {
                let text = item.ok_or(ActError::MissingItem { action: "add" })?;
                let (_, span) = read(&format!("new:{text}"))
                    .ok_or_else(|| ActError::UnknownItem(text.to_string()))?;
                self.next_id = self.next_id.saturating_add(1);
                let key = format!("span-{}", self.next_id);
                self.spans.push((key.clone(), span));
                self.put(
                    &key,
                    draw_span(
                        span.start,
                        span.end,
                        self.opts.min_length,
                        Some(self.opts.grid),
                    ),
                );
            }
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        }
        if self.settle(before, Source::Agent, Reason::Action, gate) {
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

    /// A 240-wide track standing for 24 hours, so ten units across is one hour.
    fn track(part: Part, _: Option<&str>) -> Option<Rect> {
        (part == Part::Track).then_some(Rect::new(0.0, 0.0, 240.0, 40.0))
    }

    fn pointer(
        timeline: &mut Timeline,
        phase: PointerPhase,
        part: Part,
        item: Option<&str>,
        x: f64,
        gate: Gate,
    ) -> Reaction<Part> {
        let pointer = Pointer {
            phase,
            part,
            item: item.map(str::to_string),
            at: Point::new(x, 10.0),
            mods: Modifiers::default(),
            now_ms: 0,
        };
        timeline.pointer(pointer, &track, gate)
    }

    fn drawn(from: f64, to: f64) -> Timeline {
        let mut timeline = Timeline::default();
        pointer(
            &mut timeline,
            PointerPhase::Down,
            Part::Track,
            None,
            from,
            &mut |_| true,
        );
        pointer(
            &mut timeline,
            PointerPhase::Move,
            Part::Track,
            None,
            to,
            &mut |_| true,
        );
        pointer(
            &mut timeline,
            PointerPhase::Up,
            Part::Track,
            None,
            to,
            &mut |_| true,
        );
        timeline
    }

    #[test]
    fn dragging_on_the_track_draws_a_span_snapped_to_the_hour() {
        let timeline = drawn(92.0, 128.0);
        assert_eq!(timeline.value(), ["span-1:9-13"]);
        assert_eq!(
            timeline.position_of(timeline.spans()[0].1),
            (9.0 / 24.0, 13.0 / 24.0)
        );
    }

    #[test]
    fn a_span_slides_with_its_length_kept_and_stays_on_the_track() {
        let mut timeline = drawn(90.0, 130.0);
        pointer(
            &mut timeline,
            PointerPhase::Down,
            Part::Span,
            Some("span-1"),
            100.0,
            &mut |_| true,
        );
        pointer(
            &mut timeline,
            PointerPhase::Move,
            Part::Span,
            Some("span-1"),
            5000.0,
            &mut |_| true,
        );
        pointer(
            &mut timeline,
            PointerPhase::Up,
            Part::Span,
            Some("span-1"),
            5000.0,
            &mut |_| true,
        );
        assert_eq!(timeline.value(), ["span-1:20-24"]);
    }

    #[test]
    fn dragging_the_end_changes_the_length_but_not_below_the_minimum() {
        let mut timeline = drawn(90.0, 130.0);
        pointer(
            &mut timeline,
            PointerPhase::Down,
            Part::SpanEnd,
            Some("span-1"),
            130.0,
            &mut |_| true,
        );
        pointer(
            &mut timeline,
            PointerPhase::Move,
            Part::SpanEnd,
            Some("span-1"),
            0.0,
            &mut |_| true,
        );
        pointer(
            &mut timeline,
            PointerPhase::Up,
            Part::SpanEnd,
            Some("span-1"),
            0.0,
            &mut |_| true,
        );
        assert_eq!(timeline.value(), ["span-1:9-10"]);
    }

    #[test]
    fn overlapping_spans_are_given_separate_lanes() {
        let mut timeline = drawn(90.0, 130.0);
        timeline.act("add", Some("11-15"), &mut |_| true).unwrap();
        timeline.act("add", Some("14-16"), &mut |_| true).unwrap();
        assert_eq!(timeline.lanes(), [0, 1, 0]);
    }

    #[test]
    fn a_refused_gesture_puts_everything_back() {
        let mut timeline = drawn(90.0, 130.0);
        pointer(
            &mut timeline,
            PointerPhase::Down,
            Part::Track,
            None,
            10.0,
            &mut |_| false,
        );
        pointer(
            &mut timeline,
            PointerPhase::Move,
            Part::Track,
            None,
            40.0,
            &mut |_| false,
        );
        assert_eq!(timeline.spans().len(), 2, "shown while it is being drawn");
        pointer(
            &mut timeline,
            PointerPhase::Up,
            Part::Track,
            None,
            40.0,
            &mut |_| false,
        );
        assert_eq!(timeline.value(), ["span-1:9-13"]);
    }

    #[test]
    fn arrows_slide_the_focused_span_and_shift_resizes_it() {
        let mut timeline = drawn(90.0, 130.0);
        timeline.send(
            Input::Focus {
                part: Part::Span,
                item: Some("span-1".into()),
            },
            &mut |_| true,
        );
        let key = |timeline: &mut Timeline, key, shift| {
            timeline.send(
                Input::Key {
                    key,
                    mods: Modifiers {
                        shift,
                        ..Default::default()
                    },
                    now_ms: 0,
                },
                &mut |_| true,
            )
        };
        key(&mut timeline, Key::ArrowLeft, false);
        let said = key(&mut timeline, Key::ArrowRight, true);
        assert_eq!(timeline.value(), ["span-1:8-13"]);
        assert_eq!(
            said.effects,
            [Effect::Announce {
                message: "span-1 now runs from 8 to 13.".into(),
                assertive: false
            }]
        );
        key(&mut timeline, Key::Backspace, false);
        assert_eq!(timeline.value(), Vec::<String>::new());
    }

    #[test]
    fn it_conforms() {
        let spans: Vec<Item> = std::iter::once(&"span-1")
            .map(|key| Item {
                key: key.to_string(),
                label: key.to_string(),
                disabled: false,
            })
            .collect();
        crate::testing::assert_conforms(|| drawn(90.0, 130.0), &spans);
    }
}
