//! Renders any `armature_core` primitive in GPUI. Nothing here knows about a
//! particular primitive.

pub mod agent;

use armature_core::convert::Cast;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use armature_core::agent::{Access, ActError, Target};
use armature_core::change::ValueChange;
use armature_core::geometry::{Point as CorePoint, Rect};
use armature_core::keys::{Key, Modifiers, Orientation};
use armature_core::layers::{Layer, LayerStack};
use armature_core::popup::{Align, Side};
use armature_core::primitive::{
    Effect, FileInfo, Focusable, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive,
    Reaction, Role, Semantics,
};
use armature_core::safe_area::{Axis, SafeArea};
use armature_core::shortcut::Shortcut;
use armature_core::text_edit::{Edit, TextEdit};
use gpui::prelude::*;
use gpui::{
    A11ySubtreeBuilder, Anchor, AnyElement, App, Bounds, ClickEvent, ClipboardItem, Context,
    DispatchPhase, Div, ElementId, Empty, Entity, EventEmitter, FocusHandle, GlobalElementId,
    InspectorElementId, Interactivity, KeyDownEvent, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollHandle, SharedString, Stateful,
    StyleRefinement, Toggled, Window, anchored, canvas, deferred, div, point, px,
};

pub use armature_core::change::{Reason, Source};

/// What the primitive is told about files chosen or dropped: name, size and where
/// they are. The media type is not known here, so it is left empty and a primitive
/// goes by the extension.
fn described(paths: &[std::path::PathBuf]) -> Vec<FileInfo> {
    paths
        .iter()
        .map(|path| FileInfo {
            name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            kind: String::new(),
            size: std::fs::metadata(path).map_or(0, |file| file.len()),
            path: Some(path.to_string_lossy().into_owned()),
        })
        .collect()
}

/// A length on screen as the core's plain number.
fn number(pixels: Pixels) -> f64 {
    f64::from(f32::from(pixels))
}

fn core_point(point: Point<Pixels>) -> CorePoint {
    CorePoint::new(number(point.x), number(point.y))
}

fn core_rect(bounds: Bounds<Pixels>) -> Rect {
    Rect {
        x: number(bounds.origin.x),
        y: number(bounds.origin.y),
        width: number(bounds.size.width),
        height: number(bounds.size.height),
    }
}

/// The popups and dialogs open in the application, in the order they opened.
#[derive(Default)]
struct Layers(LayerStack);

impl gpui::Global for Layers {}

/// The text GPUI's key bindings use for a shortcut, so one written once as
/// `mod+shift+k` can be bound with `KeyBinding::new`. `mod` becomes Cmd on macOS and
/// Ctrl elsewhere.
#[must_use]
pub fn binding_for(shortcut: &Shortcut) -> String {
    let lowered;
    let key = match shortcut.key {
        Key::ArrowUp => "up",
        Key::ArrowDown => "down",
        Key::ArrowLeft => "left",
        Key::ArrowRight => "right",
        Key::Home => "home",
        Key::End => "end",
        Key::PageUp => "pageup",
        Key::PageDown => "pagedown",
        Key::Enter => "enter",
        Key::Space => "space",
        Key::Escape => "escape",
        Key::Tab => "tab",
        Key::Backspace => "backspace",
        Key::F2 => "f2",
        Key::F6 => "f6",
        Key::Char(ch) => {
            lowered = ch.to_lowercase().to_string();
            lowered.as_str()
        }
    };
    let mods = [
        (
            shortcut.mods.ctrl,
            if cfg!(target_os = "macos") {
                "cmd-"
            } else {
                "ctrl-"
            },
        ),
        (shortcut.mods.alt, "alt-"),
        (shortcut.mods.shift, "shift-"),
    ];
    let prefix: String = mods
        .iter()
        .filter(|(held, _)| *held)
        .map(|(_, name)| *name)
        .collect();
    format!("{prefix}{key}")
}

type ChangeGate = Box<dyn FnMut(&ValueChange, &mut App) -> bool>;
type PartKey<P> = (<P as Primitive>::Part, Option<String>);

/// A message for assistive technology. GPUI has no live regions, so the application
/// decides how to deliver it: subscribe to the `Machine` to receive these.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Announcement {
    pub message: String,
    pub assertive: bool,
}

/// The pointer on its way from a part to the popup that part opened.
struct Crossing<P: Primitive> {
    /// Where the pointer may go without having left.
    area: SafeArea,
    popup: Rect,
    /// The part it left, which hears of it only if it never arrives.
    left: PartKey<P>,
}

/// Where a scrolling part is scrolled to, and the lengths of its view and content.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Scrolled {
    offset: f64,
    viewport: f64,
    content: f64,
}

/// Owns one primitive's state, its parts' focus handles, and its agent registration.
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent fact about the component"
)]
pub struct Machine<P: Primitive> {
    primitive: P,
    label: SharedString,
    handles: HashMap<PartKey<P>, FocusHandle>,
    /// Where each part was last painted.
    bounds: HashMap<PartKey<P>, Bounds<Pixels>>,
    /// The part a pointer gesture is being sent to until the button is released.
    captured: Option<PartKey<P>>,
    /// The caret and text of each part the person types into.
    edits: HashMap<PartKey<P>, TextEdit>,
    /// Scrolls the primitive's scrolling part, if it has one.
    scroll: ScrollHandle,
    /// How far the scrolling part was scrolled when the primitive last heard.
    scrolled: Scrolled,
    /// The accessibility node each part was last drawn as, so parts can refer to
    /// one another.
    nodes: HashMap<PartKey<P>, accesskit::NodeId>,
    /// Listens for the primitive's hotkey, wherever focus is.
    hotkey_watch: Option<gpui::Subscription>,
    /// Where focus was before the hotkey took it.
    return_to: Option<FocusHandle>,
    /// Whether files are being dragged over a part that takes drops.
    dragged_over: bool,
    /// The pointer on its way from a part to this primitive's open popup, while that
    /// part's hover-leave is held back.
    crossing: Option<Crossing<P>>,
    /// Whether the keyboard, and not a pointer, was the last thing used on this
    /// primitive.
    by_keyboard: bool,
    /// Whether a tick the primitive asked for through `wake_after` is on its way.
    waking: bool,
    /// Watches for the window being switched away from.
    away_watch: Option<gpui::Subscription>,
    /// Watches for focus leaving each part that has held it.
    blur_watch: HashMap<PartKey<P>, gpui::Subscription>,
    announcements: Vec<Announcement>,
    gate: Option<ChangeGate>,
    registration: u32,
    /// Whether the value must be kept out of anything an agent can read.
    secret: bool,
    created: Instant,
    /// The latest time a timer was asked to fire at. The clock never reads earlier,
    /// so timers stay in step with each other when the executor's clock is not the
    /// wall clock.
    woke_at_ms: u64,
}

impl<P: Primitive> EventEmitter<Announcement> for Machine<P> {}

