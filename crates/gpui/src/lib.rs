//! Renders any `armature_core` primitive in GPUI. Nothing here knows about a
//! particular primitive.

pub mod agent;

use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use armature_core::agent::{ActError, Target};
use armature_core::change::ValueChange;
use armature_core::keys::{Key, Modifiers, Orientation};
use armature_core::popup::{Align, Side};
use armature_core::primitive::{Focusable, Input, Item, PartOf, Primitive, Role, Semantics};
use gpui::prelude::*;
use gpui::{
    A11ySubtreeBuilder, Anchor, AnyElement, App, Bounds, ClickEvent, Context, Div, ElementId,
    Empty, Entity, FocusHandle, GlobalElementId, InspectorElementId, Interactivity, KeyDownEvent,
    LayoutId, MouseDownEvent, Pixels, Point, SharedString, Stateful, StyleRefinement, Window,
    anchored, deferred, div, point, px,
};

pub use armature_core::change::{Reason, Source};

type ChangeGate = Box<dyn FnMut(&ValueChange, &mut App) -> bool>;
type PartKey<P> = (<P as Primitive>::Part, Option<String>);

/// Owns one primitive's state, its parts' focus handles, and its agent registration.
pub struct Machine<P: Primitive> {
    machine: P,
    label: SharedString,
    handles: HashMap<PartKey<P>, FocusHandle>,
    /// Where each popup anchor was last painted.
    anchors: HashMap<P::Part, Bounds<Pixels>>,
    gate: Option<ChangeGate>,
    created: Instant,
}

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
            act: Rc::new(move |cx: &mut App, action, item| {
                let state = this.upgrade().ok_or(ActError::Refused)?;
                state.update(cx, |state, cx| {
                    let result =
                        state.with_gate(cx, |machine, gate| machine.act(action, item, gate));
                    cx.notify();
                    result.map(|()| state.machine.value())
                })
            }),
        });
        cx.on_release(move |_, cx| agent::registry(cx).unregister(registration))
            .detach();

        Machine {
            machine,
            label: label.into(),
            handles,
            anchors: HashMap::new(),
            gate: None,
            created: Instant::now(),
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

    pub fn value(&self) -> Vec<String> {
        self.machine.value()
    }

    /// What an agent would see: label, value and items in order.
    pub fn snapshot(&self) -> armature_core::agent::Snapshot {
        self.machine.snapshot(&self.label)
    }

    pub fn semantics(&self, part: P::Part, item: Option<&str>) -> Semantics {
        self.machine.semantics(part, item)
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

        let reaction = self.with_gate(cx, |machine, gate| machine.send(input, gate));
        let target = reaction.focus.map(|target| (target.part, target.item));
        if let Some(handle) = target.and_then(|key| self.handles.get(&key)) {
            handle.focus(window, cx);
        }
        cx.notify();
        reaction.handled
    }

    fn with_gate<R>(
        &mut self,
        cx: &mut App,
        run: impl FnOnce(&mut P, &mut dyn FnMut(&ValueChange) -> bool) -> R,
    ) -> R {
        let mut gate = self.gate.take();
        let result = run(&mut self.machine, &mut |change| {
            gate.as_mut().is_none_or(|gate| gate(change, cx))
        });
        self.gate = gate;
        result
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
    let mut el = div().id(SharedString::from(id));

    if let Some(role) = semantics.role {
        el = el.role(accessibility_role(role));
        if item.is_none() {
            el = el.aria_label(machine.label.clone());
        }
    }
    if let Some(selected) = semantics.selected {
        el = el.aria_selected(selected);
    }
    if let Some(expanded) = semantics.expanded {
        el = el.aria_expanded(expanded);
    }
    if let Some(orientation) = semantics.orientation {
        el = el.aria_orientation(match orientation {
            Orientation::Vertical => gpui::Orientation::Vertical,
            Orientation::Horizontal => gpui::Orientation::Horizontal,
        });
    }
    let parts = K::Primitive::parts();
    let anchors_a_popup = parts.iter().any(|other| {
        K::Primitive::popup_of(*other).is_some_and(|placement| placement.anchor == part)
    });
    let record: Option<RecordBounds> = anchors_a_popup.then(|| {
        let state = state.clone();
        Box::new(move |bounds, cx: &mut App| {
            state.update(cx, |state, _| state.anchors.insert(part, bounds));
        }) as RecordBounds
    });
    if let Some(placement) = K::Primitive::popup_of(part) {
        let state = state.clone();
        el = el.on_mouse_down_out(move |event: &MouseDownEvent, window, cx| {
            state.update(cx, |state, cx| {
                // A press on the anchor is that part's own press, not an outside one.
                let on_anchor = state
                    .anchors
                    .get(&placement.anchor)
                    .is_some_and(|b| b.contains(&event.position));
                if !on_anchor {
                    state.dispatch(Input::PressOutside, window, cx);
                }
            });
        });
    }

    let handle = machine
        .focus_handle(part, item)
        .filter(|_| semantics.focusable != Focusable::No);
    let layout = match K::Primitive::popup_of(part) {
        _ if semantics.hidden => Layout::Hidden,
        None => Layout::Inline,
        Some(placement) => match machine.anchors.get(&placement.anchor) {
            Some(anchor) => floating(placement.side, placement.align, placement.gap, *anchor),
            None => Layout::Hidden,
        },
    };
    let Some(handle) = handle else {
        return PartElement {
            inner: el,
            layout,
            record,
        };
    };
    let handle = handle.tab_stop(semantics.focusable == Focusable::TabStop);
    let item = item.map(str::to_string);

    let inner = el
        .track_focus(&handle)
        .on_key_down({
            let state = state.clone();
            move |event: &KeyDownEvent, window, cx| {
                let Some(key) = key_from_keystroke(&event.keystroke) else {
                    return;
                };
                let mods = modifiers(event.keystroke.modifiers);
                let handled = state.update(cx, |state, cx| {
                    let now_ms = state.created.elapsed().as_millis() as u64;
                    state.dispatch(Input::Key { key, mods, now_ms }, window, cx)
                });
                if handled {
                    cx.stop_propagation();
                }
            }
        })
        .on_click({
            let state = state.clone();
            move |event: &ClickEvent, window, cx| {
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

/// A part's `div`. It takes styles, children and listeners like a `div`, but whether
/// and where it appears is decided by the primitive, so styling cannot un-hide it.
pub struct PartElement {
    inner: Stateful<Div>,
    layout: Layout,
    /// Set on a part that anchors a popup.
    record: Option<RecordBounds>,
}

type RecordBounds = Box<dyn Fn(Bounds<Pixels>, &mut App)>;

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
        match self.layout {
            Layout::Inline => match self.record {
                None => self.inner.into_any_element(),
                Some(record) => Measured {
                    inner: self.inner,
                    record,
                }
                .into_any_element(),
            },
            Layout::Hidden => Empty.into_any_element(),
            Layout::Floating { corner, position } => deferred(
                anchored()
                    .anchor(corner)
                    .position(position)
                    .child(self.inner),
            )
            .into_any_element(),
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
        "enter" => Key::Enter,
        "space" => Key::Space,
        "escape" => Key::Escape,
        "tab" => Key::Tab,
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
