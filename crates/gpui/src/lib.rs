//! Renders any `armature_core` primitive in GPUI. Nothing here knows about a
//! particular primitive.

pub mod agent;

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
            size: std::fs::metadata(path).map(|file| file.len()).unwrap_or(0),
            path: Some(path.to_string_lossy().into_owned()),
        })
        .collect()
}

/// The popups and dialogs open in the application, in the order they opened.
#[derive(Default)]
struct Layers(LayerStack);

impl gpui::Global for Layers {}

/// The text GPUI's key bindings use for a shortcut, so one written once as
/// `mod+shift+k` can be bound with `KeyBinding::new`. `mod` becomes Cmd on macOS and
/// Ctrl elsewhere.
pub fn binding_for(shortcut: &Shortcut) -> String {
    let key = match shortcut.key {
        Key::ArrowUp => "up".to_string(),
        Key::ArrowDown => "down".to_string(),
        Key::ArrowLeft => "left".to_string(),
        Key::ArrowRight => "right".to_string(),
        Key::Home => "home".to_string(),
        Key::End => "end".to_string(),
        Key::PageUp => "pageup".to_string(),
        Key::PageDown => "pagedown".to_string(),
        Key::Enter => "enter".to_string(),
        Key::Space => "space".to_string(),
        Key::Escape => "escape".to_string(),
        Key::Tab => "tab".to_string(),
        Key::Backspace => "backspace".to_string(),
        Key::F2 => "f2".to_string(),
        Key::F6 => "f6".to_string(),
        Key::Char(ch) => ch.to_lowercase().to_string(),
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

/// Owns one primitive's state, its parts' focus handles, and its agent registration.
pub struct Machine<P: Primitive> {
    machine: P,
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
    /// The scroll offset and viewport length last reported to the primitive.
    scrolled: (f64, f64, f64),
    /// The accessibility node each part was last drawn as, so parts can refer to
    /// one another.
    nodes: HashMap<PartKey<P>, accesskit::NodeId>,
    /// Listens for the primitive's hotkey, wherever focus is.
    hotkey_watch: Option<gpui::Subscription>,
    /// Where focus was before the hotkey took it.
    return_to: Option<FocusHandle>,
    /// Whether files are being dragged over a part that takes drops.
    dragged_over: bool,
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
        mut machine: P,
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
            machine.upsert_item(item);
        }

        let this = cx.weak_entity();
        let registration = agent::registry(cx).register(Target {
            snapshot: Rc::new({
                let this = this.clone();
                move |cx: &App| {
                    let state = this.upgrade()?;
                    let state = state.read(cx);
                    Some(state.machine.snapshot(&state.label))
                }
            }),
            act: Rc::new({
                let this = this.clone();
                move |cx: &mut App, action, item| {
                    let state = this.upgrade().ok_or(ActError::Refused)?;
                    state.update(cx, |state, cx| {
                        let before = state.machine.value();
                        let mut gate = state.gate.take();
                        let result = state.machine.act(action, item, &mut |change| {
                            gate.as_mut().is_none_or(|gate| gate(change, cx))
                        });
                        state.gate = gate;
                        state.remember(before, Source::Agent, cx);
                        // An agent action cannot ask for a scroll, so the part is
                        // brought to wherever the primitive now wants it.
                        if let Some(offset) = state.machine.wanted_scroll() {
                            state.scroll.set_offset(point(px(0.), px(-(offset as f32))));
                        }
                        cx.notify();
                        result.map(|()| state.machine.value())
                    })
                }
            }),
            set: Rc::new(move |cx: &mut App, value| {
                if let Some(state) = this.upgrade() {
                    state.update(cx, |state, cx| state.set_value(value, cx));
                }
            }),
            access: Access::default(),
        });
        cx.on_release(move |_, cx| agent::registry(cx).unregister(registration))
            .detach();

        Machine {
            machine,
            label: label.into(),
            handles,
            bounds: HashMap::new(),
            captured: None,
            edits: HashMap::new(),
            scroll: ScrollHandle::new(),
            scrolled: (0.0, 0.0, 0.0),
            announcements: Vec::new(),
            blur_watch: HashMap::new(),
            away_watch: None,
            waking: false,
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
    pub fn agent_access(mut self, access: Access, cx: &mut Context<Self>) -> Self {
        self.secret = access.secret;
        agent::registry(cx).set_access(self.registration, access);
        self
    }

    /// Records a committed change in the shared history.
    fn remember(&self, before: Vec<String>, source: Source, cx: &mut App) {
        // Whatever is open sits in the stack of layers while it is open.
        let open = self.machine.snapshot("").expanded == Some(true);
        let modal = P::parts().iter().any(|part| {
            let semantics = self.machine.semantics(*part, None);
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
        let after = self.machine.value();
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
    pub fn primitive(&self) -> &P {
        &self.machine
    }

    pub fn value(&self) -> Vec<String> {
        self.machine.value()
    }

    /// Sets the value on the owner's say-so, without asking the gate.
    pub fn set_value(&mut self, value: Vec<String>, cx: &mut Context<Self>) {
        self.machine.set_value(value);
        cx.notify();
    }

    /// The text to show in a part the person types into.
    pub fn text(&self, part: P::Part, item: Option<&str>) -> Option<String> {
        self.machine.text(part, item)
    }

    /// What an agent would see: label, value and items in order.
    pub fn snapshot(&self) -> armature_core::agent::Snapshot {
        self.machine.snapshot(&self.label)
    }

    pub fn semantics(&self, part: P::Part, item: Option<&str>) -> Semantics {
        self.machine.semantics(part, item)
    }

    /// Everything announced so far, oldest first.
    /// Changes a setting the owner controls while the primitive runs, such as a
    /// combobox's `status`. Returns whether it is a setting the primitive has.
    pub fn set(&mut self, name: &str, value: &str, cx: &mut Context<Self>) -> bool {
        cx.notify();
        self.machine.set(name, value)
    }

    /// Listens for the primitive's hotkey wherever focus is, and remembers where
    /// focus was so the key can send it back.
    fn watch_hotkey(&mut self, cx: &mut Context<Self>) {
        let Some(shortcut) = self.machine.hotkey() else {
            return;
        };
        self.hotkey_watch = Some(cx.observe_keystrokes(move |state, event, window, cx| {
            let pressed = key_from_keystroke(&event.keystroke);
            let mods = modifiers(event.keystroke.modifiers);
            if !pressed.is_some_and(|key| shortcut.matches(key, mods)) {
                return;
            }
            if !state
                .handles
                .values()
                .any(|handle| handle.is_focused(window))
            {
                state.return_to = window.focused(cx);
            }
            state.dispatch(Input::Hotkey, window, cx);
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
                    && self.machine.semantics(part, item.as_deref()).focusable == Focusable::TabStop
            })
            .map(|(_, handle)| handle.clone())
    }

    /// Where the caret sits in a part the person types into, counted in characters.
    /// Draw it there; GPUI has no text field to do so.
    pub fn caret(&self, part: P::Part, item: Option<&str>) -> usize {
        let text = self.machine.text(part, item).unwrap_or_default();
        let end = text.chars().count();
        self.edits
            .get(&(part, item.map(str::to_string)))
            .map_or(end, |field| field.caret().min(end))
    }

    fn key_down(
        &mut self,
        (part, item): PartKey<P>,
        takes_text: bool,
        keystroke: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
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
        if key.is_some_and(|key| self.dispatch(Input::Key { key, mods, now_ms }, window, cx)) {
            return true;
        }
        if !takes_text {
            return false;
        }

        // GPUI has no text field element, so the edit is applied here.
        let slot = (part, item.clone());
        let text = self.machine.text(part, item.as_deref()).unwrap_or_default();
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
            field.set_text(&self.machine.text(part, item.as_deref()).unwrap_or_default());
        }
        self.edits.insert(slot, field);
        cx.notify();
        true
    }

    /// Keeps Tab among this primitive's own parts while one of them is a shown modal.
    /// The part that opens the modal sits outside it, so it is left out.
    fn trap_tab(&mut self, backwards: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let shown_modal = P::parts().iter().copied().find(|part| {
            let semantics = self.machine.semantics(*part, None);
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
                let semantics = self.machine.semantics(*part, item.as_deref());
                !semantics.hidden && semantics.focusable == Focusable::TabStop
            })
            .map(|(_, handle)| handle)
            .collect();
        let current = stops.iter().position(|handle| handle.is_focused(window));
        if let Some(next) = armature_core::layers::trapped_focus(current, stops.len(), backwards) {
            stops[next].clone().focus(window, cx);
        }
        true
    }

    fn now_ms(&self) -> u64 {
        (self.created.elapsed().as_millis() as u64).max(self.woke_at_ms)
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
            self.machine.send(landed, &mut |_| true);
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
        let pointer = Pointer {
            phase,
            part,
            item: item.clone(),
            at: CorePoint::new(f32::from(position.x) as f64, f32::from(position.y) as f64),
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

    /// Runs one step of the primitive and carries out what it asks for.
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
        let before = self.machine.value();
        let mut gate = self.gate.take();
        let bounds = &self.bounds;
        let measure = |part: P::Part, item: Option<&str>| {
            let bounds = bounds.get(&(part, item.map(str::to_string)))?;
            let number = |pixels: Pixels| f32::from(pixels) as f64;
            Some(Rect {
                x: number(bounds.origin.x),
                y: number(bounds.origin.y),
                width: number(bounds.size.width),
                height: number(bounds.size.height),
            })
        };
        let reaction = step(&mut self.machine, &measure, &mut |change| {
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
            match effect {
                Effect::Announce { message, assertive } => {
                    let announcement = Announcement {
                        message: message.clone(),
                        assertive: *assertive,
                    };
                    self.announcements.push(announcement.clone());
                    cx.emit(announcement);
                }
                Effect::Wake { at_ms } => {
                    let at_ms = *at_ms;
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
                Effect::CapturePointer => {}
                Effect::RestoreFocus => {
                    if let Some(back) = self.return_to.take() {
                        back.focus(window, cx);
                    }
                }
                Effect::ScrollTo { offset } => {
                    let along = px(-(*offset as f32));
                    let across = self.machine.scroll_axis() == Orientation::Horizontal;
                    self.scroll.set_offset(if across {
                        point(along, px(0.))
                    } else {
                        point(px(0.), along)
                    });
                }
                Effect::Copy { text } => {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                }
                // The platform's picker has no filter for types here, so the primitive
                // turns away what it does not take when the files come back.
                Effect::PickFiles { multiple, .. } => {
                    let chosen = cx.prompt_for_paths(gpui::PathPromptOptions {
                        files: true,
                        directories: false,
                        multiple: *multiple,
                        prompt: None,
                    });
                    cx.spawn_in(window, async move |this, cx| {
                        if let Ok(Ok(Some(paths))) = chosen.await {
                            let _ = this.update_in(cx, |state, window, cx| {
                                let files = described(&paths);
                                state.dispatch(
                                    Input::Files {
                                        files,
                                        dropped: false,
                                    },
                                    window,
                                    cx,
                                );
                            });
                        }
                    })
                    .detach();
                }
            }
        }
        cx.notify();
        reaction
    }
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
    let mut el = div().id(SharedString::from(id));

    if let Some(role) = semantics.role {
        el = el.role(accessibility_role(role));
        if item.is_none() && machine.machine.named_part() == Some(part) {
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
        el = el.aria_level(level as usize);
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

    let placement = K::Primitive::popup_of(part);
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

    if K::Primitive::hovers(part) {
        let hovered = (state.clone(), key.clone());
        el = el.on_hover(move |inside: &bool, window, cx| {
            let (state, (part, item)) = &hovered;
            state.update(cx, |state, cx| {
                let now_ms = state.now_ms();
                let input = Input::Hover {
                    part: *part,
                    item: item.clone(),
                    inside: *inside,
                    now_ms,
                };
                state.dispatch(input, window, cx);
            });
        });
    }
    let scrolls = K::Primitive::scrolls(part);
    if scrolls {
        el = if machine.machine.scroll_axis() == Orientation::Horizontal {
            el.overflow_x_scroll().track_scroll(&machine.scroll)
        } else {
            el.overflow_y_scroll().track_scroll(&machine.scroll)
        };
    }

    if K::Primitive::context(part) {
        let (state, item) = (state.clone(), item.map(str::to_string));
        el = el.on_mouse_down(
            MouseButton::Right,
            move |event: &MouseDownEvent, window, cx| {
                let at = CorePoint::new(
                    f32::from(event.position.x) as f64,
                    f32::from(event.position.y) as f64,
                );
                let input = Input::ContextMenu {
                    part,
                    item: item.clone(),
                    at,
                };
                state.update(cx, |state, cx| state.dispatch(input, window, cx));
            },
        );
    }

    if K::Primitive::drops(part) {
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

    let tracks_pointer = K::Primitive::tracks_pointer(part);
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
        let listen = move |_: Bounds<Pixels>, _: (), window: &mut Window, _: &mut App| {
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

    let layout = match placement {
        _ if semantics.hidden => Layout::Hidden,
        None => Layout::Inline,
        // A popup that belongs to an item sits against that item's own anchor.
        Some(placement) => match machine
            .machine
            .popup_point()
            // A popup called up at a point, as a context menu is, sits against it.
            .map(|at| {
                Bounds::new(
                    point(px(at.x as f32), px(at.y as f32)),
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
        {
            Some(anchor) => floating(placement.side, placement.align, placement.gap, anchor),
            None => Layout::Hidden,
        },
    };
    let links = {
        let relations = K::Primitive::relations(part);
        // An item's partner is the same item's other part, such as a tab's panel.
        let node = |target: Option<K>| {
            let target = target?;
            let own = machine.nodes.get(&(target, item.map(str::to_string)));
            own.or_else(|| machine.nodes.get(&(target, None))).copied()
        };
        Links {
            labelled_by: node(relations.labelled_by),
            controls: node(relations.controls),
            described_by: node(relations.described_by),
        }
    };
    let record: RecordBounds = {
        let (state, key) = (state.clone(), key.clone());
        Box::new(move |bounds, node, window: &mut Window, cx: &mut App| {
            let mut wake = None;
            let mut listen = false;
            let (scrolled, relinked) = state.update(cx, |state, _| {
                state.bounds.insert(key.clone(), bounds);
                let relinked =
                    node.is_some_and(|node| state.nodes.insert(key.clone(), node) != Some(node));
                listen = state.hotkey_watch.is_none() && state.machine.hotkey().is_some();
                // A value set from outside may have left a timer to start.
                if !state.waking
                    && let Some(delay) = state.machine.wake_after()
                {
                    state.waking = true;
                    wake = Some(state.now_ms() + delay);
                }
                // Scrolling is read off the handle each frame.
                let across = state.machine.scroll_axis() == Orientation::Horizontal;
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
                let viewport = f32::from(length) as f64;
                let now = (
                    -f32::from(offset) as f64,
                    viewport,
                    viewport + f32::from(max) as f64,
                );
                ((scrolls && now != state.scrolled).then_some(now), relinked)
            });
            if listen {
                let state = state.clone();
                window.defer(cx, move |_, cx| {
                    state.update(cx, |state, cx| state.watch_hotkey(cx))
                });
            }
            if let Some(at_ms) = wake {
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
                            offset: now.0,
                            viewport: now.1,
                            content: now.2,
                            now_ms: state.now_ms(),
                        };
                        state.run(window, cx, |machine, _, gate| machine.send(input, gate));
                    });
                });
            }
        })
    };

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
    let takes_text = matches!(semantics.role, Some(Role::TextBox | Role::SpinButton));
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
                state.update(cx, |state, cx| state.dispatch(input, window, cx));
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
fn floating(side: Side, align: Align, gap: f64, anchor: Bounds<Pixels>) -> Layout {
    let gap = px(gap as f32);
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

fn modifiers(modifiers: gpui::Modifiers) -> Modifiers {
    Modifiers {
        shift: modifiers.shift,
        ctrl: modifiers.control || modifiers.platform,
        alt: modifiers.alt,
    }
}

fn accessibility_role(role: Role) -> gpui::Role {
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
        assert!(node.described_by().is_empty());
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