impl<P: Primitive> Machine<P> {
    pub fn new(
        mut primitive: P,
        label: impl Into<SharedString>,
        items: impl IntoIterator<Item = Item>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (item_parts, single_parts): (Vec<_>, Vec<_>) = P::parts()
            .iter()
            .copied()
            .partition(|part| P::is_item(*part));
        let mut handles = HashMap::new();
        for part in single_parts {
            handles.insert((part, None), cx.focus_handle());
        }
        for item in items {
            for part in &item_parts {
                handles.insert((*part, Some(item.key.clone())), cx.focus_handle());
            }
            primitive.upsert_item(item);
        }

        let target = Self::agent_target(cx.weak_entity());
        let registration = agent::registry(cx).register(target);
        cx.on_release(move |_, cx| agent::registry(cx).unregister(registration))
            .detach();

        Self {
            primitive,
            label: label.into(),
            handles,
            bounds: HashMap::new(),
            captured: None,
            edits: HashMap::new(),
            scroll: ScrollHandle::new(),
            scrolled: Scrolled::default(),
            announcements: Vec::new(),
            blur_watch: HashMap::new(),
            away_watch: None,
            waking: false,
            by_keyboard: false,
            crossing: None,
            hotkey_watch: None,
            return_to: None,
            dragged_over: false,
            nodes: HashMap::new(),
            gate: None,
            registration,
            secret: false,
            created: Instant::now(),
            woke_at_ms: 0,
        }
    }

    /// Called with each proposed change. Returning `false` cancels it.
    #[must_use]
    pub fn on_value_change(
        mut self,
        gate: impl FnMut(&ValueChange, &mut App) -> bool + 'static,
    ) -> Self {
        self.gate = Some(Box::new(gate));
        self
    }

    /// Tells the machine which items a part is being rendered for, when they are not
    /// known up front: a calendar's days, or the tokens in a field. Call it before
    /// rendering those parts so each has a focus handle.
    pub fn track(
        &mut self,
        part: P::Part,
        items: impl IntoIterator<Item = String>,
        cx: &mut Context<Self>,
    ) {
        let wanted: Vec<String> = items.into_iter().collect();
        self.handles.retain(|(p, item), _| {
            *p != part || item.as_ref().is_none_or(|item| wanted.contains(item))
        });
        for item in wanted {
            self.handles
                .entry((part, Some(item)))
                .or_insert_with(|| cx.focus_handle());
        }
    }

    /// Sets what agents may see and do with this component.
    #[must_use]
    pub fn agent_access(mut self, access: Access, cx: &mut Context<Self>) -> Self {
        self.secret = access.secret;
        agent::registry(cx).set_access(self.registration, access);
        self
    }

    /// After a change: keeps the stack of open layers up to date, and records the
    /// change in the shared history.
    fn remember(&self, before: Vec<String>, source: Source, cx: &mut App) {
        // Whatever is open sits in the stack of layers while it is open.
        let open = self.primitive.snapshot("").expanded == Some(true);
        let modal = P::parts().iter().any(|part| {
            let semantics = self.primitive.semantics(*part, None);
            semantics.modal && !semantics.hidden
        });
        let id = self.registration.to_string();
        let layers = &mut cx.default_global::<Layers>().0;
        match (open || modal, modal) {
            (true, true) if layers.top() != Some(id.as_str()) => layers.open(Layer::dialog(&id)),
            (true, false) if layers.top() != Some(id.as_str()) => layers.open(Layer::popup(&id)),
            (false, _) => drop(layers.close(&id)),
            _ => {}
        }
        let after = self.primitive.value();
        agent::record(
            self.label.to_string(),
            before,
            after,
            source,
            self.secret,
            cx,
        );
    }

    /// The primitive itself, for what only it knows, such as a slider's fraction.
    pub const fn primitive(&self) -> &P {
        &self.primitive
    }

    pub fn value(&self) -> Vec<String> {
        self.primitive.value()
    }

    /// Sets the value on the owner's say-so, without asking the gate.
    pub fn set_value(&mut self, value: Vec<String>, cx: &mut Context<Self>) {
        self.primitive.set_value(value);
        cx.notify();
    }

    /// The text to show in a part the person types into.
    pub fn text(&self, part: P::Part, item: Option<&str>) -> Option<String> {
        self.primitive.text(part, item)
    }

    /// What an agent would see: label, value and items in order.
    pub fn snapshot(&self) -> armature_core::agent::Snapshot {
        self.primitive.snapshot(&self.label)
    }

    pub fn semantics(&self, part: P::Part, item: Option<&str>) -> Semantics {
        self.primitive.semantics(part, item)
    }

