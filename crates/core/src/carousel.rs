use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::carousel_layout::{Align, Geometry, Page, PerPage, SlideBox, carousel_pages, step_page};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::convert::Cast;
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
    /// How many slides show at once. The buttons, keys and drags move a page of
    /// them at a time.
    pub per_page: usize,
    /// Whether the slides run across or down.
    pub orientation: crate::keys::Orientation,
    /// Where a page lines up in the viewport, once the slides have been measured.
    pub align: Align,
    /// Makes a page of as many slides as the viewport shows, whatever their
    /// widths, in place of `per_page`. It takes effect once the slides have been
    /// measured, which a drag does.
    pub fit_pages: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            wrap: false,
            autoplay_ms: None,
            dir: Dir::Ltr,
            per_page: 1,
            orientation: crate::keys::Orientation::Horizontal,
            align: Align::Start,
            fit_pages: false,
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
    /// The pages as last measured, when the slides have been.
    measured: Vec<Page>,
}

impl Carousel {
    #[must_use]
    pub fn new(opts: Options) -> Self {
        Self {
            opts,
            playing: opts.autoplay_ms.is_some(),
            ..Default::default()
        }
    }

    #[must_use]
    pub const fn index(&self) -> usize {
        self.current
    }

    /// The pages worked out from where the slides and the viewport are drawn:
    /// slides of any width, grouped and lined up as the options say. Empty until
    /// both are rendered.
    pub fn measured_pages(&self, measure: Measure<Part>) -> Vec<Page> {
        let down = self.opts.orientation == crate::keys::Orientation::Vertical;
        let along = |rect: crate::geometry::Rect| {
            if down {
                (rect.y, rect.height)
            } else {
                (rect.x, rect.width)
            }
        };
        let Some((_, viewport)) = measure(Part::Viewport, None).map(along) else {
            return Vec::new();
        };
        let boxes: Option<Vec<(f64, f64)>> = self
            .slides
            .iter()
            .map(|slide| measure(Part::Slide, Some(&slide.key)).map(along))
            .collect();
        let Some(boxes) = boxes.filter(|boxes| !boxes.is_empty()) else {
            return Vec::new();
        };
        // Along the track, from its first slide, wherever the track has been moved to.
        let origin = boxes
            .iter()
            .map(|(start, _)| *start)
            .fold(f64::INFINITY, f64::min);
        let slides: Vec<SlideBox> = boxes
            .iter()
            .map(|(start, size)| SlideBox {
                start: start - origin,
                size: *size,
            })
            .collect();
        // Slides drawn on top of one another are not a track that can be read.
        if slides.windows(2).any(|pair| {
            (pair.get(1).copied().unwrap_or_default().start
                - pair.get(0).copied().unwrap_or_default().start)
                .abs()
                < 1.0
        }) {
            return Vec::new();
        }
        let end = slides
            .iter()
            .map(|slide| slide.start + slide.size)
            .fold(0.0, f64::max);
        let geometry = Geometry {
            slides: &slides,
            snapport: viewport,
            padding_start: 0.0,
            max_scroll: (end - viewport).max(0.0),
        };
        let per_page = if self.opts.fit_pages {
            PerPage::Auto
        } else {
            PerPage::Count(self.opts.per_page.max(1))
        };
        carousel_pages(&geometry, self.opts.align, per_page)
    }

    /// How far to move the strip of slides back to show the current page, in the
    /// units the slides are measured in. `None` until they are rendered.
    pub fn offset(&self, measure: Measure<Part>) -> Option<f64> {
        let pages = self.measured_pages(measure);
        pages
            .iter()
            .find(|page| page.slides.contains(&self.current))
            .map(|page| page.position)
    }

    /// The pages as the adapter last measured them: which slides are on each and
    /// how far the strip moves to show it. Empty until the slides are drawn.
    #[must_use]
    pub fn layout(&self) -> &[Page] {
        &self.measured
    }

    /// How far to move the strip to show the current page, from the last
    /// measurement.
    #[must_use]
    pub fn strip_offset(&self) -> Option<f64> {
        self.measured
            .iter()
            .find(|page| page.slides.contains(&self.current))
            .map(|page| page.position)
    }

    #[must_use]
    pub const fn is_playing(&self) -> bool {
        self.playing
    }

