use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::gesture::VelocityTracker;
use crate::items::Items;
use crate::keys::{Dir, Key};
use crate::primitive::{
    Effect, Focusable, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive, Reaction,
    Role, Semantics,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    /// The window the slides pass through. Dragging it sideways changes slide.
    Viewport,
    /// One slide, rendered per item. Move the strip of slides by `position`.
    Slide,
    Previous,
    Next,
    /// One dot per slide, keyed like its slide.
    Indicator,
    /// Starts and stops the slides advancing on their own.
    Play,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Whether going past the last slide comes round to the first.
    pub wrap: bool,
    /// How long each slide shows when advancing on its own. `None` never advances.
    pub autoplay_ms: Option<u64>,
    pub dir: Dir,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            wrap: false,
            autoplay_ms: None,
            dir: Dir::Ltr,
        }
    }
}

const ACTIONS: &[&str] = &["go_to", "next", "previous", "play", "pause"];
/// How far across the viewport a drag must go to change slide.
const DRAG_SHARE: f64 = 0.25;
/// How fast a flick must be, in units per second, to change slide whatever its length.
const FLICK: f64 = 300.0;

/// Slides shown one at a time, with buttons, dots, the arrow keys and dragging to
/// move between them. It can advance on its own, and stops while the pointer or
/// focus is on it. The value is the key of the slide on show.
#[derive(Clone, Debug, Default)]
pub struct Carousel {
    opts: Options,
    slides: Items,
    current: usize,
    playing: bool,
    /// Whether the pointer or focus is holding the slides still.
    held: (bool, bool),
    advance_at: Option<u64>,
    focused: Option<(Part, Option<String>)>,
    /// Where a drag began, how far it has gone, and how wide the viewport is.
    drag: Option<(f64, f64, f64)>,
    speed: VelocityTracker,
}

impl Carousel {
    pub fn new(opts: Options) -> Self {
        Carousel {
            opts,
            playing: opts.autoplay_ms.is_some(),
            ..Default::default()
        }
    }

    pub fn index(&self) -> usize {
        self.current
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Where the strip of slides sits, in slides: the index on show, shifted by a
    /// drag in progress.
    pub fn position(&self) -> f64 {
        let dragged = self
            .drag
            .map_or(0.0, |(_, moved, width)| moved / width.max(1.0));
        let dragged = if self.opts.dir == Dir::Rtl {
            -dragged
        } else {
            dragged
        };
        self.current as f64 - dragged
    }

    fn count(&self) -> usize {
        self.slides.iter().count()
    }

    fn target(&self, by: isize) -> Option<usize> {
        let count = self.count() as isize;
        if count == 0 {
            return None;
        }
        let wanted = self.current as isize + by;
        if self.opts.wrap {
            Some(wanted.rem_euclid(count) as usize)
        } else {
            (0..count).contains(&wanted).then_some(wanted as usize)
        }
    }

    fn go(&mut self, index: usize, source: Source, reason: Reason, gate: Gate) -> bool {
        if index == self.current || index >= self.count() {
            return false;
        }
        let allowed = gate(&ValueChange {
            value: vec![self.slides.key_at(index)],
            source,
            reason,
        });
        if allowed {
            self.current = index;
        }
        allowed
    }

    fn step(&mut self, by: isize, reason: Reason, gate: Gate) -> Reaction<Part> {
        if let Some(index) = self.target(by) {
            self.go(index, Source::User, reason, gate);
        }
        // Taking over by hand puts the next automatic advance off.
        self.advance_at = None;
        Reaction::handled()
    }

    fn running(&self) -> bool {
        self.playing && !self.held.0 && !self.held.1 && self.opts.autoplay_ms.is_some()
    }

    fn said(&self) -> String {
        format!("Slide {} of {}", self.current + 1, self.count())
    }
}

impl PartOf for Part {
    type Primitive = Carousel;
}

impl Primitive for Carousel {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[
            Part::Root,
            Part::Viewport,
            Part::Slide,
            Part::Previous,
            Part::Next,
            Part::Indicator,
            Part::Play,
        ]
    }

