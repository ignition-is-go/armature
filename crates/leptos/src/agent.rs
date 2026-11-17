//! The agent bridge: every mounted `Root` registers here, and an agent reads and
//! operates components through `snapshot` and `act` instead of screenshots and clicks.

use std::cell::RefCell;

use armature_core::agent::{Error, Registry, Snapshot, Target};
use js_sys::{Array, Object, Reflect};
use leptos::prelude::window;
use wasm_bindgen::prelude::*;

thread_local! {
    static REGISTRY: RefCell<Registry<()>> = RefCell::new(Registry::default());
}

pub(crate) fn register(target: Target<()>) -> u32 {
    REGISTRY.with_borrow_mut(|registry| registry.register(target))
}

pub(crate) fn unregister(id: u32) {
    REGISTRY.with_borrow_mut(|registry| registry.unregister(id));
}

/// Every mounted component as `(ref, snapshot)`.
pub fn snapshot() -> Vec<(String, Snapshot)> {
    REGISTRY.with_borrow(|registry| registry.snapshot(&()))
}

/// Runs one action and returns the component's value afterwards.
pub fn act(reference: &str, action: &str, item: Option<&str>) -> Result<Vec<String>, Error> {
    let target = REGISTRY.with_borrow(|registry| registry.target(reference))?;
    target.act(&mut (), action, item)
}

/// Exposes the bridge to page scripts as `window.armature.snapshot()` and
/// `window.armature.act(ref, action, item)`.
pub fn install() {
    let api = Object::new();

    let snapshot_fn = Closure::<dyn Fn() -> JsValue>::new(|| {
        snapshot()
            .iter()
            .map(|(r, s)| snapshot_to_js(r, s))
            .collect::<Array>()
            .into()
    });
    set(&api, "snapshot", snapshot_fn.as_ref());
    snapshot_fn.forget();

    let act_fn = Closure::<dyn Fn(String, String, Option<String>) -> JsValue>::new(
        |reference: String, action: String, item: Option<String>| {
            let result = Object::new();
            match act(&reference, &action, item.as_deref()) {
                Ok(value) => {
                    set(&result, "ok", &true.into());
                    set(&result, "value", &strings(&value));
                }
                Err(error) => {
                    set(&result, "ok", &false.into());
                    set(&result, "error", &error.to_string().into());
                }
            }
            result.into()
        },
    );
    set(&api, "act", act_fn.as_ref());
    act_fn.forget();

    set(&window(), "armature", &api);
}

fn snapshot_to_js(reference: &str, snapshot: &Snapshot) -> JsValue {
    let out = Object::new();
    set(&out, "ref", &reference.into());
    set(&out, "name", &snapshot.name.into());
    set(&out, "label", &snapshot.label.as_str().into());
    set(&out, "value", &strings(&snapshot.value));
    if let Some(expanded) = snapshot.expanded {
        set(&out, "expanded", &expanded.into());
    }
    let actions: Vec<String> = snapshot.actions.iter().map(|a| a.to_string()).collect();
    set(&out, "actions", &strings(&actions));
    let items: Array = snapshot
        .items
        .iter()
        .map(|item| {
            let js = Object::new();
            set(&js, "key", &item.key.as_str().into());
            set(&js, "label", &item.label.as_str().into());
            set(&js, "disabled", &item.disabled.into());
            set(&js, "selected", &item.selected.into());
            JsValue::from(js)
        })
        .collect();
    set(&out, "items", &items);
    out.into()
}

fn strings(values: &[String]) -> JsValue {
    values
        .iter()
        .map(|v| JsValue::from_str(v))
        .collect::<Array>()
        .into()
}

fn set(target: &JsValue, key: &str, value: &JsValue) {
    Reflect::set(target, &key.into(), value).expect("target is a plain object");
}