    /// Where the strip of slides sits, in slides: the index on show, shifted by a
    /// drag in progress.
    #[must_use]
    pub fn position(&self) -> f64 {
        // A drag across the whole viewport is a whole page.
        let dragged = self.drag.map_or(0.0, |(_, moved, width)| {
            moved / width.max(1.0) * self.opts.per_page.max(1).cast::<f64>()
        });
        let dragged = if self.opts.dir == Dir::Rtl {
            -dragged
        } else {
            dragged
        };
        self.current.cast::<f64>() - dragged
    }

    /// Where the pointer is along the way the slides run.
    fn along(&self, pointer: &Pointer<Part>) -> f64 {
        if self.opts.orientation == crate::keys::Orientation::Vertical {
            pointer.at.y
        } else {
            pointer.at.x
        }
    }

    /// How many pages of slides there are.
    #[must_use]
    pub fn pages(&self) -> usize {
        self.count().div_ceil(self.opts.per_page.max(1)).max(1)
    }

    fn count(&self) -> usize {
        self.slides.iter().count()
    }

    fn target(&self, by: isize) -> Option<usize> {
        let count = self.count().cast::<isize>();
        if count == 0 {
            return None;
        }
        // Once the slides are measured, a step goes to the next page as measured.
        if !self.measured.is_empty() {
            let page = self
                .measured
                .iter()
                .position(|page| page.slides.contains(&self.current))
                .unwrap_or(0);
            let to = (step_page(
                page.saturating_add(1),
                by.signum().cast::<i64>(),
                self.measured.len(),
                self.opts.wrap,
            ))
            .saturating_sub(1);
            return (to != page).then(|| self.measured[to].leader);
        }
        // A step is a page of slides, and the last page never runs past the end.
        let per_page = self.opts.per_page.max(1).cast::<isize>();
        let last = (count.saturating_sub(per_page)).max(0);
        let now = self.current.cast::<isize>();
        let wanted = (now.saturating_add(by.signum().saturating_mul(per_page))).clamp(0, last);
        if wanted != now {
            Some(wanted.cast::<usize>())
        } else if self.opts.wrap && last > 0 {
            Some(if by > 0 { 0 } else { last.cast::<usize>() })
        } else {
            None
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

    const fn running(&self) -> bool {
        self.playing && !self.held.0 && !self.held.1 && self.opts.autoplay_ms.is_some()
    }

    fn said(&self) -> String {
        format!(
            "Slide {} of {}",
            self.current.saturating_add(1),
            self.count()
        )
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
        let (forward, back) = match (self.opts.orientation, self.opts.dir) {
            (crate::keys::Orientation::Vertical, _) => (Key::ArrowDown, Key::ArrowUp),
            (_, Dir::Rtl) => (Key::ArrowLeft, Key::ArrowRight),
            _ => (Key::ArrowRight, Key::ArrowLeft),
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
                let at_ms = now_ms.saturating_add(every);
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

    fn measures(&self) -> bool {
        true
    }

    fn measured(&mut self, measure: Measure<Part>) -> bool {
        let pages = self.measured_pages(measure);
        let changed = pages != self.measured;
        self.measured = pages;
        changed
    }

    fn pointer(
        &mut self,
        pointer: Pointer<Part>,
        measure: Measure<Part>,
        gate: Gate,
    ) -> Reaction<Part> {
        match pointer.phase {
            PointerPhase::Down => {
                let down = self.opts.orientation == crate::keys::Orientation::Vertical;
                let length = measure(Part::Viewport, None)
                    .map_or(1.0, |rect| if down { rect.height } else { rect.width });
                self.drag = Some((self.along(&pointer), 0.0, length));
                self.measured = self.measured_pages(measure);
                self.speed = VelocityTracker::default();
                self.advance_at = None;
                Reaction::default().with(Effect::CapturePointer)
            }
            PointerPhase::Move => {
                let along = self.along(&pointer);
                if let Some((from, moved, _)) = &mut self.drag {
                    *moved = along - *from;
                    self.speed.push(pointer.now_ms.cast::<f64>(), along);
                }
                Reaction::default()
            }
            PointerPhase::Up => {
                let Some((_, moved, width)) = self.drag.take() else {
                    return Reaction::default();
                };
                let velocity = self.speed.velocity_at(pointer.now_ms.cast::<f64>());
                // Dragging towards the start of the line brings the next slide in.
                let far = moved.abs() >= width * DRAG_SHARE || velocity.abs() >= FLICK;
                let towards_start = if velocity.abs() >= FLICK {
                    velocity < 0.0
                } else {
                    moved < 0.0
                };
                let sideways = self.opts.orientation == crate::keys::Orientation::Horizontal;
                let next = towards_start != (sideways && self.opts.dir == Dir::Rtl);
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
    fn measured_slides_of_any_width_make_the_pages() {
        use crate::geometry::{Point, Rect};
        let mut carousel = Carousel::new(Options {
            fit_pages: true,
            ..Default::default()
        });
        // Seven slides 100 wide with a gap of 10, in a viewport 320 wide.
        let keys = ["a", "b", "c", "d", "e", "f", "g"];
        for key in keys {
            carousel.upsert_item(Item {
                key: key.into(),
                label: key.into(),
                disabled: false,
            });
        }
        let measure = |part: Part, item: Option<&str>| match part {
            Part::Viewport => Some(Rect::new(0.0, 0.0, 320.0, 100.0)),
            Part::Slide => keys
                .iter()
                .position(|key| Some(*key) == item)
                .map(|at| Rect::new(at as f64 * 110.0, 0.0, 100.0, 100.0)),
            _ => None,
        };
        let pages = carousel.measured_pages(&measure);
        assert_eq!(
            pages
                .iter()
                .map(|page| page.slides.clone())
                .collect::<Vec<_>>(),
            [vec![0, 1, 2], vec![3, 4, 5], vec![6]]
        );
        assert_eq!(carousel.offset(&measure), Some(0.0));
        // A flick towards the start goes to the next page as measured, not three slides on by count.
        let at = |phase, x: f64, now_ms| Pointer {
            phase,
            part: Part::Viewport,
            item: None,
            at: Point::new(x, 50.0),
            mods: Default::default(),
            now_ms,
        };
        // The adapter reports the layout as soon as it is drawn, before any drag.
        assert!(carousel.measures() && carousel.measured(&measure));
        assert!(
            !carousel.measured(&measure),
            "nothing changed the second time"
        );
        assert_eq!(carousel.layout().len(), 3);
        carousel.pointer(at(PointerPhase::Down, 300.0, 0), &measure, &mut |_| true);
        carousel.pointer(at(PointerPhase::Move, 100.0, 16), &measure, &mut |_| true);
        carousel.pointer(at(PointerPhase::Up, 100.0, 32), &measure, &mut |_| true);
        assert_eq!(carousel.value(), ["d"]);
        assert_eq!(carousel.offset(&measure), Some(330.0));
        // The buttons now step by measured pages too, and stop at the last one.
        carousel.act("next", None, &mut |_| true).unwrap();
        assert_eq!(carousel.value(), ["g"]);
        let _ = carousel.act("next", None, &mut |_| true);
        assert_eq!(carousel.value(), ["g"]);
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

    #[test]
    fn with_several_slides_showing_a_step_is_a_page_and_stops_at_the_last_full_one() {
        let mut carousel = Carousel::new(Options {
            per_page: 2,
            ..Default::default()
        });
        for key in ["a", "b", "c", "d", "e"] {
            carousel.upsert_item(Item {
                key: key.into(),
                label: key.into(),
                disabled: false,
            });
        }
        assert_eq!(carousel.pages(), 3);
        press(&mut carousel, Part::Next);
        assert_eq!(carousel.value(), ["c"]);
        press(&mut carousel, Part::Next);
        assert_eq!(carousel.value(), ["d"], "the last two slides fill the view");
        assert!(carousel.semantics(Part::Next, None).disabled);
    }

    #[test]
    fn slides_that_run_down_answer_the_up_and_down_arrows() {
        let mut carousel = carousel(Options {
            orientation: crate::keys::Orientation::Vertical,
            ..Default::default()
        });
        carousel.send(
            Input::Key {
                key: Key::ArrowDown,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        );
        assert_eq!(carousel.index(), 1);
        let across = carousel.send(
            Input::Key {
                key: Key::ArrowRight,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        );
        assert!(!across.handled);
    }
}
