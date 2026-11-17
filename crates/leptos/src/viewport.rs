use armature_core::geometry::Rect;
use armature_core::scroll::avoid_keyboard;
use leptos::prelude::*;
use send_wrapper::SendWrapper;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

fn visible_now() -> Rect {
    let size = |value: Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>| {
        value.ok().and_then(|value| value.as_f64()).unwrap_or(0.0)
    };
    match window().visual_viewport() {
        Some(visible) => Rect {
            x: visible.offset_left(),
            y: visible.offset_top(),
            width: visible.width(),
            height: visible.height(),
        },
        None => Rect {
            x: 0.0,
            y: 0.0,
            width: size(window().inner_width()),
            height: size(window().inner_height()),
        },
    }
}

/// The part of the window that is actually showing, which shrinks when an on-screen
/// keyboard opens or the person pinch-zooms. It updates as that changes.
pub fn use_visible_viewport() -> ReadSignal<Rect> {
    let (visible, set_visible) = signal(visible_now());
    if let Some(target) = window().visual_viewport() {
        let update = SendWrapper::new(Closure::<dyn Fn()>::new(move || {
            set_visible.set(visible_now())
        }));
        let target = SendWrapper::new(target);
        for event in ["resize", "scroll"] {
            let _ = target.add_event_listener_with_callback(event, update.as_ref().unchecked_ref());
        }
        on_cleanup(move || {
            for event in ["resize", "scroll"] {
                let _ = target
                    .remove_event_listener_with_callback(event, update.as_ref().unchecked_ref());
            }
        });
    }
    visible
}

/// How far to lift content so that `field` clears an on-screen keyboard, keeping
/// `margin` between it and the keyboard.
pub fn keyboard_lift(field: &web_sys::Element, visible: Rect, margin: f64) -> f64 {
    let bounds = field.get_bounding_client_rect();
    // The field is measured against the window; the visible part against the page.
    let field = Rect {
        x: bounds.x(),
        y: bounds.y() + visible.y,
        width: bounds.width(),
        height: bounds.height(),
    };
    avoid_keyboard(field, visible, margin)
}
