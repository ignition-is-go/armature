use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::geometry::{Point, Rect, Size};
use crate::keys::Key;
use crate::motion::{Handle, ResizeLimits, key_delta, moved, resized};
use crate::primitive::{
    Effect, Focusable, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive, Reaction,
    Role, Semantics,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The picture as shown. Its size on screen sets the scale of every drag.
    Area,
    /// The part of the picture that is kept. Drag it to move it, or use the arrows.
    Selection,
    /// A grip on an edge or corner of the selection, keyed by compass point: `n`,
    /// `ne`, `e`, `se`, `s`, `sw`, `w` or `nw`.
    Handle,
}

pub const HANDLES: [&str; 8] = ["n", "ne", "e", "se", "s", "sw", "w", "nw"];
const ACTIONS: &[&str] = &["set", "reset"];
/// The smallest the selection gets, in the picture's own pixels.
const MIN_SIDE: f64 = 8.0;

fn handle(name: &str) -> Option<Handle> {
    let vertical = if name.contains('n') {
        -1
    } else {
        i8::from(name.contains('s'))
    };
    let horizontal = if name.contains('w') {
        -1
    } else {
        i8::from(name.contains('e'))
    };
    HANDLES.contains(&name).then_some(Handle {
        horizontal,
        vertical,
    })
}

/// Chooses a rectangle of a picture to keep. The value is the rectangle as
/// `x,y,width,height` in the picture's own pixels.
#[derive(Clone, Debug)]
pub struct ImageCropper {
    image: Size,
    /// Width over height to hold the selection to, if any.
    aspect: Option<f64>,
    /// The smallest the selection may be, in the picture's own pixels.
    min: Size,
    /// How far the picture is magnified, from 1, and the most it may be.
    zoom: f64,
    max_zoom: f64,
    selection: Rect,
    /// The part being dragged, where the drag began on screen, the selection then,
    /// and how many picture pixels one screen unit is.
    drag: Option<(Option<Handle>, Point, Rect, f64)>,
    focused: Option<String>,
}

impl Default for ImageCropper {
    fn default() -> Self {
        Self::new(
            Size {
                width: 800.0,
                height: 600.0,
            },
            None,
        )
    }
}

impl ImageCropper {
    #[must_use]
    pub fn new(image: Size, aspect: Option<f64>) -> Self {
        let min = Size {
            width: MIN_SIDE,
            height: MIN_SIDE,
        };
        let mut cropper = Self {
            image,
            aspect,
            min,
            zoom: 1.0,
            max_zoom: 4.0,
            selection: Rect::default(),
            drag: None,
            focused: None,
        };
        cropper.selection = cropper.largest();
        cropper
    }

    /// Sets the smallest selection allowed, so what is kept is never too small to use.
    #[must_use]
    pub const fn at_least(mut self, width: f64, height: f64) -> Self {
        self.min = Size {
            width: width.max(MIN_SIDE),
            height: height.max(MIN_SIDE),
        };
        self
    }

    #[must_use]
    pub const fn zoom(&self) -> f64 {
        self.zoom
    }

    /// The part of the picture on show at this magnification, centred on the
    /// selection and kept inside the picture. Draw the picture so this fills the area.
    #[must_use]
    pub fn view(&self) -> Rect {
        let (width, height) = (self.image.width / self.zoom, self.image.height / self.zoom);
        let centre = self.selection.center();
        let x = (centre.x - width / 2.0).clamp(0.0, self.image.width - width);
        let y = (centre.y - height / 2.0).clamp(0.0, self.image.height - height);
        Rect::new(x, y, width, height)
    }

    const fn zoom_to(&mut self, zoom: f64) {
        self.zoom = zoom.clamp(1.0, self.max_zoom);
    }

    /// The selection, in the picture's own pixels.
    #[must_use]
    pub const fn selection(&self) -> Rect {
        self.selection
    }

    /// The selection as fractions of the picture, for placing it over the picture
    /// whatever size that is shown at: left, top, width and height from 0 to 1.
    #[must_use]
    pub fn fractions(&self) -> [f64; 4] {
        let view = self.view();
        [
            (self.selection.x - view.x) / view.width,
            (self.selection.y - view.y) / view.height,
            self.selection.width / view.width,
            self.selection.height / view.height,
        ]
    }