    /// Ends the wait while the pointer crossed to the popup. Unless it arrived, the
    /// primitive now hears that the pointer left.
    fn end_crossing(&mut self, arrived: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Crossing {
            left: (part, item), ..
        }) = self.crossing.take()
        else {
            return;
        };
        if !arrived {
            let now_ms = self.now_ms();
            self.dispatch(
                Input::Hover {
                    part,
                    item,
                    inside: false,
                    now_ms,
                },
                window,
                cx,
            );
        }
    }

    /// Whether the person is driving this primitive from the keyboard or with a
    /// pointer, for showing focus boldly for the keyboard alone.
    pub const fn modality(&self) -> armature_core::preference::Modality {
        if self.by_keyboard {
            armature_core::preference::Modality::Keyboard
        } else {
            armature_core::preference::Modality::Pointer
        }
    }

    /// Changes a setting the owner controls while the primitive runs, such as a
    /// combobox's `status`. Returns whether it is a setting the primitive has.
    pub fn set(&mut self, name: &str, value: &str, cx: &mut Context<Self>) -> bool {
        cx.notify();
        self.primitive.set(name, value)
    }

    /// Listens for the primitive's hotkey wherever focus is, and remembers where
    /// focus was so the key can send it back.
    fn watch_hotkey(&mut self, cx: &mut Context<Self>) {
        self.hotkey_watch = Some(cx.observe_keystrokes(move |state, event, window, cx| {
            let pressed = key_from_keystroke(&event.keystroke);
            let mods = modifiers(event.keystroke.modifiers);
            let keys = state.primitive.hotkeys();
            let Some(shortcut) = keys
                .into_iter()
                .find(|shortcut| pressed.is_some_and(|key| shortcut.matches(key, mods)))
            else {
                return;
            };
            if !state
                .handles
                .values()
                .any(|handle| handle.is_focused(window))
            {
                state.return_to = window.focused(cx);
            }
            state.dispatch(Input::Hotkey { shortcut }, window, cx);
        }));
    }

    /// Tells the primitive that a picture inside a part loaded or failed. GPUI's `img`
    /// does not report this to its parent, so the view passes it on.
    pub fn loaded(&mut self, part: P::Part, ok: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.dispatch(Input::Loaded { part, ok }, window, cx);
    }

    /// The accessibility node a part was last drawn as.
    pub fn node(&self, part: P::Part, item: Option<&str>) -> Option<accesskit::NodeId> {
        self.nodes.get(&(part, item.map(str::to_string))).copied()
    }

    /// The nodes a part refers to: the one that names it, the one it controls and
    /// the one that describes it.
    pub fn links(&self, part: P::Part, item: Option<&str>) -> [Option<accesskit::NodeId>; 3] {
        let relations = P::relations(part);
        let node = |target: Option<P::Part>| {
            let target = target?;
            self.node(target, item).or_else(|| self.node(target, None))
        };
        [
            node(relations.labelled_by),
            node(relations.controls),
            node(relations.described_by),
        ]
    }

    /// Everything announced so far, oldest first.
    pub fn announcements(&self) -> &[Announcement] {
        &self.announcements
    }

    pub fn focus_handle(&self, part: P::Part, item: Option<&str>) -> Option<FocusHandle> {
        self.handles.get(&(part, item.map(str::to_string))).cloned()
    }

    /// The focus handle Tab would land on among a part's instances.
    pub fn tab_stop(&self, part: P::Part) -> Option<FocusHandle> {
        self.handles
            .iter()
            .find(|((p, item), _)| {
                *p == part
                    && self.primitive.semantics(part, item.as_deref()).focusable
                        == Focusable::TabStop
            })
            .map(|(_, handle)| handle.clone())
    }

    /// Where the caret sits in a part the person types into, counted in characters.
    /// Draw it there; GPUI has no text field to do so.
    pub fn caret(&self, part: P::Part, item: Option<&str>) -> usize {
        let text = self.primitive.text(part, item).unwrap_or_default();
        let end = text.chars().count();
        self.edits
            .get(&(part, item.map(str::to_string)))
            .map_or(end, |field| field.caret().min(end))
    }

    /// A key in a part the person types into. GPUI has no text field element, so
    /// the edit is applied here and the new text offered to the primitive.
    fn edit_text(
        &mut self,
        (part, item): PartKey<P>,
        keystroke: &gpui::Keystroke,
        key: Option<Key>,
        mods: Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let slot = (part, item.clone());
        let text = self
            .primitive
            .text(part, item.as_deref())
            .unwrap_or_default();
        let mut field = self
            .edits
            .remove(&slot)
            .unwrap_or_else(|| TextEdit::new(&text));
        field.set_text(&text);
        let typed = keystroke
            .key_char
            .as_deref()
            .and_then(|typed| typed.chars().next());
        let edit = match (keystroke.key.as_str(), typed, key) {
            ("delete", _, _) => Some(Edit::Delete),
            (_, Some(ch), _) if !mods.ctrl && !mods.alt => Some(Edit::Insert(ch)),
            (_, _, Some(key)) => TextEdit::edit_for(key, mods),
            _ => None,
        };
        let Some(edit) = edit else {
            self.edits.insert(slot, field);
            return false;
        };
        if field.apply(edit) {
            let value = field.text().to_string();
            self.dispatch(
                Input::Text {
                    part,
                    item: item.clone(),
                    value,
                },
                window,
                cx,
            );
            // The primitive may have cleaned what was typed, such as dropping a letter
            // from a code.
            field.set_text(
                &self
                    .primitive
                    .text(part, item.as_deref())
                    .unwrap_or_default(),
            );
        }
        self.edits.insert(slot, field);
        cx.notify();
        true
    }

    fn key_down(
        &mut self,
        (part, item): PartKey<P>,
        takes_text: bool,
        keystroke: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.by_keyboard = true;
        let mods = modifiers(keystroke.modifiers);
        if mods.ctrl && keystroke.key == "v" {
            let pasted = cx
                .read_from_clipboard()
                .and_then(|clipboard| clipboard.text());
            if let Some(text) = pasted {
                return self.dispatch(Input::Paste { text }, window, cx);
            }
        }
        if keystroke.key == "tab" && self.trap_tab(keystroke.modifiers.shift, window, cx) {
            return true;
        }
        let now_ms = self.now_ms();
        let key = key_from_keystroke(keystroke);
        // Apple platforms have their own keys for the ends of a list and for
        // stepping through one.
        let held = keystroke.modifiers;
        let meant = key.filter(|_| cfg!(target_os = "macos")).and_then(|key| {
            armature_core::keys::apple_key(key, held.control, held.platform, held.shift || held.alt)
        });
        let (key, mods) = meant.map_or((key, mods), |meant| (Some(meant), Modifiers::default()));
        if key.is_some_and(|key| self.dispatch(Input::Key { key, mods, now_ms }, window, cx)) {
            return true;
        }
        if !takes_text {
            return false;
        }

        self.edit_text((part, item), keystroke, key, mods, window, cx)
    }

    /// Keeps Tab among this primitive's own parts while one of them is a shown modal.
    /// The part that opens the modal sits outside it, so it is left out.
    fn trap_tab(&self, backwards: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let shown_modal = P::parts().iter().copied().find(|part| {
            let semantics = self.primitive.semantics(*part, None);
            semantics.modal && !semantics.hidden
        });
        let Some(modal) = shown_modal else {
            return false;
        };
        let stops: Vec<&FocusHandle> = P::parts()
            .iter()
            .filter(|part| P::relations(**part).controls != Some(modal))
            .flat_map(|part| self.handles.iter().filter(move |((p, _), _)| p == part))
            .filter(|((part, item), _)| {
                let semantics = self.primitive.semantics(*part, item.as_deref());
                !semantics.hidden && semantics.focusable == Focusable::TabStop
            })
            .map(|(_, handle)| handle)
            .collect();
        let current = stops.iter().position(|handle| handle.is_focused(window));
        let next = armature_core::layers::trapped_focus(current, stops.len(), backwards);
        if let Some(stop) = next.and_then(|next| stops.get(next)) {
            (*stop).clone().focus(window, cx);
        }
        true
    }

    fn now_ms(&self) -> u64 {
        u64::try_from(self.created.elapsed().as_millis())
            .unwrap_or(u64::MAX)
            .max(self.woke_at_ms)
    }

    fn dispatch(
        &mut self,
        input: Input<P::Part>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // Focus can arrive by click, Tab or the application, so read it instead of
        // tracking it.
        let focused = self
            .handles
            .iter()
            .find(|(_, h)| h.is_focused(window))
            .map(|(key, _)| key);
        if let (Input::Key { .. }, Some((part, item))) = (&input, focused) {
            let landed = Input::Focus {
                part: *part,
                item: item.clone(),
            };
            self.primitive.send(landed, &mut |_| true);
        }
        self.run(window, cx, |machine, _, gate| machine.send(input, gate))
            .handled
    }

    fn pointer(
        &mut self,
        phase: PointerPhase,
        (part, item): PartKey<P>,
        position: Point<Pixels>,
        mods: gpui::Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.by_keyboard = false;
        let pointer = Pointer {
            phase,
            part,
            item: item.clone(),
            at: core_point(position),
            mods: modifiers(mods),
            now_ms: self.now_ms(),
        };
        let reaction = self.run(window, cx, |machine, measure, gate| {
            machine.pointer(pointer, measure, gate)
        });
        self.captured = match phase {
            PointerPhase::Down if reaction.effects.contains(&Effect::CapturePointer) => {
                Some((part, item))
            }
            PointerPhase::Move => self.captured.take(),
            _ => None,
        };
    }

    /// Tells the primitive when focus leaves the part holding it for somewhere
    /// outside the primitive.
    fn watch_blur(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.away_watch.is_none() {
            self.away_watch = Some(cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    this.run(window, cx, |machine, _, gate| {
                        machine.send(Input::WindowBlur, gate)
                    });
                }
            }));
        }
        let focused = self
            .handles
            .iter()
            .find(|(_, handle)| handle.is_focused(window));
        let Some((key, handle)) = focused.map(|(key, handle)| (key.clone(), handle.clone())) else {
            return;
        };
        if self.blur_watch.contains_key(&key) {
            return;
        }
        let (part, item) = key.clone();
        let watch = cx.on_blur(&handle, window, move |this, window, cx| {
            if this
                .handles
                .values()
                .any(|handle| handle.is_focused(window))
            {
                return;
            }
            let input = Input::Blur {
                part,
                item: item.clone(),
            };
            this.run(window, cx, |machine, _, gate| machine.send(input, gate));
            cx.notify();
        });
        self.blur_watch.insert(key, watch);
    }

    /// The popup this primitive has open that the pointer may be leaving `part` for.
    fn open_popup_beside(&self, part: P::Part, item: Option<&String>) -> Option<Rect> {
        let others = P::parts().iter().copied().filter(|other| *other != part);
        others.into_iter().find_map(|other| {
            P::popup_of(other).filter(|_| P::hovers(other))?;
            let own = self.bounds.get(&(other, item.cloned()));
            let bounds = own.or_else(|| self.bounds.get(&(other, None)))?;
            let shown = !self
                .primitive
                .semantics(other, item.map(String::as_str))
                .hidden;
            shown.then(|| core_rect(*bounds))
        })
    }

    /// The pointer came to rest on a part or left it. Leaving for a popup this
    /// primitive has open is not leaving yet: the pointer may cross the gap, and
    /// cut the corner, on its way there.
    fn hovered(
        &mut self,
        part: P::Part,
        item: Option<String>,
        inside: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.crossing = None;
        let popup = if inside {
            None
        } else {
            self.open_popup_beside(part, item.as_ref())
        };
        let Some(popup) = popup else {
            let input = Input::Hover {
                part,
                item,
                inside,
                now_ms: self.now_ms(),
            };
            self.dispatch(input, window, cx);
            return;
        };
        let from = core_point(window.mouse_position());
        let axis = if from.y < popup.y || from.y > popup.bottom() {
            Axis::Y
        } else {
            Axis::X
        };
        self.crossing = Some(Crossing {
            area: SafeArea::start(from, popup, axis, self.now_ms()),
            popup,
            left: (part, item),
        });
        // A pointer that stops short of the popup has left after all. One that is
        // resting on it has arrived, even if no move was seen on the way.
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            let _ = this.update_in(cx, |state, window, cx| {
                let arrived = popup.contains(core_point(window.mouse_position()));
                state.end_crossing(arrived, window, cx);
            });
        })
        .detach();
    }

    /// The pointer moved while crossing towards a popup: it has arrived, has
    /// wandered off, or is still on its way.
    fn crossing_moved(&mut self, at: CorePoint, window: &mut Window, cx: &mut Context<Self>) {
        let now_ms = self.now_ms();
        let Some(crossing) = self.crossing.as_mut() else {
            return;
        };
        if crossing.popup.contains(at) {
            self.end_crossing(true, window, cx);
        } else if !crossing.area.contains(at, now_ms) {
            self.end_crossing(false, window, cx);
        }
    }

    /// How an agent reaches this component: to see it, act on it, and put a value back.
    fn agent_target(this: gpui::WeakEntity<Self>) -> Target<App> {
        let (seen, acted) = (this.clone(), this.clone());
        Target {
            snapshot: Rc::new(move |cx: &App| {
                let state = seen.upgrade()?;
                let state = state.read(cx);
                Some(state.primitive.snapshot(&state.label))
            }),
            act: Rc::new(move |cx: &mut App, action, item| {
                let state = acted.upgrade().ok_or(ActError::Refused)?;
                state.update(cx, |state, cx| state.act_for_agent(action, item, cx))
            }),
            set: Rc::new(move |cx: &mut App, value| {
                if let Some(state) = this.upgrade() {
                    state.update(cx, |state, cx| state.set_value(value, cx));
                }
            }),
            access: Access::default(),
        }
    }

    /// Runs one agent action and returns the value afterwards.
    fn act_for_agent(
        &mut self,
        action: &str,
        item: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Result<Vec<String>, ActError> {
        let before = self.primitive.value();
        let mut gate = self.gate.take();
        let result = self.primitive.act(action, item, &mut |change| {
            gate.as_mut().is_none_or(|gate| gate(change, cx))
        });
        self.gate = gate;
        self.remember(before, Source::Agent, cx);
        // An agent action cannot ask for a scroll, so the part is brought to
        // wherever the primitive now wants it.
        if let Some(offset) = self.primitive.wanted_scroll() {
            self.scroll_to(offset);
        }
        cx.notify();
        result.map(|()| self.primitive.value())
    }

    /// Scrolls the primitive's scrolling part to `offset`, along the way it scrolls.
    fn scroll_to(&self, offset: f64) {
        let along = px(-(offset.cast::<f32>()));
        let across = self.primitive.scroll_axis() == Orientation::Horizontal;
        self.scroll.set_offset(if across {
            point(along, px(0.))
        } else {
            point(px(0.), along)
        });
    }

    /// Does what the primitive asked the platform to do.
    fn carry_out(&mut self, effect: &Effect, window: &mut Window, cx: &mut Context<Self>) {
        match effect {
            Effect::Announce { message, assertive } => {
                let announcement = Announcement {
                    message: message.clone(),
                    assertive: *assertive,
                };
                self.announcements.push(announcement.clone());
                cx.emit(announcement);
            }
            Effect::Wake { at_ms } => self.wake_at(*at_ms, window, cx),
            Effect::CapturePointer => {}
            Effect::RestoreFocus => {
                if let Some(back) = self.return_to.take() {
                    back.focus(window, cx);
                }
            }
            Effect::ScrollTo { offset } => self.scroll_to(f64::from(*offset)),
            Effect::Copy { text } => {
                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
            }
            Effect::PickFiles { multiple, .. } => Self::pick_files(*multiple, window, cx),
        }
    }

    /// Sends the primitive a tick once the clock reaches `at_ms`.
    fn wake_at(&self, at_ms: u64, window: &Window, cx: &Context<Self>) {
        let wait = Duration::from_millis(at_ms.saturating_sub(self.now_ms()));
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update_in(cx, |state, window, cx| {
                state.woke_at_ms = state.woke_at_ms.max(at_ms);
                let now_ms = state.now_ms();
                state.dispatch(Input::Tick { now_ms }, window, cx);
            });
        })
        .detach();
    }

    /// Asks the person for files and hands the primitive what they chose. The
    /// platform's picker has no filter for types here, so the primitive turns away
    /// what it does not take when the files come back.
    fn pick_files(multiple: bool, window: &Window, cx: &Context<Self>) {
        let chosen = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else {
                return;
            };
            let _ = this.update_in(cx, |state, window, cx| {
                let input = Input::Files {
                    files: described(&paths),
                    dropped: false,
                };
                state.dispatch(input, window, cx);
            });
        })
        .detach();
    }

    /// Runs one step of the primitive and carries out what it asks for.
    fn run(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        step: impl FnOnce(
            &mut P,
            Measure<P::Part>,
            &mut dyn FnMut(&ValueChange) -> bool,
        ) -> Reaction<P::Part>,
    ) -> Reaction<P::Part> {
        self.watch_blur(window, cx);
        let before = self.primitive.value();
        let mut gate = self.gate.take();
        let bounds = &self.bounds;
        let measure = |part: P::Part, item: Option<&str>| {
            let bounds = bounds.get(&(part, item.map(str::to_string)))?;
            Some(Rect {
                x: number(bounds.origin.x),
                y: number(bounds.origin.y),
                width: number(bounds.size.width),
                height: number(bounds.size.height),
            })
        };
        let reaction = step(&mut self.primitive, &measure, &mut |change| {
            gate.as_mut().is_none_or(|gate| gate(change, cx))
        });
        self.gate = gate;
        self.remember(before, Source::User, cx);

        let target = reaction
            .focus
            .clone()
            .map(|target| (target.part, target.item));
        // A target that has not been rendered yet, such as a day in the month just
        // paged to, gets its handle now so focus is waiting for it.
        if let Some(key) = target {
            let handle = self
                .handles
                .entry(key)
                .or_insert_with(|| cx.focus_handle())
                .clone();
            handle.focus(window, cx);
        }
        for effect in &reaction.effects {
            self.carry_out(effect, window, cx);
        }
        cx.notify();
        reaction
    }
}

