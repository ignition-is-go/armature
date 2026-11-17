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
use armature_core::popup::{Align, Side};
use armature_core::primitive::{
    Effect, Focusable, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive, Reaction,
    Role, Semantics,
};
use gpui::prelude::*;
use gpui::{
    A11ySubtreeBuilder, Anchor, AnyElement, App, Bounds, ClickEvent, Context, DispatchPhase, Div,
    ElementId, Empty, Entity, EventEmitter, FocusHandle, GlobalElementId, InspectorElementId,
    Interactivity, KeyDownEvent, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, SharedString, Stateful, StyleRefinement, Toggled, Window,
    anchored, canvas, deferred, div, point, px,
};

pub use armature_core::change::{Reason, Source};

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
            announcements: Vec::new(),
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

    /// Sets what agents may see and do with this component.
    pub fn agent_access(mut self, access: Access, cx: &mut Context<Self>) -> Self {
        self.secret = access.secret;
        agent::registry(cx).set_access(self.registration, access);
        self
    }

    /// Records a committed change in the shared history.
    fn remember(&self, before: Vec<String>, source: Source, cx: &mut App) {
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
        if let Some(handle) = target.and_then(|key| self.handles.get(&key)) {
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
    if let Some(checked) = semantics.checked {
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
        el = el.on_mouse_down_out(move |event: &MouseDownEvent, window, cx| {
            state.update(cx, |state, cx| {
                // A press on the anchor is that part's own press, not an outside one.
                let on_anchor = state
                    .bounds
                    .get(&(placement.anchor, None))
                    .is_some_and(|bounds| bounds.contains(&event.position));
                if !on_anchor {
                    state.dispatch(Input::PressOutside, window, cx);
                }
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
        Some(placement) => match machine.bounds.get(&(placement.anchor, None)) {
            Some(anchor) => floating(placement.side, placement.align, placement.gap, *anchor),
            None => Layout::Hidden,
        },
    };
    let record: RecordBounds = {
        let (state, key) = (state.clone(), key.clone());
        Box::new(move |bounds, cx: &mut App| {
            state.update(cx, |state, _| state.bounds.insert(key.clone(), bounds));
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
                let keystroke = &event.keystroke;
                let mods = modifiers(keystroke.modifiers);
                let handled = state.update(cx, |state, cx| {
                    let now_ms = state.now_ms();
                    let handled = key_from_keystroke(keystroke).is_some_and(|key| {
                        state.dispatch(Input::Key { key, mods, now_ms }, window, cx)
                    });
                    if handled || !takes_text {
                        return handled;
                    }
                    // GPUI has no text field element, so typing is applied here.
                    let text = state
                        .machine
                        .text(part, item.as_deref())
                        .unwrap_or_default();
                    let Some(value) = edited(&text, keystroke) else {
                        return false;
                    };
                    state.dispatch(
                        Input::Text {
                            part,
                            item: item.clone(),
                            value,
                        },
                        window,
                        cx,
                    );
                    true
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
    }
}

/// `text` after a keystroke in a text part: a typed character appended, or the last
/// one removed by Backspace. `None` for a keystroke that does not edit.
fn edited(text: &str, keystroke: &gpui::Keystroke) -> Option<String> {
    let held = keystroke.modifiers;
    if held.control || held.platform || held.alt {
        return None;
    }
    if keystroke.key == "backspace" {
        let mut shorter = text.to_string();
        shorter.pop()?;
        return Some(shorter);
    }
    let typed = match (keystroke.key_char.as_deref(), keystroke.key.as_str()) {
        (Some(typed), _) => typed,
        (None, "space") => " ",
        (None, key) if key.chars().count() == 1 => key,
        _ => return None,
    };
    Some(format!("{text}{typed}"))
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

type RecordBounds = Box<dyn Fn(Bounds<Pixels>, &mut App)>;

/// A part's `div`. It takes styles, children and listeners like a `div`, but whether
/// and where it appears is decided by the primitive, so styling cannot un-hide it.
pub struct PartElement {
    inner: Stateful<Div>,
    layout: Layout,
    record: RecordBounds,
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
    }
}

/// A `div` that reports the bounds it was laid out with, borders and padding
/// included. A probe child could not see those.
struct Measured {
    inner: Stateful<Div>,
    record: RecordBounds,
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
        (self.record)(bounds, cx);
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
