use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use armature_core::agent::{Access, ActError, Target};
use armature_core::change::{Source, ValueChange};
use armature_core::geometry::Point;
use armature_core::keys::{Key, Modifiers, Orientation};
use armature_core::popup::{self, Placement, Rect, Side, Size};
use armature_core::primitive::{
    Effect as Ask, FocusTarget, Focusable, Input, Item, PartOf, Pointer, PointerPhase, Primitive,
    Reaction, Role,
};
use leptos::prelude::*;
use send_wrapper::SendWrapper;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::HtmlElement;

use crate::agent;

/// Called with each proposed change. Returning `false` cancels it.
pub type ChangeGate = Callback<ValueChange, bool>;

thread_local! {
    static NEXT_INSTANCE: Cell<u32> = const { Cell::new(1) };
}

struct Context<P: Primitive> {
    instance: u32,
    machine: RwSignal<P>,
    label: StoredValue<String>,
    gate: Option<ChangeGate>,
    /// The part a pointer gesture is being sent to until the pointer is released.
    captured: StoredValue<Option<(P::Part, Option<String>)>>,
    /// Whether the value must be kept out of anything an agent can read.
    secret: bool,
}

impl<P: Primitive> Clone for Context<P> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<P: Primitive> Copy for Context<P> {}

struct Rendered {
    part: String,
    item: Option<String>,
    registers: bool,
    element: HtmlElement,
}

impl<P: Primitive + Send + Sync> Context<P> {
    fn dispatch(self, input: Input<P::Part>) -> bool {
        self.run(|machine, gate| machine.send(input, gate))
    }

    fn pointer(
        self,
        phase: PointerPhase,
        part: P::Part,
        item: Option<String>,
        ev: &web_sys::MouseEvent,
    ) -> bool {
        let pointer = Pointer {
            phase,
            part,
            item,
            at: Point::new(ev.client_x() as f64, ev.client_y() as f64),
            mods: modifiers(ev.shift_key(), ev.ctrl_key() || ev.meta_key(), ev.alt_key()),
            now_ms: js_sys::Date::now() as u64,
        };
        let measure = move |part: P::Part, item: Option<&str>| {
            let element = self.element_of(part, item)?;
            let rect = element.get_bounding_client_rect();
            Some(Rect {
                x: rect.x(),
                y: rect.y(),
                width: rect.width(),
                height: rect.height(),
            })
        };
        self.run(|machine, gate| machine.pointer(pointer, &measure, gate))
    }

    /// The primitive with its items in rendered order and their labels filled in from
    /// the rendered text.
    fn synced(self, rendered: &[Rendered]) -> Option<P> {
        let mut machine = self.machine.try_get_untracked()?;
        let order: Vec<(String, String)> = rendered
            .iter()
            .filter(|r| r.registers)
            .filter_map(|r| {
                Some((
                    r.item.clone()?,
                    r.element.text_content().unwrap_or_default(),
                ))
            })
            .collect();
        machine.set_order(&order);
        Some(machine)
    }

    /// Runs one step of the primitive and carries out what it asks for.
    fn run(
        self,
        step: impl FnOnce(&mut P, &mut dyn FnMut(&ValueChange) -> bool) -> Reaction<P::Part>,
    ) -> bool {
        // The gate runs user code, so the primitive is copied out instead of borrowed.
        let rendered = self.rendered();
        let Some(mut machine) = self.synced(&rendered) else {
            return false;
        };
        let before = machine.value();
        let reaction = step(&mut machine, &mut |change| allows(self.gate, change));
        let after = machine.value();
        self.machine.set(machine);
        agent::record(
            self.label.get_value(),
            before,
            after,
            Source::User,
            self.secret,
        );

        if let Some(target) = reaction.focus {
            self.focus(target);
        }
        for effect in reaction.effects {
            match effect {
                Ask::Announce { message, assertive } => announce(&message, assertive),
                Ask::Wake { at_ms } => {
                    let wait = at_ms.saturating_sub(js_sys::Date::now() as u64);
                    set_timeout(
                        move || {
                            self.dispatch(Input::Tick {
                                now_ms: js_sys::Date::now() as u64,
                            });
                        },
                        Duration::from_millis(wait),
                    );
                }
                // The part to send to was recorded when the gesture began.
                Ask::CapturePointer => {}
                Ask::ScrollTo { offset } => {
                    let scrolling = P::parts().iter().copied().find(|part| P::scrolls(*part));
                    if let Some(element) = scrolling.and_then(|part| self.element_of(part, None)) {
                        element.set_scroll_top(offset as i32);
                    }
                }
                Ask::Copy { text } => {
                    let _ = window().navigator().clipboard().write_text(&text);
                }
            }
        }
        reaction.handled
    }

