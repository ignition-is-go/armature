use std::cell::Cell;
use std::rc::Rc;

use armature_core::agent::{ActError, Target};
use armature_core::change::ValueChange;
use armature_core::keys::{Key, Modifiers, Orientation};
use armature_core::popup::{self, Placement, Rect, Side, Size};
use armature_core::primitive::{FocusTarget, Focusable, Input, Item, PartOf, Primitive, Role};
use leptos::prelude::*;
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
        let rendered = self.rendered();
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

        // The gate runs user code, so it gets a copy instead of a borrow of the signal.
        let mut machine = self.machine.get_untracked();
        machine.set_order(&order);
        let reaction = machine.send(input, &mut |change| allows(self.gate, change));
        self.machine.set(machine);

        if let Some(target) = reaction.focus {
            self.focus(&rendered, &target);
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

    fn element_of(self, part: P::Part) -> Option<HtmlElement> {
        let name = format!("{part:?}");
        self.rendered()
            .into_iter()
            .find(|r| r.part == name)
            .map(|r| r.element)
    }

    fn focus(self, rendered: &[Rendered], target: &FocusTarget<P::Part>) {
        let part = format!("{:?}", target.part);
        let Some(found) = rendered
            .iter()
            .find(|r| r.part == part && r.item == target.item)
        else {
            return;
        };
        let element = found.element.clone();
        let _ = element.focus();
        // A target inside a popup that is still hidden cannot take focus until the
        // popup has rendered.
        let focused = document()
            .active_element()
            .is_some_and(|active| active.is_same_node(Some(&element)));
        if !focused {
            request_animation_frame(move || {
                let _ = element.focus();
            });
        }
    }

    fn place(self, popup: &HtmlElement, placement: Placement<P::Part>) {
        let Some(anchor) = self.element_of(placement.anchor) else {
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

fn key_from_dom(key: &str) -> Option<Key> {
    Some(match key {
        "ArrowUp" => Key::ArrowUp,
        "ArrowDown" => Key::ArrowDown,
        "ArrowLeft" => Key::ArrowLeft,
        "ArrowRight" => Key::ArrowRight,
        "Home" => Key::Home,
        "End" => Key::End,
        "Enter" => Key::Enter,
        " " => Key::Space,
        "Escape" => Key::Escape,
        "Tab" => Key::Tab,
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
    }
}

/// Owns one primitive's state and makes it available to the `Part`s inside.
#[component]
pub fn Root<P>(
    machine: P,
    /// Accessible name, also how agents find this component.
    #[prop(into)]
    label: String,
    #[prop(optional, into)] on_value_change: Option<ChangeGate>,
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
    };
    provide_context(ctx);

    let registration = agent::register(Target {
        snapshot: Rc::new(move |_| machine.try_with_untracked(|m| m.snapshot(&label.get_value()))),
        act: Rc::new(move |_, action, item| {
            let mut next = machine.try_get_untracked().ok_or(ActError::Refused)?;
            next.act(action, item, &mut |change| allows(on_value_change, change))?;
            let value = next.value();
            machine.set(next);
            Ok(value)
        }),
    });
    on_cleanup(move || agent::unregister(registration));

    if P::parts().iter().any(|part| P::popup_of(*part).is_some()) {
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

    children()
}

/// One part of the primitive owned by the nearest `Root`, rendered as an unstyled
/// `div` carrying the part's semantics and event wiring.
#[component]
pub fn Part<K>(
    part: K,
    /// The item this part belongs to. Required for item parts.
    #[prop(optional, into)]
    item: Option<String>,
    /// Text for typeahead and agents. Defaults to the rendered text.
    #[prop(optional, into)]
    label: String,
    #[prop(optional)] disabled: bool,
    children: Children,
) -> impl IntoView
where
    K: PartOf,
    K::Primitive: Send + Sync,
{
    let ctx = expect_context::<Context<K::Primitive>>();
    let registers = K::Primitive::is_item(part);
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
    let interactive = move || semantics.get_untracked().focusable != Focusable::No;
    let names_component = item.is_none() && semantics.get_untracked().role.is_some();

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
    }

    let on_click = {
        let item = item.clone();
        move |ev: leptos::ev::MouseEvent| {
            if !interactive() {
                return;
            }
            let mods = Modifiers {
                shift: ev.shift_key(),
                ctrl: ev.ctrl_key() || ev.meta_key(),
            };
            ctx.dispatch(Input::Press {
                part,
                item: item.clone(),
                mods,
            });
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
        if !interactive() {
            return;
        }
        let Some(key) = key_from_dom(&ev.key()) else {
            return;
        };
        let mods = Modifiers {
            shift: ev.shift_key(),
            ctrl: ev.ctrl_key() || ev.meta_key(),
        };
        let now_ms = js_sys::Date::now() as u64;
        if ctx.dispatch(Input::Key { key, mods, now_ms }) {
            ev.prevent_default();
            ev.stop_propagation();
        }
    };

    view! {
        <div
            node_ref=element
            role=move || semantics.get().role.map(aria_role)
            aria-label=names_component.then(|| ctx.label.get_value())
            aria-selected=move || semantics.get().selected.map(|s| s.to_string())
            aria-expanded=move || semantics.get().expanded.map(|e| e.to_string())
            aria-haspopup=move || (semantics.get().role == Some(Role::ComboBox)).then_some("listbox")
            aria-multiselectable=move || semantics.get().multiselectable.map(|m| m.to_string())
            aria-orientation=move || {
                semantics.get().orientation.map(|o| match o {
                    Orientation::Vertical => "vertical",
                    Orientation::Horizontal => "horizontal",
                })
            }
            aria-disabled=move || semantics.get().disabled.then_some("true")
            hidden=move || semantics.get().hidden
            style:display=move || semantics.get().hidden.then_some("none")
            tabindex=move || match semantics.get().focusable {
                Focusable::No => None,
                Focusable::Programmatic => Some("-1"),
                Focusable::TabStop => Some("0"),
            }
            style:position=placement.map(|_| "fixed")
            data-active=move || semantics.get().active
            data-armature=ctx.instance.to_string()
            data-armature-part=format!("{part:?}")
            data-armature-item=registers
            data-value=item
            on:click=on_click
            on:focus=on_focus
            on:keydown=on_keydown
        >
            {children()}
        </div>
    }
}