    fn is_item(part: Part) -> bool {
        part == Part::Slide
    }

    fn tracks_pointer(part: Part) -> bool {
        part == Part::Viewport
    }

    fn hovers(part: Part) -> bool {
        part == Part::Root
    }

    fn named_part(&self) -> Option<Part> {
        Some(Part::Root)
    }

    fn wake_after(&self) -> Option<u64> {
        (self.running() && self.advance_at.is_none() && self.count() > 1).then_some(0)
    }

    fn upsert_item(&mut self, item: Item) {
        self.slides.upsert(item);
    }

    fn remove_item(&mut self, key: &str) {
        self.slides.remove(key);
        self.current = self.current.min(self.count().saturating_sub(1));
    }

    fn set_order(&mut self, rendered: &[(String, String)]) {
        self.slides.set_order(rendered);
    }

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        let at = item.and_then(|key| self.slides.index_of(key));
        let button = |disabled: bool| Semantics {
            role: Some(Role::Button),
            disabled,
            focusable: if disabled {
                Focusable::No
            } else {
                Focusable::TabStop
            },
            ..Default::default()
        };
        match part {
            Part::Root => Semantics {
                role: Some(Role::Region),
                ..Default::default()
            },
            Part::Viewport => Semantics {
                active: self.drag.is_some(),
                ..Default::default()
            },
            Part::Slide => Semantics {
                role: Some(Role::Group),
                current: at == Some(self.current),
                active: at == Some(self.current),
                ..Default::default()
            },
            Part::Previous => button(self.target(-1).is_none()),
            Part::Next => button(self.target(1).is_none()),
            Part::Indicator => Semantics {
                current: at == Some(self.current),
                active: at == Some(self.current),
                hidden: at.is_none() && item.is_some(),
                ..button(false)
            },
            Part::Play => Semantics {
                pressed: Some(self.playing),
                hidden: self.opts.autoplay_ms.is_none(),
                ..button(self.opts.autoplay_ms.is_none())
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        let (forward, back) = if self.opts.dir == Dir::Rtl {
            (Key::ArrowLeft, Key::ArrowRight)
        } else {
            (Key::ArrowRight, Key::ArrowLeft)
        };
        match input {
            Input::Focus { part, item } => {
                self.focused = Some((part, item));
                self.held.1 = true;
                self.advance_at = None;
                Reaction::default()
            }
            Input::Blur { .. } => {
                self.focused = None;
                self.held.1 = false;
                Reaction::default()
            }
            Input::Hover {
                part: Part::Root,
                inside,
                ..
            } => {
                self.held.0 = inside;
                self.advance_at = None;
                Reaction::default()
            }
            Input::Tick { now_ms } => {
                let Some(every) = self.opts.autoplay_ms.filter(|_| self.running()) else {
                    return Reaction::default();
                };
                if self.advance_at.is_some_and(|at| now_ms >= at)
                    && let Some(index) = self.target(1).or(Some(0))
                {
                    self.go(index, Source::User, Reason::Timer, gate);
                }
                let at_ms = now_ms + every;
                self.advance_at = Some(at_ms);
                Reaction::default().with(Effect::Wake { at_ms })
            }
            Input::Press {
                part: Part::Previous,
                ..
            } => self.step(-1, Reason::Press, gate),
            Input::Press {
                part: Part::Next, ..
            } => self.step(1, Reason::Press, gate),
            Input::Press {
                part: Part::Indicator,
                item: Some(key),
                ..
            } => {
                if let Some(index) = self.slides.index_of(&key) {
                    self.go(index, Source::User, Reason::Press, gate);
                }
                Reaction::handled().announcing(self.said())
            }
            Input::Press {
                part: Part::Play, ..
            } => {
                self.playing = !self.playing;
                self.advance_at = None;
                Reaction::handled()
            }
            Input::Key {
                key: Key::Enter | Key::Space,
                mods,
                ..
            } => match self.focused.clone() {
                Some((
                    part @ (Part::Previous | Part::Next | Part::Indicator | Part::Play),
                    item,
                )) => self.send(Input::Press { part, item, mods }, gate),
                _ => Reaction::default(),
            },
            Input::Key { key, .. } if key == forward => {
                self.step(1, Reason::Keyboard, gate).announcing(self.said())
            }
            Input::Key { key, .. } if key == back => self
                .step(-1, Reason::Keyboard, gate)
                .announcing(self.said()),
            Input::Key { key: Key::Home, .. } => {
                self.go(0, Source::User, Reason::Keyboard, gate);
                Reaction::handled().announcing(self.said())
            }
            Input::Key { key: Key::End, .. } => {
                self.go(
                    self.count().saturating_sub(1),
                    Source::User,
                    Reason::Keyboard,
                    gate,
                );
                Reaction::handled().announcing(self.said())
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
                let width = measure(Part::Viewport, None).map_or(1.0, |rect| rect.width);
                self.drag = Some((pointer.at.x, 0.0, width));
                self.speed = VelocityTracker::default();
                self.advance_at = None;
                Reaction::default().with(Effect::CapturePointer)
            }
            PointerPhase::Move => {
                if let Some((from, moved, _)) = &mut self.drag {
                    *moved = pointer.at.x - *from;
                    self.speed.push(pointer.now_ms as f64, pointer.at.x);
                }
                Reaction::default()
            }
            PointerPhase::Up => {
                let Some((_, moved, width)) = self.drag.take() else {
                    return Reaction::default();
                };
                let velocity = self.speed.velocity_at(pointer.now_ms as f64);
                // Dragging towards the start of the line brings the next slide in.
                let far = moved.abs() >= width * DRAG_SHARE || velocity.abs() >= FLICK;
                let towards_start = if velocity.abs() >= FLICK {
                    velocity < 0.0
                } else {
                    moved < 0.0
                };
                let next = towards_start != (self.opts.dir == Dir::Rtl);
                if far && let Some(index) = self.target(if next { 1 } else { -1 }) {
                    self.go(index, Source::User, Reason::Drag, gate);
                }
                Reaction::default()
            }
            PointerPhase::Cancel => {
                self.drag = None;
                Reaction::default()
            }
        }
    }

    fn value(&self) -> Vec<String> {
        if self.count() == 0 {
            Vec::new()
        } else {
            vec![self.slides.key_at(self.current)]
        }
    }

    fn set_value(&mut self, value: Vec<String>) {
        if let Some(index) = value.first().and_then(|key| self.slides.index_of(key)) {
            self.current = index;
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .slides
            .iter()
            .enumerate()
            .map(|(at, item)| SnapshotItem {
                key: item.key.clone(),
                label: item.label.clone(),
                disabled: false,
                selected: at == self.current,
            })
            .collect();
        Snapshot {
            name: "carousel",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let index = match action {
            "play" | "pause" => {
                self.playing = action == "play" && self.opts.autoplay_ms.is_some();
                self.advance_at = None;
                return Ok(());
            }
            "next" => self.target(1),
            "previous" => self.target(-1),
            "go_to" => {
                let key = item.ok_or(ActError::MissingItem { action: "go_to" })?;
                Some(
                    self.slides
                        .index_of(key)
                        .ok_or_else(|| ActError::UnknownItem(key.to_string()))?,
                )
            }
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        match index {
            Some(index)
                if index == self.current || self.go(index, Source::Agent, Reason::Action, gate) =>
            {
                Ok(())
            }
            _ => Err(ActError::Refused),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Rect};
    use crate::keys::Modifiers;

    fn carousel(opts: Options) -> Carousel {
        let mut carousel = Carousel::new(opts);
        for key in ["one", "two", "three"] {
            carousel.upsert_item(Item {
                key: key.into(),
                label: key.into(),
                disabled: false,
            });
        }
        carousel
    }

    fn press(carousel: &mut Carousel, part: Part) {
        carousel.send(
            Input::Press {
                part,
                item: None,
                mods: Modifiers::default(),
            },
            &mut |_| true,
        );
    }

    #[test]
    fn the_buttons_stop_at_the_ends_unless_the_slides_come_round() {
        let mut carousel = carousel(Options::default());
        assert!(carousel.semantics(Part::Previous, None).disabled);
        press(&mut carousel, Part::Next);
        press(&mut carousel, Part::Next);
        press(&mut carousel, Part::Next);
        assert_eq!(carousel.value(), ["three"]);
        assert!(carousel.semantics(Part::Next, None).disabled);
        let mut round = self::carousel(Options {
            wrap: true,
            ..Default::default()
        });
        press(&mut round, Part::Previous);
        assert_eq!(round.value(), ["three"]);
    }

    #[test]
    fn dragging_a_quarter_of_the_way_or_flicking_changes_slide() {
        let mut carousel = carousel(Options::default());
        let drag = |carousel: &mut Carousel, phase, x: f64, now_ms: u64| {
            let pointer = Pointer {
                phase,
                part: Part::Viewport,
                item: None,
                at: Point::new(x, 0.0),
                mods: Modifiers::default(),
                now_ms,
            };
            carousel.pointer(
                pointer,
                &|_, _| Some(Rect::new(0.0, 0.0, 400.0, 200.0)),
                &mut |_| true,
            );
        };
        drag(&mut carousel, PointerPhase::Down, 300.0, 0);
        drag(&mut carousel, PointerPhase::Move, 250.0, 500);
        assert_eq!(carousel.position(), 0.125, "the strip follows the finger");
        drag(&mut carousel, PointerPhase::Up, 250.0, 1000);
        assert_eq!(carousel.index(), 0, "not far enough, and not fast");
        drag(&mut carousel, PointerPhase::Down, 300.0, 2000);
        drag(&mut carousel, PointerPhase::Move, 180.0, 2500);
        drag(&mut carousel, PointerPhase::Up, 180.0, 3000);
        assert_eq!(carousel.index(), 1);
        drag(&mut carousel, PointerPhase::Down, 300.0, 4000);
        drag(&mut carousel, PointerPhase::Move, 290.0, 4010);
        drag(&mut carousel, PointerPhase::Move, 270.0, 4020);
        drag(&mut carousel, PointerPhase::Up, 270.0, 4025);
        assert_eq!(carousel.index(), 2, "a short flick is enough");
    }

    #[test]
    fn it_advances_on_its_own_and_stops_while_the_pointer_is_on_it() {
        let mut carousel = carousel(Options {
            autoplay_ms: Some(3000),
            wrap: true,
            ..Default::default()
        });
        assert_eq!(carousel.wake_after(), Some(0));
        let tick =
            |carousel: &mut Carousel, now_ms| carousel.send(Input::Tick { now_ms }, &mut |_| true);
        assert_eq!(
            tick(&mut carousel, 0).effects,
            [Effect::Wake { at_ms: 3000 }]
        );
        tick(&mut carousel, 3000);
        assert_eq!(carousel.index(), 1);
        carousel.send(
            Input::Hover {
                part: Part::Root,
                item: None,
                inside: true,
                now_ms: 3500,
            },
            &mut |_| true,
        );
        tick(&mut carousel, 6000);
        assert_eq!(carousel.index(), 1, "held still");
        carousel.send(
            Input::Hover {
                part: Part::Root,
                item: None,
                inside: false,
                now_ms: 7000,
            },
            &mut |_| true,
        );
        assert_eq!(carousel.wake_after(), Some(0), "the clock starts again");
        tick(&mut carousel, 7000);
        tick(&mut carousel, 10_000);
        assert_eq!(carousel.index(), 2);
    }

    #[test]
    fn it_conforms() {
        let items: Vec<Item> = ["a", "b", "c"]
            .iter()
            .map(|k| Item {
                key: k.to_string(),
                label: k.to_string(),
                disabled: false,
            })
            .collect();
        crate::testing::assert_conforms(Carousel::default, &items);
        crate::testing::assert_conforms(
            || {
                Carousel::new(Options {
                    autoplay_ms: Some(1000),
                    wrap: true,
                    ..Default::default()
                })
            },
            &items,
        );
    }
}
