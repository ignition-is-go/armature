//! The agent bridge: every mounted `Root` registers here, and an agent reads and
//! operates components through it instead of screenshots and clicks.

use std::cell::RefCell;

use armature_core::agent::{Entry, Error, Found, Registry, Run, Snapshot, Step, TOOLS, Target};
use armature_core::change::Source;
use js_sys::{Array, Function, Object, Reflect};
use leptos::prelude::window;
use wasm_bindgen::prelude::*;

thread_local! {
    static REGISTRY: RefCell<Registry<()>> = RefCell::new(Registry::default());
    static HISTORY: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn register(target: Target<()>) -> u32 {
    REGISTRY.with_borrow_mut(|registry| registry.register(target))
}

pub(crate) fn unregister(id: u32) {
    REGISTRY.with_borrow_mut(|registry| registry.unregister(id));
}

/// Remembers a change that was committed, if it was one. A secret component's values
/// are never kept.
pub(crate) fn record(
    label: String,
    before: Vec<String>,
    after: Vec<String>,
    source: Source,
    secret: bool,
) {
    if before == after {
        return;
    }
    let shown = |value: Vec<String>| {
        if secret {
            vec!["[redacted]".to_string()]
        } else {
            value
        }
    };
    let entry = Entry {
        label,
        before: shown(before),
        after: shown(after),
        source,
    };
    HISTORY.with_borrow_mut(|history| history.push(entry));
}

/// A copy of the registry, so no borrow is held while application code runs.
fn registry() -> Registry<()> {
    REGISTRY.with_borrow(Clone::clone)
}

/// Every mounted component an agent may see, as `(ref, snapshot)`.
pub fn snapshot() -> Vec<(String, Snapshot)> {
    registry().snapshot(&())
}

/// Runs one action and returns the component's value afterwards.
pub fn act(reference: &str, action: &str, item: Option<&str>) -> Result<Vec<String>, Error> {
    registry().target(reference)?.act(&mut (), action, item)
}

/// Components and items whose labels match `text`, best match first.
pub fn query(text: &str) -> Vec<Found> {
    registry().query(&(), text)
}

/// Runs steps in order, stopping at the first failure. `approve` is asked before any
/// step on a component that needs a person's approval.
pub fn run(steps: &[Step], approve: impl FnMut(&Step, &str) -> bool) -> Run {
    armature_core::agent::run(|_| registry(), &mut (), steps, approve)
}

/// Every committed change so far, oldest first.
pub fn history() -> Vec<Entry> {
    HISTORY.with_borrow(Clone::clone)
}

/// Exposes the bridge to page scripts as `window.armature`, with `snapshot`, `query`,
/// `act`, `run`, `history` and `tools`. `run` asks `window.armature.approve(step,
/// what)` before acting on a component that needs approval, and refuses if the page
/// has not defined it.
pub fn install() {
    let api = Object::new();
    expose(
        &api,
        "snapshot",
        Closure::<dyn Fn() -> JsValue>::new(|| {
            snapshot()
                .iter()
                .map(|(reference, s)| snapshot_to_js(reference, s))
                .collect::<Array>()
                .into()
        }),
    );
    expose(
        &api,
        "query",
        Closure::<dyn Fn(String) -> JsValue>::new(|text: String| {
            let found = query(&text).into_iter().map(|found| {
                let js = Object::new();
                set(&js, "ref", &found.reference.into());
                set(
                    &js,
                    "item",
                    &found.item.map(JsValue::from).unwrap_or(JsValue::NULL),
                );
                set(&js, "label", &found.label.into());
                JsValue::from(js)
            });
            found.collect::<Array>().into()
        }),
    );
    expose(
        &api,
        "act",
        Closure::<dyn Fn(String, String, Option<String>) -> JsValue>::new(
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
        ),
    );
    expose(
        &api,
        "run",
        Closure::<dyn Fn(JsValue) -> JsValue>::new(run_from_js),
    );
    expose(
        &api,
        "history",
        Closure::<dyn Fn() -> JsValue>::new(|| {
            let entries = history().into_iter().map(|entry| {
                let js = Object::new();
                set(
                    &js,
                    "text",
                    &armature_core::agent::describe_entry(&entry).into(),
                );
                set(&js, "label", &entry.label.into());
                set(&js, "before", &strings(&entry.before));
                set(&js, "after", &strings(&entry.after));
                set(
                    &js,
                    "source",
                    &if entry.source == Source::Agent {
                        "agent"
                    } else {
                        "user"
                    }
                    .into(),
                );
                JsValue::from(js)
            });
            entries.collect::<Array>().into()
        }),
    );
    expose(
        &api,
        "tools",
        Closure::<dyn Fn() -> JsValue>::new(|| {
            TOOLS.iter().map(tool_to_js).collect::<Array>().into()
        }),
    );
    expose(
        &api,
        "registerTools",
        Closure::<dyn Fn() -> bool>::new(register_tools),
    );
    set(&window(), "armature", &api);
}

/// Offers the tools to an agent built into the browser, where the browser supports
/// WebMCP. Returns whether they were registered. Call it after `install`.
pub fn register_tools() -> bool {
    let Ok(navigator) = Reflect::get(&window(), &"navigator".into()) else {
        return false;
    };
    let Ok(context) = Reflect::get(&navigator, &"modelContext".into()) else {
        return false;
    };
    let Ok(register) = Reflect::get(&context, &"registerTool".into()) else {
        return false;
    };
    let Some(register) = register.dyn_ref::<Function>() else {
        return false;
    };
    let Ok(api) = Reflect::get(&window(), &"armature".into()) else {
        return false;
    };

    for tool in &TOOLS {
        let js = tool_to_js(tool);
        let api = api.clone();
        let name = tool.name;
        let execute = Closure::<dyn Fn(JsValue) -> JsValue>::new(move |input: JsValue| {
            let field = |key: &str| Reflect::get(&input, &key.into()).unwrap_or(JsValue::UNDEFINED);
            let call = |method: &str, argument: JsValue| {
                let method = Reflect::get(&api, &method.into()).ok();
                let method = method
                    .as_ref()
                    .and_then(|method| method.dyn_ref::<Function>());
                method
                    .and_then(|method| method.call1(&api, &argument).ok())
                    .unwrap_or(JsValue::NULL)
            };
            match name {
                "query" => call("query", field("text")),
                "act" => call("run", field("steps")),
                _ => call("snapshot", JsValue::UNDEFINED),
            }
        });
        set(&js, "execute", execute.as_ref());
        execute.forget();
        if register.call1(&context, &js).is_err() {
            return false;
        }
    }
    true
}

fn run_from_js(steps: JsValue) -> JsValue {
    let text = |value: &JsValue, key: &str| {
        Reflect::get(value, &key.into())
            .ok()
            .and_then(|v| v.as_string())
    };
    let steps: Vec<(Step, JsValue)> = Array::from(&steps)
        .iter()
        .map(|js| {
            let step = Step {
                target: text(&js, "target")
                    .or_else(|| text(&js, "ref"))
                    .unwrap_or_default(),
                action: text(&js, "action").unwrap_or_default(),
                item: text(&js, "item"),
            };
            (step, js)
        })
        .collect();
    let plain: Vec<Step> = steps.iter().map(|(step, _)| step.clone()).collect();

    let approve = |step: &Step, what: &str| {
        let hook = Reflect::get(&window(), &"armature".into())
            .and_then(|api| Reflect::get(&api, &"approve".into()))
            .ok();
        let Some(hook) = hook.as_ref().and_then(|hook| hook.dyn_ref::<Function>()) else {
            return false;
        };
        let original = steps
            .iter()
            .find(|(s, _)| s == step)
            .map(|(_, js)| js.clone());
        let answer = hook.call2(
            &JsValue::NULL,
            &original.unwrap_or(JsValue::NULL),
            &what.into(),
        );
        answer.is_ok_and(|answer| answer.is_truthy())
    };
    let outcome = run(&plain, approve);

    let result = Object::new();
    set(&result, "ok", &outcome.ok().into());
    let changes = outcome.changes.iter().map(|change| {
        let js = Object::new();
        set(&js, "ref", &change.reference.as_str().into());
        set(&js, "label", &change.label.as_str().into());
        set(&js, "before", &strings(&change.before));
        set(&js, "after", &strings(&change.after));
        JsValue::from(js)
    });
    set(&result, "changes", &changes.collect::<Array>());
    set(&result, "appeared", &strings(&outcome.appeared));
    if let Some((index, error)) = outcome.failed {
        let failed = Object::new();
        set(&failed, "step", &(index as u32).into());
        set(&failed, "error", &error.to_string().into());
        set(&result, "failed", &failed);
    }
    result.into()
}

fn tool_to_js(tool: &armature_core::agent::Tool) -> Object {
    let js = Object::new();
    set(&js, "name", &tool.name.into());
    set(&js, "description", &tool.description.into());
    let schema = js_sys::JSON::parse(tool.input_schema).unwrap_or(JsValue::NULL);
    set(&js, "inputSchema", &schema);
    js
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

fn expose<F: ?Sized + wasm_bindgen::closure::WasmClosure>(
    api: &Object,
    name: &str,
    closure: Closure<F>,
) {
    set(api, name, closure.as_ref());
    closure.forget();
}

fn strings(values: &[String]) -> JsValue {
    values
        .iter()
        .map(|v| JsValue::from_str(v))
        .collect::<Array>()
        .into()
}

fn set(target: &JsValue, key: &str, value: &JsValue) {
    Reflect::set(target, &key.into(), value).expect("target is an object");
}