    fn rendered(self) -> Vec<Rendered> {
        let selector = format!("[data-armature=\"{}\"]", self.instance);
        let Ok(nodes) = document().query_selector_all(&selector) else {
            return Vec::new();
        };
        (0..nodes.length())
            .filter_map(|i| nodes.item(i)?.dyn_into::<HtmlElement>().ok())
            .filter_map(|element| {
                Some(Rendered {
                    part: element.get_attribute("data-armature-part")?,
                    item: element.get_attribute("data-value"),
                    registers: element.has_attribute("data-armature-item"),
                    element,
                })
            })
            .collect()
    }

    fn element_of(self, part: P::Part, item: Option<&str>) -> Option<HtmlElement> {
        let name = format!("{part:?}");
        self.rendered()
            .into_iter()
            .find(|r| r.part == name && r.item.as_deref() == item)
            .map(|r| r.element)
    }

    /// Moves focus to a part, now and again a frame later. The change that asked for
    /// it may not have rendered yet, and a re-render that moves the part drops focus.
    fn focus(self, target: FocusTarget<P::Part>) {
        let attempt = move || {
            let Some(element) = self.element_of(target.part, target.item.as_deref()) else {
                return;
            };
            let focused = document()
                .active_element()
                .is_some_and(|active| active.is_same_node(Some(&element)));
            if !focused {
                let _ = element.focus();
            }
        };
        attempt();
        request_animation_frame(attempt);
    }

    fn place(self, popup: &HtmlElement, placement: Placement<P::Part>) {
        let Some(anchor) = self.element_of(placement.anchor, None) else {
            return;
        };
        let anchor = anchor.get_bounding_client_rect();
        let size = popup.get_bounding_client_rect();
        let dimension = |value: Result<JsValue, JsValue>| value.ok().and_then(|v| v.as_f64());
        let viewport = Size {
            width: dimension(window().inner_width()).unwrap_or(f64::MAX),
            height: dimension(window().inner_height()).unwrap_or(f64::MAX),
        };
        let placed = popup::place(
            Rect {
                x: anchor.x(),
                y: anchor.y(),
                width: anchor.width(),
                height: anchor.height(),
            },
            Size {
                width: size.width(),
                height: size.height(),
            },
            viewport,
            placement.side,
            placement.align,
            placement.gap,
        );
        let style = popup.style();
        let _ = style.set_property("left", &format!("{}px", placed.x));
        let _ = style.set_property("top", &format!("{}px", placed.y));
        let anchor_box = Rect {
            x: anchor.x(),
            y: anchor.y(),
            width: anchor.width(),
            height: anchor.height(),
        };
        let popup_size = Size {
            width: size.width(),
            height: size.height(),
        };
        let arrow = popup::arrow_offset(anchor_box, placed, popup_size, 12.0);
        let _ = style.set_property("--armature-arrow-x", &format!("{arrow}px"));
        let side = if placed.side == Side::Bottom {
            "bottom"
        } else {
            "top"
        };
        let _ = popup.set_attribute("data-side", side);
    }
}