/// The accessibility state a part reports: its role, name and what it says of itself.
fn with_semantics<P: Primitive>(
    mut el: Stateful<Div>,
    machine: &Machine<P>,
    semantics: &Semantics,
    part: P::Part,
    item: Option<&str>,
) -> Stateful<Div> {
    if let Some(role) = semantics.role {
        el = el.role(accessibility_role(role));
        if item.is_none() && machine.primitive.named_part() == Some(part) {
            el = el.aria_label(machine.label.clone());
        }
    }
    if let Some(selected) = semantics.selected {
        el = el.aria_selected(selected);
    }
    if let Some(expanded) = semantics.expanded {
        el = el.aria_expanded(expanded);
    }
    if semantics.mixed {
        el = el.aria_toggled(Toggled::Mixed);
    } else if let Some(checked) = semantics.checked.or(semantics.pressed) {
        el = el.aria_toggled(if checked {
            Toggled::True
        } else {
            Toggled::False
        });
    }
    if let Some(level) = semantics.level {
        el = el.aria_level(level.cast::<usize>());
    }
    if let Some(range) = semantics.range {
        el = el
            .aria_numeric_value(range.now)
            .aria_min_numeric_value(range.min)
            .aria_max_numeric_value(range.max);
    }
    if let Some(orientation) = semantics.orientation {
        el = el.aria_orientation(match orientation {
            Orientation::Vertical => gpui::Orientation::Vertical,
            Orientation::Horizontal => gpui::Orientation::Horizontal,
        });
    }
    el
}

