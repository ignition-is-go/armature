use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use armature_core::agent::{Access, ActError, Target};
use armature_core::change::{Source, ValueChange};
use armature_core::geometry::Point;
use armature_core::keys::{Key, Modifiers, Orientation};
use armature_core::popup::{self, Placement, Rect, Side, Size};
use armature_core::presence::Presence;
use armature_core::primitive::{
    Effect as Ask, FocusTarget, Focusable, Input, Item, PartOf, Pointer, PointerPhase, Primitive,
    Reaction, Role,
};
use armature_core::safe_area::{Axis, SafeArea};
use leptos::prelude::*;
use send_wrapper::SendWrapper;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::HtmlElement;

use crate::agent;

/// Called with each proposed change. Returning `false` cancels it.
pub type ChangeGate = Callback<ValueChange, bool>;

thread_local! {
        static WATCHING_MODALITY: Cell<bool> = const { Cell::new(false) };
    /// Where focus was before a component's hotkey took it, by component.
    static RETURN_TO: std::cell::RefCell<std::collections::HashMap<u32, HtmlElement>> = std::cell::RefCell::new(Default::default());
    /// The popups and dialogs open on the page, in the order they opened.
    static LAYERS: std::cell::RefCell<armature_core::layers::LayerStack> = std::cell::RefCell::new(Default::default());
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
                    let across =
                        self.machine.with_untracked(|m| m.scroll_axis()) == Orientation::Horizontal;
                    if let Some(element) = scrolling.and_then(|part| self.element_of(part, None)) {
                        if across {
                            element.set_scroll_left(offset as i32);
                        } else {
                            element.set_scroll_top(offset as i32);
                        }
                    }
                }
                Ask::Copy { text } => {
                    let _ = window().navigator().clipboard().write_text(&text);
                }
                Ask::RestoreFocus => {
                    let back = RETURN_TO.with_borrow_mut(|held| held.remove(&self.instance));
                    if let Some(back) = back {
                        let _ = back.focus();
                    }
                }
                Ask::PickFiles { multiple, accept } => {
                    // The browser only shows its picker from a file input, so one is
                    // made for the occasion.
                    let Ok(input) = document().create_element("input") else {
                        continue;
                    };
                    let input: web_sys::HtmlInputElement = input.unchecked_into();
                    input.set_type("file");
                    input.set_multiple(multiple);
                    input.set_accept(&accept);
                    let chosen = input.clone();
                    let picked = Closure::<dyn Fn()>::new(move || {
                        let files = crate::files::remember(chosen.files());
                        self.dispatch(Input::Files {
                            files,
                            dropped: false,
                        });
                    });
                    let _ = input.add_event_listener_with_callback(
                        "change",
                        picked.as_ref().unchecked_ref(),
                    );
                    picked.forget();
                    input.click();
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
        // A popup that belongs to an item sits against that item's own anchor.
        let item = popup.get_attribute("data-value");
        let own = item
            .as_deref()
            .and_then(|item| self.element_of(placement.anchor, Some(item)));
        // A popup called up at a point, as a context menu is, sits against that point.
        let point = self.machine.with_untracked(|m| m.popup_point());
        let at_point = point.and_then(|point| {
            web_sys::DomRect::new_with_x_and_y_and_width_and_height(point.x, point.y, 0.0, 0.0).ok()
        });
        let anchor = match at_point {
            Some(rect) => rect,
            None => match own.or_else(|| self.element_of(placement.anchor, None)) {
                Some(anchor) => anchor.get_bounding_client_rect(),
                None => return,
            },
        };
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
        // Where the arrow's middle goes, and whether it had to stop short of a
        // corner instead of pointing at the middle of the anchor.
        let popup_box = Rect::new(placed.x, placed.y, popup_size.width, popup_size.height);
        let (arrow, clamped) = popup::arrow_along(
            anchor_box,
            popup_box,
            Size {
                width: 0.0,
                height: 0.0,
            },
            false,
            12.0,
        );
        let _ = style.set_property("--armature-arrow-x", &format!("{arrow}px"));
        let _ = if clamped {
            popup.set_attribute("data-arrow-clamped", "")
        } else {
            popup.remove_attribute("data-arrow-clamped")
        };
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

pub(crate) fn key_from_dom(key: &str) -> Option<Key> {
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
        "F2" => Key::F2,
        "F6" => Key::F6,
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
        Role::Checkbox => "checkbox",
        Role::Menu => "menu",
        Role::MenuItem => "menuitem",
        Role::MenuItemCheckbox => "menuitemcheckbox",
        Role::MenuItemRadio => "menuitemradio",
        Role::MenuBar => "menubar",
        Role::Meter => "meter",
        Role::ProgressBar => "progressbar",
        Role::Navigation => "navigation",
        Role::Link => "link",
        Role::Image => "img",
        Role::Status => "status",
        Role::Alert => "alert",
        Role::ScrollBar => "scrollbar",
        Role::AlertDialog => "alertdialog",
        Role::Feed => "feed",
        Role::Article => "article",
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
    /// Settings the owner changes while the component runs, as name and value: a
    /// combobox's `status`, for one. Each is passed to `Primitive::set`.
    #[prop(optional, into)]
    settings: Option<Signal<Vec<(String, String)>>>,
    children: Children,
) -> impl IntoView
where
    P: Primitive + Send + Sync,
{
    // Each root gets an owner of its own. Without one, its context would sit on
    // the owner it shares with its siblings, and a later root of the same
    // primitive would take over any part this one creates later, such as a menu's
    // items when it opens.
    let owner = Owner::current()
        .expect("a Root is rendered inside a reactive owner")
        .child();
    let view = owner.with(move || {
        root_view(
            machine,
            label,
            on_value_change,
            value,
            agent_hidden,
            agent_secret,
            agent_confirm,
            settings,
            children,
        )
    });
    leptos::tachys::reactive_graph::OwnedView::new_with_owner(view, owner)
}

#[allow(clippy::too_many_arguments)]
fn root_view<P>(
    machine: P,
    label: String,
    on_value_change: Option<ChangeGate>,
    value: Option<Signal<Vec<String>>>,
    agent_hidden: bool,
    agent_secret: bool,
    agent_confirm: Option<String>,
    settings: Option<Signal<Vec<(String, String)>>>,
    children: Children,
) -> AnyView
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
            let wanted = next.wanted_scroll();
            machine.set(next);
            // An agent action cannot ask for a scroll, so the part is brought to
            // wherever the primitive now wants it.
            let scrolling = P::parts().iter().copied().find(|part| P::scrolls(*part));
            if let (Some(offset), Some(part)) = (wanted, scrolling)
                && let Some(element) = ctx.element_of(part, None)
            {
                element.set_scroll_top(offset.round() as i32);
            }
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
            // A press while a dialog is up belongs to the dialog, not to what is under it.
            let inert = LAYERS.with_borrow(|layers| layers.is_inert(&instance.to_string()));
            if !pressed_inside && !inert {
                ctx.dispatch(Input::PressOutside);
            }
        });
        on_cleanup(move || listener.remove());
        // Switching to another window or tab closes what is open, as a press elsewhere does.
        let away = window_event_listener(leptos::ev::blur, move |_| {
            ctx.dispatch(Input::WindowBlur);
        });
        on_cleanup(move || away.remove());
    }

    if let Some(settings) = settings {
        Effect::new(move |_| {
            let settings = settings.get();
            machine.update(|m| {
                for (name, value) in &settings {
                    m.set(name, value);
                }
            });
        });
    }

    // A key that reaches the component from anywhere on the page.
    // The keys are read as they stand when the key is pressed, since they may change.
    if !machine.with_untracked(|m| m.hotkeys()).is_empty() {
        let inside = format!("[data-armature=\"{instance}\"]");
        let listener = window_event_listener(leptos::ev::keydown, move |ev| {
            let mods = modifiers(ev.shift_key(), ev.ctrl_key() || ev.meta_key(), ev.alt_key());
            let pressed = key_from_dom(&ev.key());
            let keys = machine.with_untracked(|m| m.hotkeys());
            let Some(shortcut) = keys
                .into_iter()
                .find(|shortcut| pressed.is_some_and(|key| shortcut.matches(key, mods)))
            else {
                return;
            };
            ev.prevent_default();
            // Where focus was is remembered, so the key can send it back.
            let active = document().active_element();
            let outside = active
                .as_ref()
                .is_some_and(|active| !matches!(active.closest(&inside), Ok(Some(_))));
            if let Some(active) = active
                .filter(|_| outside)
                .and_then(|active| active.dyn_into::<HtmlElement>().ok())
            {
                RETURN_TO.with_borrow_mut(|held| held.insert(instance, active));
            }
            ctx.dispatch(Input::Hotkey { shortcut });
        });
        on_cleanup(move || {
            listener.remove();
            RETURN_TO.with_borrow_mut(|held| held.remove(&instance));
        });
    }

    // Whether the keyboard or a pointer is in use is noted on the page, once for
    // every component, so focus can be styled for each.
    if !WATCHING_MODALITY.replace(true) {
        let note = |how: &'static str| {
            move || {
                if let Some(root) = document().document_element() {
                    let _ = root.set_attribute("data-armature-modality", how);
                }
            }
        };
        let (keys, pointer) = (note("keyboard"), note("pointer"));
        // These last as long as the page does.
        std::mem::forget(window_event_listener(leptos::ev::keydown, move |_| keys()));
        std::mem::forget(window_event_listener(leptos::ev::pointerdown, move |_| {
            pointer()
        }));
    }

    // A primitive that lays itself out from where its parts are drawn hears of it
    // after every change and whenever the window is resized.
    if machine.with_untracked(|m| m.measures()) {
        let report = move || {
            let measure = move |part: P::Part, item: Option<&str>| {
                let rect = ctx.element_of(part, item)?.get_bounding_client_rect();
                Some(Rect::new(rect.x(), rect.y(), rect.width(), rect.height()))
            };
            let changed = machine
                .try_update_untracked(|m| m.measured(&measure))
                .unwrap_or(false);
            if changed {
                machine.notify();
            }
        };
        Effect::new(move |_| {
            machine.track();
            request_animation_frame(report);
        });
        let resized = window_event_listener(leptos::ev::resize, move |_| report());
        on_cleanup(move || resized.remove());
    }

    // A primitive whose value was set from outside may have a timer to start.
    {
        let waking = StoredValue::new(false);
        Effect::new(move |_| {
            let Some(delay) = machine.with(|m| m.wake_after()) else {
                return;
            };
            if waking.get_value() {
                return;
            }
            waking.set_value(true);
            set_timeout(
                move || {
                    waking.set_value(false);
                    ctx.dispatch(Input::Tick {
                        now_ms: js_sys::Date::now() as u64,
                    });
                },
                Duration::from_millis(delay),
            );
        });
    }

    // Anything that opens joins the stack of layers while it is open.
    {
        let id = instance.to_string();
        let layer_id = id.clone();
        Effect::new(move |was: Option<Option<bool>>| {
            let state = machine.with(|m| {
                let open = m.snapshot("").expanded == Some(true);
                let modal = parts.iter().any(|part| {
                    let semantics = m.semantics(*part, None);
                    semantics.modal && !semantics.hidden
                });
                (open || modal).then_some(modal)
            });
            if was != Some(state) {
                LAYERS.with_borrow_mut(|layers| match state {
                    Some(true) => layers.open(armature_core::layers::Layer::dialog(&id)),
                    Some(false) => layers.open(armature_core::layers::Layer::popup(&id)),
                    None => drop(layers.close(&id)),
                });
            }
            state
        });
        on_cleanup(move || LAYERS.with_borrow_mut(|layers| drop(layers.close(&layer_id))));
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
    // A part the primitive keeps text for is a native field. One that edits its
    // own content, key by key, is not.
    let takes_text = matches!(role, Some(Role::TextBox | Role::SpinButton))
        && ctx
            .machine
            .with_untracked(|m| m.text(part, item.as_deref()).is_some());

    let element = NodeRef::<leptos::html::Div>::new();
    // A part the person types into is an input, not a div.
    let field = NodeRef::<leptos::html::Input>::new();
    // A part that closes stays rendered until its exit animation, if it has one, ends.
    let presence = RwSignal::new(if semantics.get_untracked().hidden {
        Presence::Absent
    } else {
        Presence::Present
    });
    Effect::new(move |_| {
        let open = !semantics.get().hidden;
        let mut next = presence.get_untracked();
        if !next.set(open) {
            return;
        }
        presence.set(next);
        let Some(node) = element.get_untracked() else {
            presence.update(Presence::animation_ended);
            return;
        };
        let mark = if open {
            "data-starting-style"
        } else {
            "data-ending-style"
        };
        let _ = node.set_attribute(mark, "");
        let settle = move || {
            let _ = node.remove_attribute(mark);
            // Something newer may have happened while the animation ran.
            let expected = if open {
                Presence::Entering
            } else {
                Presence::Exiting
            };
            if presence.try_get_untracked() == Some(expected) {
                presence.update(Presence::animation_ended);
            }
        };
        if open {
            // One frame in the starting style gives a transition something to start from.
            request_animation_frame(settle);
            return;
        }
        let wait = element
            .get_untracked()
            .and_then(|node| running_animations(&node));
        match wait {
            Some(finished) => {
                let done = Closure::once_into_js(settle);
                let then = js_sys::Reflect::get(&finished, &"then".into()).ok();
                if let Some(then) = then
                    .as_ref()
                    .and_then(|then| then.dyn_ref::<js_sys::Function>())
                {
                    let _ = then.call2(&finished, &done, &done);
                }
            }
            None => settle(),
        }
    });
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
    let on_blur = {
        let item = item.clone();
        let instance = ctx.instance.to_string();
        move |ev: leptos::ev::FocusEvent| {
            // Focus moving to another part of the same component is not leaving it.
            let next = ev
                .related_target()
                .and_then(|target| target.dyn_into::<web_sys::Element>().ok());
            let stays = next
                .and_then(|next| next.closest("[data-armature]").ok().flatten())
                .is_some_and(|owner| {
                    owner.get_attribute("data-armature").as_deref() == Some(instance.as_str())
                });
            if stays {
                return;
            }
            let leave = {
                let item = item.clone();
                move || {
                    ctx.dispatch(Input::Blur {
                        part,
                        item: item.clone(),
                    });
                }
            };
            if ev.related_target().is_some() {
                leave();
                return;
            }
            // With nowhere named, focus may only have dropped because the focused
            // part was re-rendered, and is about to be put back. Look again once it
            // has had the chance.
            let instance = instance.clone();
            request_animation_frame(move || {
                request_animation_frame(move || {
                    let owner = document()
                        .active_element()
                        .and_then(|now| now.closest("[data-armature]").ok().flatten());
                    let inside = owner.is_some_and(|owner| {
                        owner.get_attribute("data-armature").as_deref() == Some(instance.as_str())
                    });
                    if !inside {
                        leave();
                    }
                });
            });
        }
    };
    // Find-in-page may search a hidden part that allows it, and says when it matched.
    {
        let item = item.clone();
        Effect::new(move |_| {
            let node = element.get().map(|node| web_sys::Element::clone(&node));
            let node = node.or_else(|| field.get().map(|node| web_sys::Element::clone(&node)));
            let Some(element) = node else { return };
            let item = item.clone();
            let for_context = item.clone();
            let for_wheel = item.clone();
            let found = Closure::<dyn Fn()>::new(move || {
                ctx.dispatch(Input::Found {
                    part,
                    item: item.clone(),
                });
            });
            let _ = element
                .add_event_listener_with_callback("beforematch", found.as_ref().unchecked_ref());
            found.forget();
            // The wheel goes to a part that asks for it at that moment, and scrolls the
            // page otherwise.
            {
                let item = for_wheel;
                let turned =
                    Closure::<dyn Fn(web_sys::WheelEvent)>::new(move |ev: web_sys::WheelEvent| {
                        if !ctx.machine.with_untracked(|m| m.wheels(part)) {
                            return;
                        }
                        let mods =
                            modifiers(ev.shift_key(), ev.ctrl_key() || ev.meta_key(), ev.alt_key());
                        // The page scrolls as usual unless the primitive made use of it.
                        if ctx.dispatch(Input::Wheel {
                            part,
                            item: item.clone(),
                            delta: -ev.delta_y() / 100.0,
                            mods,
                        }) {
                            ev.prevent_default();
                        }
                    });
                let _ = element
                    .add_event_listener_with_callback("wheel", turned.as_ref().unchecked_ref());
                turned.forget();
            }
            // The platform's gesture for a context menu goes to a part that takes it,// in place of the browser's own menu.
            if K::Primitive::context(part) {
                let item = for_context;
                let asked =
                    Closure::<dyn Fn(web_sys::MouseEvent)>::new(move |ev: web_sys::MouseEvent| {
                        ev.prevent_default();
                        let at = Point::new(ev.client_x() as f64, ev.client_y() as f64);
                        ctx.dispatch(Input::ContextMenu {
                            part,
                            item: item.clone(),
                            at,
                        });
                    });
                let _ = element.add_event_listener_with_callback(
                    "contextmenu",
                    asked.as_ref().unchecked_ref(),
                );
                asked.forget();
            }
            // Files dragged from outside the page may be dropped on a part that takes them.
            if K::Primitive::drops(part) {
                let over =
                    Closure::<dyn Fn(web_sys::DragEvent)>::new(move |ev: web_sys::DragEvent| {
                        // Without this the browser opens the file instead of handing it over.
                        ev.prevent_default();
                        if ev.type_() == "dragenter" {
                            ctx.dispatch(Input::DragOver { inside: true });
                        }
                    });
                let left = Closure::<dyn Fn()>::new(move || {
                    ctx.dispatch(Input::DragOver { inside: false });
                });
                let dropped =
                    Closure::<dyn Fn(web_sys::DragEvent)>::new(move |ev: web_sys::DragEvent| {
                        ev.prevent_default();
                        let files = crate::files::remember(
                            ev.data_transfer().and_then(|data| data.files()),
                        );
                        ctx.dispatch(Input::Files {
                            files,
                            dropped: true,
                        });
                    });
                for name in ["dragenter", "dragover"] {
                    let _ = element
                        .add_event_listener_with_callback(name, over.as_ref().unchecked_ref());
                }
                let _ = element
                    .add_event_listener_with_callback("dragleave", left.as_ref().unchecked_ref());
                let _ = element
                    .add_event_listener_with_callback("drop", dropped.as_ref().unchecked_ref());
                over.forget();
                left.forget();
                dropped.forget();
            }
            // A picture inside the part says when it has loaded or failed. Those events
            // do not bubble, so they are caught on the way down.
            for (name, ok) in [("load", true), ("error", false)] {
                let heard = Closure::<dyn Fn()>::new(move || {
                    ctx.dispatch(Input::Loaded { part, ok });
                });
                let _ = element.add_event_listener_with_callback_and_bool(
                    name,
                    heard.as_ref().unchecked_ref(),
                    true,
                );
                heard.forget();
            }
        });
    }
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
        // Apple platforms have their own keys for the ends of a list and for
        // stepping through one.
        let others = ev.shift_key() || ev.alt_key();
        let (key, mods) =
            match armature_core::keys::apple_key(key, ev.ctrl_key(), ev.meta_key(), others)
                .filter(|_| on_apple())
            {
                Some(meant) => (meant, Modifiers::default()),
                None => (key, mods),
            };
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
            if !hovers || ev.pointer_type() == "touch" {
                return;
            }
            let leave = {
                let item = item.clone();
                move || {
                    let now_ms = js_sys::Date::now() as u64;
                    ctx.dispatch(Input::Hover {
                        part,
                        item: item.clone(),
                        inside,
                        now_ms,
                    });
                }
            };
            // Leaving for a popup this component has open is not leaving yet: the
            // pointer may cross the gap, and cut the corner, on its way there.
            let popup = (!inside)
                .then(|| {
                    K::Primitive::parts()
                        .iter()
                        .copied()
                        .filter(|other| *other != part)
                        .find_map(|other| {
                            K::Primitive::popup_of(other)
                                .filter(|_| K::Primitive::hovers(other))?;
                            let found = ctx
                                .element_of(other, item.as_deref())
                                .or_else(|| ctx.element_of(other, None))?;
                            let rect = found.get_bounding_client_rect();
                            (rect.width() > 0.0)
                                .then(|| Rect::new(rect.x(), rect.y(), rect.width(), rect.height()))
                        })
                })
                .flatten();
            let Some(popup) = popup else {
                leave();
                return;
            };
            let from = Point::new(ev.client_x() as f64, ev.client_y() as f64);
            let axis = if from.y < popup.y || from.y > popup.bottom() {
                Axis::Y
            } else {
                Axis::X
            };
            let area = Rc::new(std::cell::RefCell::new(SafeArea::start(
                from,
                popup,
                axis,
                js_sys::Date::now() as u64,
            )));
            let last = Rc::new(Cell::new(from));
            // The shape spanning this part and its popup: a pointer inside it is
            // still on its way across.
            let trigger = element.get_untracked().map(|own| {
                let rect = own.get_bounding_client_rect();
                Rect::new(rect.x(), rect.y(), rect.width(), rect.height())
            });
            let bridged = move |at: Point| {
                trigger.is_some_and(|trigger| popup::bridges(trigger, popup, 4.0, at))
            };
            type Held = Rc<std::cell::RefCell<Option<(WindowListenerHandle, IntervalHandle)>>>;
            let held: Held = Rc::default();
            // Ends the wait, telling the primitive the pointer left unless it arrived.
            let finish = {
                let held = held.clone();
                move |arrived: bool| {
                    let Some((moves, ticks)) = held.borrow_mut().take() else {
                        return;
                    };
                    moves.remove();
                    ticks.clear();
                    if !arrived {
                        leave();
                    }
                }
            };
            let moves = window_event_listener(leptos::ev::pointermove, {
                let (area, last, finish) = (area.clone(), last.clone(), finish.clone());
                move |ev| {
                    let at = Point::new(ev.client_x() as f64, ev.client_y() as f64);
                    last.set(at);
                    if popup.contains(at) {
                        finish(true);
                    } else if !area.borrow_mut().contains(at, js_sys::Date::now() as u64)
                        && !bridged(at)
                    {
                        finish(false);
                    }
                }
            });
            // A pointer that stops short of the popup has left after all.
            let ticks = set_interval_with_handle(
                move || {
                    if !area
                        .borrow_mut()
                        .contains(last.get(), js_sys::Date::now() as u64)
                    {
                        finish(false);
                    }
                },
                Duration::from_millis(100),
            );
            match ticks {
                Ok(ticks) => *held.borrow_mut() = Some((moves, ticks)),
                Err(_) => moves.remove(),
            }
        }
    };
    let (on_enter, on_leave) = (hover.clone(), hover);
    let scrolls = K::Primitive::scrolls(part);
    let report_scroll = move || {
        if let Some(scroller) = element.get_untracked() {
            let across = ctx.machine.with_untracked(|m| m.scroll_axis()) == Orientation::Horizontal;
            let (offset, viewport, content) = if across {
                (
                    scroller.scroll_left(),
                    scroller.client_width(),
                    scroller.scroll_width(),
                )
            } else {
                (
                    scroller.scroll_top(),
                    scroller.client_height(),
                    scroller.scroll_height(),
                )
            };
            ctx.dispatch(Input::Scrolled {
                part,
                offset: offset as f64,
                viewport: viewport as f64,
                content: content as f64,
                now_ms: js_sys::Date::now() as u64,
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
        let node = element.get().map(|node| web_sys::Element::clone(&node));
        let node = node.or_else(|| field.get().map(|node| web_sys::Element::clone(&node)));
        let Some(element) = node else { return };
        let flag = |on: Option<bool>| on.map(|on| on.to_string());
        let number = |value: Option<f64>| value.map(|value| value.to_string());
        let attributes = [
            ("aria-selected", flag(state.selected)),
            ("aria-expanded", flag(state.expanded)),
            (
                "aria-checked",
                if state.mixed {
                    Some("mixed".to_string())
                } else {
                    flag(state.checked)
                },
            ),
            ("aria-pressed", flag(state.pressed)),
            ("aria-current", state.current.then(|| "true".to_string())),
            ("aria-invalid", state.invalid.then(|| "true".to_string())),
            ("aria-busy", state.busy.then(|| "true".to_string())),
            ("data-destructive", state.destructive.then(String::new)),
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
                node_ref=field
                type=move || if semantics.get().obscured { "password" } else { "text" }
                role=(role == Some(Role::SpinButton)).then_some("spinbutton")
                aria-label=label_text
                aria-valuenow=move || range().map(|r| r.now.to_string())
                aria-valuemin=move || range().map(|r| r.min.to_string())
                aria-valuemax=move || range().map(|r| r.max.to_string())
                aria-disabled=move || semantics.get().disabled.then_some("true")
                autocomplete="off"
                hidden=move || semantics.get().hidden
                style:display=move || semantics.get().hidden.then_some("none")
                prop:value=text
                id=own_id
                data-armature=instance
                data-armature-part=part_name
                data-value=item
                on:input=on_input
                on:focus=on_focus
                on:blur=on_blur
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
            hidden=move || {
                let findable = semantics.get().findable;
                (!presence.get().is_rendered()).then_some(if findable { "until-found" } else { "" })
            }
            style:display=move || {
                let findable = semantics.get().findable;
                (!presence.get().is_rendered() && !findable).then_some("none")
            }
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
            on:blur=on_blur
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

/// A promise that settles when every animation and transition running on `node` has
/// ended, or `None` when nothing is running.
fn running_animations(node: &web_sys::Element) -> Option<js_sys::Promise> {
    let get = js_sys::Reflect::get(node, &"getAnimations".into()).ok()?;
    let running: js_sys::Array = get
        .dyn_ref::<js_sys::Function>()?
        .call0(node)
        .ok()?
        .dyn_into()
        .ok()?;
    if running.length() == 0 {
        return None;
    }
    let finished: js_sys::Array = running
        .iter()
        .filter_map(|animation| js_sys::Reflect::get(&animation, &"finished".into()).ok())
        .collect();
    Some(js_sys::Promise::all_settled(&finished))
}

/// Whether the page is running on an Apple platform, where some keys mean other
/// things.
fn on_apple() -> bool {
    let platform = window().navigator().platform().unwrap_or_default();
    ["Mac", "iPhone", "iPad", "iPod"]
        .iter()
        .any(|name| platform.starts_with(name))
}