    const fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.image.width, self.image.height)
    }

    /// The biggest selection the picture and the proportions allow, in the middle.
    fn largest(&self) -> Rect {
        let Size { width, height } = self.image;
        let (w, h) = match self.aspect {
            Some(aspect) if width / height > aspect => (height * aspect, height),
            Some(aspect) => (width, width / aspect),
            None => (width, height),
        };
        Rect::new((width - w) / 2.0, (height - h) / 2.0, w, h)
    }

    /// Holds a resized selection to the proportions, keeping the edge opposite the
    /// grip where it was, and inside the picture.
    fn proportioned(&self, rect: Rect, start: Rect, grip: Handle) -> Rect {
        let Some(aspect) = self.aspect else {
            return rect;
        };
        // A top or bottom grip sets the height. Every other grip sets the width.
        let (mut width, mut height) = if grip.horizontal == 0 {
            (rect.height * aspect, rect.height)
        } else {
            (rect.width, rect.width / aspect)
        };
        let room_w = if grip.horizontal < 0 {
            start.x + start.width
        } else {
            self.image.width - start.x
        };
        let room_h = if grip.vertical < 0 {
            start.y + start.height
        } else {
            self.image.height - start.y
        };
        let shrink = (room_w / width).min(room_h / height).min(1.0);
        (width, height) = (width * shrink, height * shrink);
        let x = if grip.horizontal < 0 {
            start.x + start.width - width
        } else {
            start.x
        };
        let y = if grip.vertical < 0 {
            start.y + start.height - height
        } else {
            start.y
        };
        Rect::new(x, y, width, height)
    }

    fn written(rect: Rect) -> Vec<String> {
        vec![format!(
            "{},{},{},{}",
            rect.x.round(),
            rect.y.round(),
            rect.width.round(),
            rect.height.round()
        )]
    }

    fn commit(&mut self, rect: Rect, source: Source, reason: Reason, gate: Gate) -> bool {
        if Self::written(rect) == Self::written(self.selection) {
            self.selection = rect;
            return true;
        }
        let allowed = gate(&ValueChange {
            value: Self::written(rect),
            source,
            reason,
        });
        if allowed {
            self.selection = rect;
        }
        allowed
    }

    const fn limits(&self) -> ResizeLimits {
        ResizeLimits {
            min: self.min,
            max: None,
            bounds: Some(self.bounds()),
        }
    }
}

impl PartOf for Part {
    type Primitive = ImageCropper;
}