/// A popup hears of a press anywhere outside it, other than on its own anchor.
fn with_outside_press<P: Primitive>(
    mut el: Stateful<Div>,
    state: &Entity<Machine<P>>,
    placement: Option<armature_core::popup::Placement<P::Part>>,
    item: Option<&str>,
) -> Stateful<Div> {
    if let Some(placement) = placement {
        let state = state.clone();
        let own = item.map(str::to_string);
        el = el.on_mouse_down_out(move |event: &MouseDownEvent, window, cx| {
            state.update(cx, |state, cx| {
                // A press on the anchor is that part's own press, not an outside one.
                let on_anchor = state
                    .bounds
                    .get(&(placement.anchor, own.clone()))
                    .or_else(|| state.bounds.get(&(placement.anchor, None)))
                    .is_some_and(|bounds| bounds.contains(&event.position));
                // A press while a dialog is up belongs to the dialog, not to what is
                // under it.
                let inert = cx
                    .default_global::<Layers>()
                    .0
                    .is_inert(&state.registration.to_string());
                if !on_anchor && !inert {
                    state.dispatch(Input::PressOutside, window, cx);
                }
            });
        });
    }
    el
}

/// Hover, held while the pointer crosses from a part to the popup it opened.
fn with_hover<P: Primitive>(
    mut el: Stateful<Div>,
    state: &Entity<Machine<P>>,
    key: &PartKey<P>,
) -> Stateful<Div> {
    let part = key.0;
    if P::hovers(part) {
        let hovered = (state.clone(), key.clone());
        el = el.on_hover(move |inside: &bool, window, cx| {
            let (state, (part, item)) = &hovered;
            state.update(cx, |state, cx| {
                state.hovered(*part, item.clone(), *inside, window, cx);
            });
        });
        // While the pointer crosses towards the popup, its path is followed.
        let crossing = state.clone();
        let follow = move |_: Bounds<Pixels>, (): (), window: &mut Window, _: &mut App| {
            let crossing = crossing.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                crossing.update(cx, |state, cx| {
                    state.crossing_moved(core_point(event.position), window, cx);
                });
            });
        };
        el = el.child(canvas(|_, _, _| (), follow).absolute());
    }
    el
}

/// A part the primitive scrolls is given the scroll handle to follow.
fn with_scroll_tracking<P: Primitive>(
    mut el: Stateful<Div>,
    machine: &Machine<P>,
    scrolls: bool,
) -> Stateful<Div> {
    if scrolls {
        el = if machine.primitive.scroll_axis() == Orientation::Horizontal {
            el.overflow_x_scroll().track_scroll(&machine.scroll)
        } else {
            el.overflow_y_scroll().track_scroll(&machine.scroll)
        };
    }
    el
}

/// The wheel goes to a part that asks for it, and scrolls the page otherwise.
fn with_wheel<P: Primitive>(
    mut el: Stateful<Div>,
    state: &Entity<Machine<P>>,
    part: P::Part,
    item: Option<&str>,
) -> Stateful<Div> {
    let (state, item) = (state.clone(), item.map(str::to_string));
    el = el.on_scroll(move |event: &gpui::ScrollEvent, window, cx| {
        let lines = match event.delta {
            gpui::ScrollDelta::Lines(lines) => f64::from(lines.y),
            gpui::ScrollDelta::Pixels(pixels) => number(pixels.y) / 20.0,
        };
        let taken = state.update(cx, |state, cx| {
            if !state.primitive.wheels(part) {
                return false;
            }
            let input = Input::Wheel {
                part,
                item: item.clone(),
                delta: lines,
                mods: modifiers(event.modifiers),
            };
            // What is under it scrolls as usual unless the primitive made use of it.
            state.dispatch(input, window, cx)
        });
        if taken {
            cx.stop_propagation();
        }
    });
    el
}

/// The right mouse button, on a part that calls up a context menu.
fn with_context_menu<P: Primitive>(
    mut el: Stateful<Div>,
    state: &Entity<Machine<P>>,
    part: P::Part,
    item: Option<&str>,
) -> Stateful<Div> {
    if P::context(part) {
        let (state, item) = (state.clone(), item.map(str::to_string));
        el = el.on_mouse_down(
            MouseButton::Right,
            move |event: &MouseDownEvent, window, cx| {
                let at = core_point(event.position);
                let input = Input::ContextMenu {
                    part,
                    item: item.clone(),
                    at,
                };
                state.update(cx, |state, cx| state.dispatch(input, window, cx));
            },
        );
    }
    el
}

/// Files dragged over a part that takes them, and dropped on it.
fn with_file_drops<P: Primitive>(
    mut el: Stateful<Div>,
    state: &Entity<Machine<P>>,
    part: P::Part,
) -> Stateful<Div> {
    if P::drops(part) {
        let over = state.clone();
        el = el.on_drag_move(
            move |event: &gpui::DragMoveEvent<gpui::ExternalPaths>, window, cx| {
                let inside = event.bounds.contains(&event.event.position);
                over.update(cx, |state, cx| {
                    if state.dragged_over != inside {
                        state.dragged_over = inside;
                        state.dispatch(Input::DragOver { inside }, window, cx);
                    }
                });
            },
        );
        let state = state.clone();
        el = el.on_drop(move |paths: &gpui::ExternalPaths, window, cx| {
            let files = described(paths.paths());
            state.update(cx, |state, cx| {
                state.dispatch(
                    Input::Files {
                        files,
                        dropped: true,
                    },
                    window,
                    cx,
                );
            });
        });
    }
    el
}