fn allows(gate: Option<ChangeGate>, change: &ValueChange) -> bool {
    gate.is_none_or(|gate| gate.run(change.clone()))
}

fn modifiers(shift: bool, ctrl: bool, alt: bool) -> Modifiers {
    Modifiers { shift, ctrl, alt }
}

/// Speaks a message through a live region shared by every component on the page.
fn announce(message: &str, assertive: bool) {
    let id = if assertive {
        "armature-live-assertive"
    } else {
        "armature-live-polite"
    };
    let region = document().get_element_by_id(id).or_else(|| {
        let region = document().create_element("div").ok()?;
        region.set_id(id);
        let _ = region.set_attribute("aria-live", if assertive { "assertive" } else { "polite" });
        let _ = region.set_attribute("aria-atomic", "true");
        let hidden = "position:absolute;width:1px;height:1px;overflow:hidden;\
                      clip-path:inset(50%);white-space:nowrap";
        let _ = region.set_attribute("style", hidden);
        document().body()?.append_child(&region).ok()?;
        Some(region)
    });
    let Some(region) = region else { return };
    // Emptied first so that repeating a message is still spoken.
    region.set_text_content(Some(""));
    let message = message.to_string();
    request_animation_frame(move || region.set_text_content(Some(&message)));
}

/// Moves focus to the next tabbable element inside a modal part, wrapping at the ends
/// so Tab never leaves it.
fn trap_tab(container: &HtmlElement, backwards: bool) {
    let tabbable = "a[href], button, input, select, textarea, [tabindex]:not([tabindex=\"-1\"])";
    let Ok(nodes) = container.query_selector_all(tabbable) else {
        return;
    };
    let stops: Vec<HtmlElement> = (0..nodes.length())
        .filter_map(|i| nodes.item(i)?.dyn_into::<HtmlElement>().ok())
        .filter(|el| !el.has_attribute("disabled") && el.offset_parent().is_some())
        .collect();
    let active = document().active_element();
    let current = stops.iter().position(|stop| {
        active
            .as_ref()
            .is_some_and(|active| active.is_same_node(Some(stop)))
    });
    if let Some(next) = armature_core::layers::trapped_focus(current, stops.len(), backwards) {
        let _ = stops[next].focus();
    }
}

fn orientation_name(orientation: Option<Orientation>) -> Option<&'static str> {
    orientation.map(|orientation| match orientation {
        Orientation::Vertical => "vertical",
        Orientation::Horizontal => "horizontal",
    })
}

fn key_from_dom(key: &str) -> Option<Key> {
    Some(match key {
        "ArrowUp" => Key::ArrowUp,
        "ArrowDown" => Key::ArrowDown,
        "ArrowLeft" => Key::ArrowLeft,
        "ArrowRight" => Key::ArrowRight,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "Enter" => Key::Enter,
        " " => Key::Space,
        "Escape" => Key::Escape,
        "Tab" => Key::Tab,
        "Backspace" => Key::Backspace,
        other => {
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Key::Char(c),
                _ => return None,
            }
        }
    })
}

fn aria_role(role: Role) -> &'static str {
    match role {
        Role::Listbox => "listbox",
        Role::Option => "option",
        Role::TabList => "tablist",
        Role::Tab => "tab",
        Role::TabPanel => "tabpanel",
        Role::ComboBox => "combobox",
        Role::Button => "button",
        Role::Switch => "switch",
        Role::RadioGroup => "radiogroup",
        Role::Radio => "radio",
        Role::Toolbar => "toolbar",
        Role::Slider => "slider",
        Role::SpinButton => "spinbutton",
        Role::TextBox => "textbox",
        Role::Tree => "tree",
        Role::TreeItem => "treeitem",
        Role::Group => "group",
        Role::Region => "region",
        Role::Grid => "grid",
        Role::GridCell => "gridcell",
        Role::Dialog => "dialog",
        Role::List => "list",
        Role::ListItem => "listitem",
        Role::Separator => "separator",
        Role::Tooltip => "tooltip",
    }
}