impl Primitive for ImageCropper {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Area, Part::Selection, Part::Handle]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn tracks_pointer(part: Part) -> bool {
        matches!(part, Part::Selection | Part::Handle)
    }

    fn wheels(&self, part: Part) -> bool {
        part == Part::Area
    }

    fn set(&mut self, name: &str, value: &str) -> bool {
        match (name, value.parse::<f64>()) {
            ("zoom", Ok(zoom)) => self.zoom_to(zoom),
            _ => return false,
        }
        true
    }

    fn named_part(&self) -> Option<Part> {
        Some(Part::Selection)
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match part {
            Part::Area => Semantics {
                role: Some(Role::Image),
                ..Default::default()
            },
            Part::Selection => Semantics {
                role: Some(Role::Group),
                focusable: Focusable::TabStop,
                active: matches!(self.drag, Some((None, ..))),
                ..Default::default()
            },
            Part::Handle => Semantics {
                role: Some(Role::Button),
                hidden: item.is_some_and(|name| handle(name).is_none()),
                focusable: Focusable::Programmatic,
                active: item.and_then(handle).is_some_and(
                    |grip| matches!(self.drag, Some((Some(held), ..)) if held == grip),
                ),
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Focus { part, item } => {
                self.focused = if part == Part::Handle { item } else { None };
                Reaction::default()
            }
            // The wheel, and plus and minus, magnify the picture under the selection.
            Input::Wheel { delta, .. } => {
                self.zoom_to(self.zoom * if delta > 0.0 { 1.1 } else { 1.0 / 1.1 });
                Reaction::handled()
            }
            Input::Key {
                key: Key::Char(sign @ ('+' | '=' | '-')),
                ..
            } => {
                self.zoom_to(self.zoom * if sign == '-' { 1.0 / 1.25 } else { 1.25 });
                Reaction::handled()
            }
            // Arrows move the selection, or the edge whose grip has focus. Shift goes
            // ten times as far.
            Input::Key { key, mods, .. } => {
                let Some(delta) = key_delta(key, mods, 1.0, 10.0) else {
                    return Reaction::default();
                };
                let next = match self.focused.as_deref().and_then(handle) {
                    Some(grip) => self.proportioned(
                        resized(self.selection, grip, delta, self.limits()),
                        self.selection,
                        grip,
                    ),
                    None => moved(self.selection, delta, Some(self.bounds()), None),
                };
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
        match pointer.phase {
            PointerPhase::Down => {
                let shown = measure(Part::Area, None).map_or(self.image.width, |rect| rect.width);
                let grip = if pointer.part == Part::Handle {
                    pointer.item.as_deref().and_then(handle)
                } else {
                    None
                };
                if pointer.part == Part::Handle && grip.is_none() {
                    return Reaction::default();
                }
                // Magnified, a screen unit covers less of the picture.
                self.drag = Some((
                    grip,
                    pointer.at,
                    self.selection,
                    self.image.width / self.zoom / shown.max(1.0),
                ));
                Reaction::handled().with(Effect::CapturePointer)
            }
            PointerPhase::Move => {
                let Some((grip, from, start, scale)) = self.drag else {
                    return Reaction::default();
                };
                let delta = Point::new(
                    (pointer.at.x - from.x) * scale,
                    (pointer.at.y - from.y) * scale,
                );
                let next = grip.map_or_else(
                    || moved(start, delta, Some(self.bounds()), None),
                    |grip| {
                        self.proportioned(resized(start, grip, delta, self.limits()), start, grip)
                    },
                );
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
        Self::written(self.selection)
    }

    fn set_value(&mut self, value: Vec<String>) {
        let numbers: Vec<f64> = value
            .first()
            .map(|text| {
                text.split(',')
                    .filter_map(|n| n.trim().parse().ok())
                    .collect()
            })
            .unwrap_or_default();
        if let [x, y, width, height] = numbers[..] {
            let width = width.clamp(1.0, self.image.width);
            let height = height.clamp(1.0, self.image.height);
            self.selection = Rect::new(
                x.clamp(0.0, self.image.width - width),
                y.clamp(0.0, self.image.height - height),
                width,
                height,
            );
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "image-cropper",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let rect = match action {
            "reset" => self.largest(),
            "set" => {
                let text = item.ok_or(ActError::MissingItem { action: "set" })?;
                let mut trial = self.clone();
                trial.set_value(vec![text.to_string()]);
                trial.selection
            }
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        if self.commit(rect, Source::Agent, Reason::Action, gate) {
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
    use crate::keys::{Key, Modifiers};

    fn drag(
        cropper: &mut ImageCropper,
        part: Part,
        item: Option<&str>,
        from: (f64, f64),
        to: (f64, f64),
    ) {
        let shown = |_: Part, _: Option<&str>| Some(Rect::new(0.0, 0.0, 400.0, 300.0));
        for (phase, at) in [
            (PointerPhase::Down, from),
            (PointerPhase::Move, to),
            (PointerPhase::Up, to),
        ] {
            let pointer = Pointer {
                phase,
                part,
                item: item.map(str::to_string),
                at: Point::new(at.0, at.1),
                mods: Modifiers::default(),
                now_ms: 0,
            };
            cropper.pointer(pointer, &shown, &mut |_| true);
        }
    }

    #[test]
    fn a_drag_on_screen_moves_the_selection_by_picture_pixels_and_stays_inside() {
        let mut cropper = ImageCropper::default();
        cropper.set_value(vec!["100,100,200,200".into()]);
        // The picture is shown at half size, so 50 on screen is 100 in the picture.
        drag(
            &mut cropper,
            Part::Selection,
            None,
            (100.0, 100.0),
            (150.0, 100.0),
        );
        assert_eq!(cropper.value(), ["200,100,200,200"]);
        drag(
            &mut cropper,
            Part::Selection,
            None,
            (100.0, 100.0),
            (900.0, 900.0),
        );
        assert_eq!(
            cropper.value(),
            ["600,400,200,200"],
            "stopped at the corner"
        );
        assert_eq!(cropper.fractions(), [0.75, 2.0 / 3.0, 0.25, 1.0 / 3.0]);
    }

    #[test]
    fn a_grip_resizes_from_its_own_edge_and_proportions_are_held() {
        let mut free = ImageCropper::default();
        free.set_value(vec!["100,100,200,200".into()]);
        drag(&mut free, Part::Handle, Some("e"), (0.0, 0.0), (50.0, 0.0));
        assert_eq!(free.value(), ["100,100,300,200"]);
        drag(
            &mut free,
            Part::Handle,
            Some("nw"),
            (0.0, 0.0),
            (25.0, 25.0),
        );
        assert_eq!(free.value(), ["150,150,250,150"]);

        let mut square = ImageCropper::new(
            Size {
                width: 800.0,
                height: 600.0,
            },
            Some(1.0),
        );
        assert_eq!(
            square.value(),
            ["100,0,600,600"],
            "the largest square, in the middle"
        );
        square.set_value(vec!["0,0,200,200".into()]);
        drag(
            &mut square,
            Part::Handle,
            Some("se"),
            (0.0, 0.0),
            (100.0, 10.0),
        );
        assert_eq!(
            square.value(),
            ["0,0,400,400"],
            "the width leads and the height follows"
        );
    }

    #[test]
    fn arrows_nudge_the_selection_and_shift_goes_further() {
        let mut cropper = ImageCropper::default();
        cropper.set_value(vec!["100,100,200,200".into()]);
        let key = |cropper: &mut ImageCropper, key, shift| {
            cropper.send(
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
        key(&mut cropper, Key::ArrowRight, false);
        key(&mut cropper, Key::ArrowDown, true);
        assert_eq!(cropper.value(), ["101,110,200,200"]);
        cropper.send(
            Input::Focus {
                part: Part::Handle,
                item: Some("s".into()),
            },
            &mut |_| true,
        );
        key(&mut cropper, Key::ArrowDown, true);
        assert_eq!(cropper.value(), ["101,110,200,210"]);
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(ImageCropper::default, &[]);
    }

    #[test]
    fn the_selection_cannot_be_made_smaller_than_the_least_allowed() {
        let mut cropper = ImageCropper::default().at_least(200.0, 100.0);
        cropper.set_value(vec!["0,0,400,300".into()]);
        drag(
            &mut cropper,
            Part::Handle,
            Some("se"),
            (0.0, 0.0),
            (-390.0, -290.0),
        );
        assert_eq!(cropper.value(), ["0,0,200,100"]);
    }

    #[test]
    fn magnifying_shows_less_of_the_picture_and_drags_cover_less_of_it() {
        let mut cropper = ImageCropper::default();
        cropper.set_value(vec!["300,200,200,200".into()]);
        assert!(cropper.set("zoom", "2"));
        assert_eq!(
            cropper.view(),
            Rect::new(200.0, 150.0, 400.0, 300.0),
            "half the picture, around the selection"
        );
        assert_eq!(cropper.fractions(), [0.25, 1.0 / 6.0, 0.5, 2.0 / 3.0]);
        drag(
            &mut cropper,
            Part::Selection,
            None,
            (100.0, 100.0),
            (150.0, 100.0),
        );
        assert_eq!(
            cropper.value(),
            ["350,200,200,200"],
            "fifty on screen is fifty in the picture at twice the size"
        );
        cropper.send(
            Input::Wheel {
                part: Part::Area,
                item: None,
                delta: -1.0,
                mods: Modifiers::default(),
            },
            &mut |_| true,
        );
        assert!(cropper.zoom() < 2.0);
        cropper.set("zoom", "99");
        assert_eq!(cropper.zoom(), 4.0);
    }
}