/// A press that the primitive follows as a gesture, wherever the pointer goes.
fn with_pointer_gesture<P: Primitive>(
    mut el: Stateful<Div>,
    state: &Entity<Machine<P>>,
    key: &PartKey<P>,
    tracks_pointer: bool,
) -> Stateful<Div> {
    if tracks_pointer {
        let pressed = (state.clone(), key.clone());
        el = el.on_mouse_down(
            MouseButton::Left,
            move |event: &MouseDownEvent, window, cx| {
                let (state, key) = &pressed;
                state.update(cx, |state, cx| {
                    let at = event.position;
                    state.pointer(
                        PointerPhase::Down,
                        key.clone(),
                        at,
                        event.modifiers,
                        window,
                        cx,
                    );
                });
            },
        );
        // A captured gesture is followed on the window, so it continues when the
        // pointer leaves the part it began on.
        let followed = (state.clone(), key.clone());
        let listen = move |_: Bounds<Pixels>, (): (), window: &mut Window, _: &mut App| {
            let (state, key) = followed.clone();
            let follow = move |phase, at, mods, window: &mut Window, cx: &mut App| {
                state.update(cx, |state, cx| {
                    if state.captured.as_ref() == Some(&key) {
                        state.pointer(phase, key.clone(), at, mods, window, cx);
                    }
                });
            };
            let moved = follow.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    moved(
                        PointerPhase::Move,
                        event.position,
                        event.modifiers,
                        window,
                        cx,
                    );
                }
            });
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    follow(
                        PointerPhase::Up,
                        event.position,
                        event.modifiers,
                        window,
                        cx,
                    );
                }
            });
        };
        el = el.child(canvas(|_, _, _| (), listen).absolute());
    }
    el
}

/// Whether a part is drawn in place, hidden, or floated against its anchor.
fn layout_of<P: Primitive>(
    machine: &Machine<P>,
    placement: Option<armature_core::popup::Placement<P::Part>>,
    semantics: &Semantics,
    item: Option<&str>,
) -> Layout {
    match placement {
        _ if semantics.hidden => Layout::Hidden,
        None => Layout::Inline,
        // A popup that belongs to an item sits against that item's own anchor.
        Some(placement) => machine
            .primitive
            .popup_point()
            // A popup called up at a point, as a context menu is, sits against it.
            .map(|at| {
                Bounds::new(
                    point(px(at.x.cast::<f32>()), px(at.y.cast::<f32>())),
                    gpui::size(px(0.), px(0.)),
                )
            })
            .or_else(|| {
                machine
                    .bounds
                    .get(&(placement.anchor, item.map(str::to_string)))
                    .copied()
            })
            .or_else(|| machine.bounds.get(&(placement.anchor, None)).copied())
            .map_or(Layout::Hidden, |anchor| {
                floating(placement.side, placement.align, placement.gap, anchor)
            }),
    }
}

/// The accessibility nodes of the parts this one names, controls or is described by.
fn links_of<P: Primitive>(machine: &Machine<P>, part: P::Part, item: Option<&str>) -> Links {
    let relations = P::relations(part);
    // An item's partner is the same item's other part, such as a tab's panel.
    let node = |target: Option<P::Part>| {
        let target = target?;
        let own = machine.nodes.get(&(target, item.map(str::to_string)));
        own.or_else(|| machine.nodes.get(&(target, None))).copied()
    };
    Links {
        labelled_by: node(relations.labelled_by),
        controls: node(relations.controls),
        described_by: node(relations.described_by),
    }
}

/// Sends the primitive a tick at `at_ms`, once the frame being drawn is done.
fn tick_at<P: Primitive>(state: &Entity<Machine<P>>, at_ms: u64, window: &Window, cx: &mut App) {
    let state = state.clone();
    window.defer(cx, move |window, cx| {
        state.update(cx, |_, cx| {
            cx.spawn_in(window, async move |this, cx| {
                let wait = this
                    .read_with(cx, |state, _| at_ms.saturating_sub(state.now_ms()))
                    .unwrap_or(0);
                cx.background_executor()
                    .timer(Duration::from_millis(wait))
                    .await;
                let _ = this.update_in(cx, |state, window, cx| {
                    state.waking = false;
                    state.woke_at_ms = state.woke_at_ms.max(at_ms);
                    let now_ms = state.now_ms();
                    state.dispatch(Input::Tick { now_ms }, window, cx);
                });
            })
            .detach();
        });
    });
}

/// Lets a primitive that lays itself out from its parts measure them, once the
/// frame being drawn is done.
fn remeasure_after_frame<P: Primitive>(state: &Entity<Machine<P>>, window: &Window, cx: &mut App) {
    let state = state.clone();
    window.defer(cx, move |window, cx| {
        state.update(cx, |state, cx| {
            let mut changed = false;
            state.run(window, cx, |machine, measure, _| {
                changed = machine.measured(measure);
                Reaction::default()
            });
            if changed {
                cx.notify();
            }
        });
    });
}

/// What happens once a part has been laid out: its place is remembered, and anything
/// the primitive is waiting on is started.
fn record_bounds<P: Primitive>(
    state: &Entity<Machine<P>>,
    key: &PartKey<P>,
    scrolls: bool,
) -> RecordBounds {
    let (state, key) = (state.clone(), key.clone());
    Box::new(move |bounds, node, window: &mut Window, cx: &mut App| {
        let mut wake = None;
        let mut listen = false;
        let mut remeasure = false;
        let (scrolled, relinked) = state.update(cx, |state, _| {
            // A primitive that lays itself out from where its parts are drawn
            // hears of it when one of them moves.
            remeasure = state.bounds.insert(key.clone(), bounds) != Some(bounds)
                && state.primitive.measures();
            let relinked =
                node.is_some_and(|node| state.nodes.insert(key.clone(), node) != Some(node));
            listen = state.hotkey_watch.is_none() && !state.primitive.hotkeys().is_empty();
            // A value set from outside may have left a timer to start.
            if !state.waking
                && let Some(delay) = state.primitive.wake_after()
            {
                state.waking = true;
                wake = Some(state.now_ms().saturating_add(delay));
            }
            // Scrolling is read off the handle each frame.
            let across = state.primitive.scroll_axis() == Orientation::Horizontal;
            let (offset, max, length) = if across {
                (
                    state.scroll.offset().x,
                    state.scroll.max_offset().x,
                    bounds.size.width,
                )
            } else {
                (
                    state.scroll.offset().y,
                    state.scroll.max_offset().y,
                    bounds.size.height,
                )
            };
            let viewport = number(length);
            let now = Scrolled {
                offset: f64::from(-f32::from(offset)),
                viewport,
                content: viewport + number(max),
            };
            ((scrolls && now != state.scrolled).then_some(now), relinked)
        });
        if listen {
            let state = state.clone();
            window.defer(cx, move |_, cx| {
                state.update(cx, Machine::watch_hotkey);
            });
        }
        if let Some(at_ms) = wake {
            tick_at(&state, at_ms, window, cx);
        }
        if remeasure {
            remeasure_after_frame(&state, window, cx);
        }
        // Parts that refer to this one pick up its node on the next frame.
        if relinked {
            let state = state.clone();
            window.defer(cx, move |_, cx| state.update(cx, |_, cx| cx.notify()));
        }
        // The primitive hears of a change once this frame is done, since a frame
        // being drawn cannot ask for another.
        if let Some(now) = scrolled {
            let (state, part) = (state.clone(), key.0);
            window.defer(cx, move |window, cx| {
                state.update(cx, |state, cx| {
                    state.scrolled = now;
                    let input = Input::Scrolled {
                        part,
                        offset: now.offset,
                        viewport: now.viewport,
                        content: now.content,
                        now_ms: state.now_ms(),
                    };
                    state.run(window, cx, |machine, _, gate| machine.send(input, gate));
                });
            });
        }
    })
}