/// The primitive owned by the nearest `Root`, for what only it knows, such as a
/// calendar's weeks or a slider's fraction. Call it from a component inside the `Root`.
pub fn use_primitive<P>() -> ReadSignal<P>
where
    P: Primitive + Send + Sync,
{
    expect_context::<Context<P>>().machine.read_only()
}

/// Owns one primitive's state and makes it available to the `Part`s inside.
#[component]
pub fn Root<P>(
    machine: P,
    /// Accessible name, also how agents find this component.
    #[prop(into)]
    label: String,
    /// Called with each proposed change. Returning `false` cancels it.
    #[prop(optional, into)]
    on_value_change: Option<ChangeGate>,
    /// A value the owner controls. The component follows it whenever it changes.
    #[prop(optional, into)]
    value: Option<Signal<Vec<String>>>,
    /// Keeps the component from agents altogether.
    #[prop(optional)]
    agent_hidden: bool,
    /// Lets agents see the component but not its value.
    #[prop(optional)]
    agent_secret: bool,
    /// Makes agent actions on the component need a person's approval. The text says
    /// what is being approved.
    #[prop(optional, into)]
    agent_confirm: Option<String>,
    children: Children,
) -> impl IntoView
where
    P: Primitive + Send + Sync,
{
    let machine = RwSignal::new(machine);
    let label = StoredValue::new(label);
    let instance = NEXT_INSTANCE.replace(NEXT_INSTANCE.get() + 1);
    let ctx = Context {
        instance,
        machine,
        label,
        gate: on_value_change,
        captured: StoredValue::new(None),
        secret: agent_secret,
    };
    provide_context(ctx);

    if let Some(value) = value {
        Effect::new(move |_| {
            let value = value.get();
            if machine.with_untracked(|m| m.value() != value) {
                machine.update(|m| m.set_value(value));
            }
        });
    }

    let registration = agent::register(Target {
        snapshot: Rc::new(move |_| Some(ctx.synced(&ctx.rendered())?.snapshot(&label.get_value()))),
        act: Rc::new(move |_, action, item| {
            let mut next = ctx.synced(&ctx.rendered()).ok_or(ActError::Refused)?;
            let before = next.value();
            next.act(action, item, &mut |change| allows(on_value_change, change))?;
            let value = next.value();
            machine.set(next);
            agent::record(
                label.get_value(),
                before,
                value.clone(),
                Source::Agent,
                agent_secret,
            );
            Ok(value)
        }),
        set: Rc::new(move |_, value| {
            machine.try_update(|m| m.set_value(value));
        }),
        access: Access {
            hidden: agent_hidden,
            secret: agent_secret,
            confirm: agent_confirm,
        },
    });
    on_cleanup(move || agent::unregister(registration));

    let parts = P::parts();
    if parts.iter().any(|part| P::popup_of(*part).is_some()) {
        let inside = format!("[data-armature=\"{instance}\"]");
        let listener = window_event_listener(leptos::ev::pointerdown, move |ev| {
            let target = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
            let pressed_inside =
                target.is_some_and(|el| matches!(el.closest(&inside), Ok(Some(_))));
            if !pressed_inside {
                ctx.dispatch(Input::PressOutside);
            }
        });
        on_cleanup(move || listener.remove());
    }

    // A captured gesture is followed on the window, so it continues when the pointer
    // leaves the part it began on.
    if parts.iter().any(|part| P::tracks_pointer(*part)) {
        let follow = move |phase: PointerPhase, ev: &web_sys::MouseEvent| {
            let Some((part, item)) = ctx.captured.try_get_value().flatten() else {
                return;
            };
            if phase != PointerPhase::Move {
                ctx.captured.set_value(None);
            }
            ctx.pointer(phase, part, item, ev);
        };
        let moved = window_event_listener(leptos::ev::pointermove, move |ev| {
            follow(PointerPhase::Move, &ev)
        });
        let released = window_event_listener(leptos::ev::pointerup, move |ev| {
            follow(PointerPhase::Up, &ev)
        });
        let cancelled = window_event_listener(leptos::ev::pointercancel, move |ev| {
            follow(PointerPhase::Cancel, &ev)
        });
        on_cleanup(move || {
            moved.remove();
            released.remove();
            cancelled.remove();
        });
    }

    children()
}