/// One part of a primitive as an unstyled `div` carrying the part's semantics, focus
/// handle and event wiring. Style it and give it children like any other `div`.
pub fn part<K: PartOf>(
    state: &Entity<Machine<K::Primitive>>,
    part: K,
    item: Option<&str>,
    cx: &App,
) -> PartElement {
    let machine = state.read(cx);
    let semantics = machine.semantics(part, item);
    let id = format!(
        "armature-{}-{part:?}-{}",
        state.entity_id(),
        item.unwrap_or_default()
    );
    let key: PartKey<K::Primitive> = (part, item.map(str::to_string));
    let placement = K::Primitive::popup_of(part);
    let scrolls = K::Primitive::scrolls(part);
    let tracks_pointer = K::Primitive::tracks_pointer(part);

    let mut el = div().id(SharedString::from(id));
    el = with_semantics(el, machine, &semantics, part, item);
    el = with_outside_press(el, state, placement, item);
    el = with_hover(el, state, &key);
    el = with_scroll_tracking(el, machine, scrolls);
    el = with_wheel(el, state, part, item);
    el = with_context_menu(el, state, part, item);
    el = with_file_drops(el, state, part);
    el = with_pointer_gesture(el, state, &key, tracks_pointer);

    let layout = layout_of(machine, placement, &semantics, item);
    let links = links_of(machine, part, item);
    let record = record_bounds(state, &key, scrolls);

    let handle = machine
        .handles
        .get(&key)
        .cloned()
        .filter(|_| semantics.focusable != Focusable::No);
    let Some(handle) = handle else {
        return PartElement {
            inner: el,
            layout,
            record,
            links,
        };
    };
    let handle = handle.tab_stop(semantics.focusable == Focusable::TabStop);
    // A part the primitive keeps text for is edited as a text field. One that
    // edits its own content, key by key, is not.
    let takes_text = matches!(semantics.role, Some(Role::TextBox | Role::SpinButton))
        && machine.text(key.0, key.1.as_deref()).is_some();
    let (part, item) = key;

    let inner = el
        .track_focus(&handle)
        .on_key_down({
            let (state, item) = (state.clone(), item.clone());
            move |event: &KeyDownEvent, window, cx| {
                let slot = (part, item.clone());
                let handled = state.update(cx, |state, cx| {
                    state.key_down(slot, takes_text, &event.keystroke, window, cx)
                });
                if handled {
                    cx.stop_propagation();
                }
            }
        })
        .on_click({
            let state = state.clone();
            move |event: &ClickEvent, window, cx| {
                // A part that tracks the pointer already saw this press as a gesture.
                if tracks_pointer {
                    return;
                }
                let input = Input::Press {
                    part,
                    item: item.clone(),
                    mods: modifiers(event.modifiers()),
                };
                state.update(cx, |state, cx| {
                    state.by_keyboard = false;
                    state.dispatch(input, window, cx)
                });
            }
        });
    PartElement {
        inner,
        layout,
        record,
        links,
    }
}

enum Layout {
    Inline,
    Hidden,
    /// Painted above the window with `corner` of the part pinned at `position`.
    Floating {
        corner: Anchor,
        position: Point<Pixels>,
    },
}

/// GPUI flips a floating part to the other side of its anchor when it would overflow.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "lengths on screen, added as the toolkit's own pixel type"
)]
fn floating(side: Side, align: Align, gap: f64, anchor: Bounds<Pixels>) -> Layout {
    let gap = px(gap.cast::<f32>());
    let (corner, x) = match (side, align) {
        (Side::Bottom, Align::Start) => (Anchor::TopLeft, anchor.left()),
        (Side::Bottom, Align::End) => (Anchor::TopRight, anchor.right()),
        (Side::Top, Align::Start) => (Anchor::BottomLeft, anchor.left()),
        (Side::Top, Align::End) => (Anchor::BottomRight, anchor.right()),
    };
    let y = match side {
        Side::Bottom => anchor.bottom() + gap,
        Side::Top => anchor.top() - gap,
    };
    Layout::Floating {
        corner,
        position: point(x, y),
    }
}

type RecordBounds = Box<dyn Fn(Bounds<Pixels>, Option<accesskit::NodeId>, &mut Window, &mut App)>;

/// A part's `div`. It takes styles, children and listeners like a `div`, but whether
/// and where it appears is decided by the primitive, so styling cannot un-hide it.
pub struct PartElement {
    inner: Stateful<Div>,
    layout: Layout,
    record: RecordBounds,
    links: Links,
}

/// The nodes of the parts this one refers to.
#[derive(Clone, Default)]
struct Links {
    labelled_by: Option<accesskit::NodeId>,
    controls: Option<accesskit::NodeId>,
    described_by: Option<accesskit::NodeId>,
}

/// The accessibility node GPUI draws an element as. GPUI keeps this to itself, so it
/// is read out of the id's debug text. If that text ever changes shape this finds
/// nothing and parts simply go without their links.
fn node_id(id: &GlobalElementId) -> Option<accesskit::NodeId> {
    let text = format!("{id:?}");
    let hash = text.rsplit_once("hash: ")?.1;
    let digits: String = hash.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().map(accesskit::NodeId)
}

impl Styled for PartElement {
    fn style(&mut self) -> &mut StyleRefinement {
        self.inner.style()
    }
}

impl ParentElement for PartElement {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.inner.extend(elements);
    }
}

impl InteractiveElement for PartElement {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.inner.interactivity()
    }
}

impl StatefulInteractiveElement for PartElement {}

impl IntoElement for PartElement {
    type Element = AnyElement;

    fn into_element(self) -> AnyElement {
        let measured = Measured {
            inner: self.inner,
            record: self.record,
            live: None,
            links: self.links,
        };
        match self.layout {
            Layout::Inline => measured.into_any_element(),
            Layout::Hidden => Empty.into_any_element(),
            Layout::Floating { corner, position } => {
                deferred(anchored().anchor(corner).position(position).child(measured))
                    .into_any_element()
            }
        }
    }
}

fn key_from_keystroke(keystroke: &gpui::Keystroke) -> Option<Key> {
    Some(match keystroke.key.as_str() {
        "up" => Key::ArrowUp,
        "down" => Key::ArrowDown,
        "left" => Key::ArrowLeft,
        "right" => Key::ArrowRight,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "enter" => Key::Enter,
        "space" => Key::Space,
        "escape" => Key::Escape,
        "tab" => Key::Tab,
        "backspace" => Key::Backspace,
        "f2" => Key::F2,
        "f6" => Key::F6,
        key => {
            let typed = keystroke.key_char.as_deref().unwrap_or(key);
            let mut chars = typed.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Key::Char(c),
                _ => return None,
            }
        }
    })
}

const fn modifiers(modifiers: gpui::Modifiers) -> Modifiers {
    Modifiers {
        shift: modifiers.shift,
        ctrl: modifiers.control || modifiers.platform,
        alt: modifiers.alt,
    }
}

const fn accessibility_role(role: Role) -> gpui::Role {
    match role {
        Role::Listbox => gpui::Role::ListBox,
        Role::Option => gpui::Role::ListBoxOption,
        Role::TabList => gpui::Role::TabList,
        Role::Tab => gpui::Role::Tab,
        Role::TabPanel => gpui::Role::TabPanel,
        Role::ComboBox => gpui::Role::ComboBox,
        Role::Button => gpui::Role::Button,
        Role::Switch => gpui::Role::Switch,
        Role::RadioGroup => gpui::Role::RadioGroup,
        Role::Radio => gpui::Role::RadioButton,
        Role::Toolbar => gpui::Role::Toolbar,
        Role::Slider => gpui::Role::Slider,
        Role::SpinButton => gpui::Role::SpinButton,
        Role::TextBox => gpui::Role::TextInput,
        Role::Tree => gpui::Role::Tree,
        Role::TreeItem => gpui::Role::TreeItem,
        Role::Group => gpui::Role::Group,
        Role::Region => gpui::Role::Region,
        Role::Grid => gpui::Role::Grid,
        Role::GridCell => gpui::Role::GridCell,
        Role::Dialog => gpui::Role::Dialog,
        Role::List => gpui::Role::List,
        Role::ListItem => gpui::Role::ListItem,
        Role::Separator => gpui::Role::Splitter,
        Role::Tooltip => gpui::Role::Tooltip,
        Role::Checkbox => gpui::Role::CheckBox,
        Role::Menu => gpui::Role::Menu,
        Role::MenuItem => gpui::Role::MenuItem,
        Role::MenuItemCheckbox => gpui::Role::MenuItemCheckBox,
        Role::MenuItemRadio => gpui::Role::MenuItemRadio,
        Role::MenuBar => gpui::Role::MenuBar,
        Role::Meter => gpui::Role::Meter,
        Role::ProgressBar => gpui::Role::ProgressIndicator,
        Role::Navigation => gpui::Role::Navigation,
        Role::Link => gpui::Role::Link,
        Role::Image => gpui::Role::Image,
        Role::Status => gpui::Role::Status,
        Role::Alert => gpui::Role::Alert,
        Role::ScrollBar => gpui::Role::ScrollBar,
        Role::AlertDialog => gpui::Role::AlertDialog,
        Role::Feed => gpui::Role::Feed,
        Role::Article => gpui::Role::Article,
    }
}

fn live_node(id: ElementId, message: SharedString, assertive: bool) -> Measured {
    let (role, live) = if assertive {
        (gpui::Role::Alert, accesskit::Live::Assertive)
    } else {
        (gpui::Role::Status, accesskit::Live::Polite)
    };
    Measured {
        inner: div().id(id).role(role).aria_label(message),
        record: Box::new(|_, _, _, _| {}),
        links: Links::default(),
        live: Some(live),
    }
}

/// A region with no size that carries the primitive's latest announcement to a
/// screen reader, as a live region does on the web. Render it once, anywhere in the
/// view that owns the machine.
pub fn live_region<P: Primitive>(
    state: &Entity<Machine<P>>,
    cx: &App,
) -> impl IntoElement + use<P> {
    let latest = state.read(cx).announcements.last();
    let message = latest
        .map(|said| SharedString::from(said.message.clone()))
        .unwrap_or_default();
    let assertive = latest.is_some_and(|said| said.assertive);
    live_node(
        ("armature-live", state.entity_id()).into(),
        message,
        assertive,
    )
}

/// A `div` that reports the bounds it was laid out with, borders and padding
/// included. A probe child could not see those.
struct Measured {
    inner: Stateful<Div>,
    record: RecordBounds,
    /// Set on a region whose changes a screen reader should speak.
    live: Option<accesskit::Live>,
    links: Links,
}

impl IntoElement for Measured {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Measured {
    type RequestLayoutState = <Stateful<Div> as Element>::RequestLayoutState;
    type PrepaintState = <Stateful<Div> as Element>::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        Element::id(&self.inner)
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        Element::source_location(&self.inner)
    }

    fn a11y_role(&self) -> Option<gpui::Role> {
        Element::a11y_role(&self.inner)
    }

    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        Element::write_a11y_info(&self.inner, node);
        if let Some(live) = self.live {
            node.set_live(live);
        }
        if let Some(target) = self.links.labelled_by {
            node.set_labelled_by(vec![target]);
        }
        if let Some(target) = self.links.controls {
            node.set_controls(vec![target]);
        }
        if let Some(target) = self.links.described_by {
            node.set_described_by(vec![target]);
        }
    }

    fn a11y_synthetic_children(
        &mut self,
        prepaint: &mut Self::PrepaintState,
        builder: &mut A11ySubtreeBuilder,
    ) {
        Element::a11y_synthetic_children(&mut self.inner, prepaint, builder);
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.inner.request_layout(id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        (self.record)(bounds, id.and_then(node_id), window, cx);
        self.inner
            .prepaint(id, inspector_id, bounds, state, window, cx)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.inner.paint(
            id,
            inspector_id,
            bounds,
            request_layout,
            prepaint,
            window,
            cx,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_live_region_marks_its_node_so_changes_are_spoken() {
        let read = |assertive| {
            let region = live_node(
                "live".into(),
                "Dropped milk at position 3 of 4.".into(),
                assertive,
            );
            let mut node = accesskit::Node::new(Element::a11y_role(&region).unwrap());
            Element::write_a11y_info(&region, &mut node);
            (node.role(), node.live(), node.label().map(str::to_string))
        };
        let said = Some("Dropped milk at position 3 of 4.".to_string());
        assert_eq!(
            read(false),
            (
                accesskit::Role::Status,
                Some(accesskit::Live::Polite),
                said.clone()
            )
        );
        assert_eq!(
            read(true),
            (
                accesskit::Role::Alert,
                Some(accesskit::Live::Assertive),
                said
            )
        );
    }

    #[test]
    fn a_part_points_at_the_parts_it_is_related_to() {
        let mut part = live_node("part".into(), "".into(), false);
        part.live = None;
        part.links = Links {
            labelled_by: Some(accesskit::NodeId(7)),
            controls: Some(accesskit::NodeId(8)),
            described_by: None,
        };
        let mut node = accesskit::Node::new(accesskit::Role::Dialog);
        Element::write_a11y_info(&part, &mut node);
        assert_eq!(node.labelled_by(), [accesskit::NodeId(7)]);
        assert_eq!(node.controls(), [accesskit::NodeId(8)]);
        assert_eq!(node.described_by().len(), 0);
    }

    #[test]
    fn a_shortcut_written_once_becomes_a_gpui_binding() {
        let text = binding_for(&Shortcut::parse("mod+shift+k").unwrap());
        let wanted = if cfg!(target_os = "macos") {
            "cmd-shift-k"
        } else {
            "ctrl-shift-k"
        };
        assert_eq!(text, wanted);
        let parsed = gpui::Keystroke::parse(wanted).expect("a binding GPUI accepts");
        assert!(parsed.modifiers.shift && parsed.key == "k");
        assert_eq!(
            binding_for(&Shortcut::parse("alt+enter").unwrap()),
            "alt-enter"
        );
    }
}