/// One part of the primitive owned by the nearest `Root`. It renders unstyled, as a
/// `div`, or as an `input` for a part the person types into, and carries the part's
/// semantics and event wiring.
#[component]
pub fn Part<K>(
    part: K,
    /// The item this part belongs to. Required for item parts.
    #[prop(optional, into)]
    item: Option<String>,
    /// For an item, its text for typeahead and agents, which defaults to the rendered
    /// text. For any other part, its own accessible name.
    #[prop(optional, into)]
    label: String,
    #[prop(optional)] disabled: bool,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView
where
    K: PartOf,
    K::Primitive: Send + Sync,
{
    let ctx = expect_context::<Context<K::Primitive>>();
    let registers = K::Primitive::is_item(part);
    let tracks_pointer = K::Primitive::tracks_pointer(part);
    let own_name = (!registers && !label.is_empty()).then(|| label.clone());
    if registers {
        let key = item
            .clone()
            .expect("this part is an item and needs an `item` key");
        ctx.machine.update(|m| {
            m.upsert_item(Item {
                key: key.clone(),
                label,
                disabled,
            })
        });
        on_cleanup(move || {
            ctx.machine.try_update(|m| m.remove_item(&key));
        });
    }

    let semantics = Memo::new({
        let item = item.clone();
        move |_| ctx.machine.with(|m| m.semantics(part, item.as_deref()))
    });
    let role = semantics.get_untracked().role;
    let interactive = move || semantics.get_untracked().focusable != Focusable::No;
    let names_component =
        item.is_none() && ctx.machine.with_untracked(|m| m.named_part()) == Some(part);
    let takes_text = matches!(role, Some(Role::TextBox | Role::SpinButton));

    let element = NodeRef::<leptos::html::Div>::new();
    let placement = K::Primitive::popup_of(part);
    if let Some(placement) = placement {
        Effect::new(move |_| {
            if semantics.get().hidden {
                return;
            }
            // Measured a frame later, once the popup has a size.
            request_animation_frame(move || {
                if let Some(popup) = element.get_untracked() {
                    ctx.place(&popup, placement);
                }
            });
        });

        let follow = move || {
            if !semantics.get_untracked().hidden
                && let Some(popup) = element.get_untracked()
            {
                ctx.place(&popup, placement);
            }
        };
        // Scroll does not bubble, so it is caught on the way down. That follows the
        // anchor inside any scrolling container, not only the page.
        let on_scroll = SendWrapper::new(Closure::<dyn Fn()>::new(follow));
        let _ = window().add_event_listener_with_callback_and_bool(
            "scroll",
            on_scroll.as_ref().unchecked_ref(),
            true,
        );
        let on_resize = window_event_listener(leptos::ev::resize, move |_| follow());
        on_cleanup(move || {
            let _ = window().remove_event_listener_with_callback_and_bool(
                "scroll",
                on_scroll.as_ref().unchecked_ref(),
                true,
            );
            on_resize.remove();
        });
    }

    let on_click = {
        let item = item.clone();
        move |ev: leptos::ev::MouseEvent| {
            // A part that tracks the pointer already saw this press as a gesture.
            if tracks_pointer || !interactive() {
                return;
            }
            let mods = modifiers(ev.shift_key(), ev.ctrl_key() || ev.meta_key(), ev.alt_key());
            ctx.dispatch(Input::Press {
                part,
                item: item.clone(),
                mods,
            });
        }
    };
    let on_pointerdown = {
        let item = item.clone();
        move |ev: leptos::ev::PointerEvent| {
            // A part nested inside this one may already have taken the gesture.
            if !tracks_pointer || ev.button() != 0 || ev.default_prevented() {
                return;
            }
            ctx.captured.set_value(Some((part, item.clone())));
            ctx.pointer(PointerPhase::Down, part, item.clone(), &ev);
            // Stops the browser starting a text selection or a native drag.
            ev.prevent_default();
        }
    };
    let on_focus = {
        let item = item.clone();
        move |_| {
            if interactive() && !semantics.get_untracked().active {
                ctx.dispatch(Input::Focus {
                    part,
                    item: item.clone(),
                });
            }
        }
    };
    let on_keydown = move |ev: leptos::ev::KeyboardEvent| {
        // Keys pressed while an input method is composing belong to it.
        if !interactive() || ev.is_composing() {
            return;
        }
        if ev.key() == "Tab"
            && semantics.get_untracked().modal
            && let Some(container) = element.get_untracked()
        {
            trap_tab(&container, ev.shift_key());
            ev.prevent_default();
            ev.stop_propagation();
            return;
        }
        let Some(key) = key_from_dom(&ev.key()) else {
            return;
        };
        let mods = modifiers(ev.shift_key(), ev.ctrl_key() || ev.meta_key(), ev.alt_key());
        let now_ms = js_sys::Date::now() as u64;
        if ctx.dispatch(Input::Key { key, mods, now_ms }) {
            ev.prevent_default();
            ev.stop_propagation();
        }
    };

    let hovers = K::Primitive::hovers(part);
    let hover = {
        let item = item.clone();
        move |inside: bool, ev: leptos::ev::PointerEvent| {
            // Touch has no hover, so a tap never counts as one.
            if hovers && ev.pointer_type() != "touch" {
                let now_ms = js_sys::Date::now() as u64;
                ctx.dispatch(Input::Hover {
                    part,
                    item: item.clone(),
                    inside,
                    now_ms,
                });
            }
        }
    };
    let (on_enter, on_leave) = (hover.clone(), hover);
    let scrolls = K::Primitive::scrolls(part);
    let report_scroll = move || {
        if let Some(scroller) = element.get_untracked() {
            let offset = scroller.scroll_top() as f64;
            let viewport = scroller.client_height() as f64;
            ctx.dispatch(Input::Scrolled {
                part,
                offset,
                viewport,
            });
        }
    };
    if scrolls {
        // The first report waits a frame, for the part to have a size.
        Effect::new(move |_| request_animation_frame(report_scroll));
    }
    let on_paste = move |ev: leptos::ev::ClipboardEvent| {
        if !interactive() {
            return;
        }
        let pasted = ev
            .clipboard_data()
            .and_then(|data| data.get_data("text/plain").ok());
        if let Some(text) = pasted
            && ctx.dispatch(Input::Paste { text })
        {
            ev.prevent_default();
            ev.stop_propagation();
        }
    };

    let range = move || semantics.get().range;
    let tabindex = move || match semantics.get().focusable {
        Focusable::No => None,
        Focusable::Programmatic => Some("-1"),
        Focusable::TabStop => Some("0"),
    };
    // Related parts find each other by id. Parts that share an item share its key.
    let id_of = {
        let (instance, item) = (ctx.instance, item.clone());
        move |part: K| match &item {
            Some(item) => format!("armature-{instance}-{part:?}-{item}"),
            None => format!("armature-{instance}-{part:?}"),
        }
    };
    let relations = K::Primitive::relations(part);
    let controls = relations.controls.map(&id_of);
    let labelled_by = relations.labelled_by.map(&id_of);
    let described_by = relations.described_by.map(&id_of);
    let own_id = id_of(part);

    // State that changes often is written straight onto the element. The view layer
    // caps how many attributes one element may declare.
    Effect::new(move |_| {
        let state = semantics.get();
        let Some(element) = element.get() else { return };
        let flag = |on: Option<bool>| on.map(|on| on.to_string());
        let number = |value: Option<f64>| value.map(|value| value.to_string());
        let attributes = [
            ("aria-selected", flag(state.selected)),
            ("aria-expanded", flag(state.expanded)),
            ("aria-checked", flag(state.checked)),
            ("aria-multiselectable", flag(state.multiselectable)),
            ("aria-disabled", state.disabled.then(|| "true".to_string())),
            ("aria-level", state.level.map(|level| level.to_string())),
            ("aria-valuenow", number(state.range.map(|range| range.now))),
            ("aria-valuemin", number(state.range.map(|range| range.min))),
            ("aria-valuemax", number(state.range.map(|range| range.max))),
            (
                "aria-orientation",
                orientation_name(state.orientation).map(str::to_string),
            ),
            (
                "aria-haspopup",
                (state.role == Some(Role::ComboBox)).then(|| "listbox".to_string()),
            ),
            ("aria-modal", state.modal.then(|| "true".to_string())),
            ("aria-controls", controls.clone()),
            ("aria-labelledby", labelled_by.clone()),
            ("aria-describedby", described_by.clone()),
            ("data-active", state.active.then(String::new)),
        ];
        for (name, value) in attributes {
            let _ = match value {
                Some(value) => element.set_attribute(name, &value),
                None => element.remove_attribute(name),
            };
        }
    });

    let label_text = match (names_component, own_name) {
        (true, _) => Some(ctx.label.get_value()),
        (false, name) => name,
    };
    let instance = ctx.instance.to_string();
    let part_name = format!("{part:?}");

    if takes_text {
        let text_item = item.clone();
        let text = move || {
            ctx.machine
                .with(|m| m.text(part, text_item.as_deref()))
                .unwrap_or_default()
        };
        let input_item = item.clone();
        let on_input = move |ev: leptos::ev::Event| {
            let value = event_target_value(&ev);
            ctx.dispatch(Input::Text {
                part,
                item: input_item.clone(),
                value,
            });
        };
        return view! {
            <input
                type="text"
                role=(role == Some(Role::SpinButton)).then_some("spinbutton")
                aria-label=label_text
                aria-valuenow=move || range().map(|r| r.now.to_string())
                aria-valuemin=move || range().map(|r| r.min.to_string())
                aria-valuemax=move || range().map(|r| r.max.to_string())
                aria-disabled=move || semantics.get().disabled.then_some("true")
                autocomplete="off"
                prop:value=text
                id=own_id
                data-armature=instance
                data-armature-part=part_name
                data-value=item
                on:input=on_input
                on:focus=on_focus
                on:keydown=on_keydown
            />
        }
        .into_any();
    }

    view! {
        <div
            node_ref=element
            id=own_id
            role=move || semantics.get().role.map(aria_role)
            aria-label=label_text
            hidden=move || semantics.get().hidden
            style:display=move || semantics.get().hidden.then_some("none")
            style:position=placement.map(|_| "fixed")
            style:touch-action=tracks_pointer.then_some("none")
            tabindex=tabindex
            data-armature=instance
            data-armature-part=part_name
            data-armature-item=registers
            data-value=item
            on:click=on_click
            on:pointerdown=on_pointerdown
            on:focus=on_focus
            on:keydown=on_keydown
            on:pointerenter=move |ev| on_enter(true, ev)
            on:pointerleave=move |ev| on_leave(false, ev)
            on:scroll=move |_| {
                if scrolls {
                    report_scroll();
                }
            }
            on:paste=on_paste
        >
            {children.map(|children| children())}
        </div>
    }
    .into_any()
}
