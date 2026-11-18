//! The agent bridge: every `Machine` registers here, and an agent reads and operates
//! components through `snapshot` and `act` instead of screenshots and synthetic input.

use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::sync::mpsc::Sender;

use armature_core::agent::{
    Entry, Error, Found, Registry, Run, Snapshot, Step, TOOLS, describe_entry,
};
use armature_core::change::Source;
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::{App, Global};
use serde_json::{Value, json};

#[derive(Default)]
struct Agents {
    registry: Registry<App>,
    history: Vec<Entry>,
}

impl Global for Agents {}

pub(crate) fn registry(cx: &mut App) -> &mut Registry<App> {
    &mut cx.default_global::<Agents>().registry
}

/// A copy of the registry, so the app is not borrowed while application code runs.
fn registry_copy(cx: &App) -> Registry<App> {
    cx.try_global::<Agents>()
        .map(|agents| agents.registry.clone())
        .unwrap_or_default()
}

/// Remembers a change that was committed, if it was one. A secret component's values
/// are never kept.
pub(crate) fn record(
    label: String,
    before: Vec<String>,
    after: Vec<String>,
    source: Source,
    secret: bool,
    cx: &mut App,
) {
    if let Some(entry) = Entry::committed(label, before, after, source, secret) {
        cx.default_global::<Agents>().history.push(entry);
    }
}

/// Every live component an agent may see, as `(ref, snapshot)`.
pub fn snapshot(cx: &App) -> Vec<(String, Snapshot)> {
    registry_copy(cx).snapshot(cx)
}

/// Runs one action and returns the component's value afterwards.
///
/// # Errors
///
/// When no component has that ref, or it refuses the action.
pub fn act(
    cx: &mut App,
    reference: &str,
    action: &str,
    item: Option<&str>,
) -> Result<Vec<String>, Error> {
    registry_copy(cx).target(reference)?.act(cx, action, item)
}

/// Components and items whose labels match `text`, best match first.
pub fn query(cx: &App, text: &str) -> Vec<Found> {
    registry_copy(cx).query(cx, text)
}

/// Runs steps in order, stopping at the first failure. `approve` is asked before any
/// step on a component that needs a person's approval.
pub fn run(cx: &mut App, steps: &[Step], approve: impl FnMut(&Step, &str) -> bool) -> Run {
    armature_core::agent::run(registry_copy, cx, steps, approve)
}

/// Every committed change so far, oldest first.
pub fn history(cx: &App) -> Vec<Entry> {
    cx.try_global::<Agents>()
        .map(|agents| agents.history.clone())
        .unwrap_or_default()
}

/// Answers one JSON request with one JSON response.
///
/// The methods are `snapshot`, `query` (with `text`), `act` (with `ref`, `action` and
/// `item`), `run` (with `steps`), `history` and `tools`. A `run` over this channel has
/// nobody to ask, so a step that needs a person's approval is refused.
pub fn handle(request: &str, cx: &mut App) -> String {
    let failure = |error: String| json!({ "ok": false, "error": error });
    let request = match serde_json::from_str::<Value>(request) {
        Ok(request) => request,
        Err(error) => return failure(format!("invalid JSON: {error}")).to_string(),
    };
    let text = |key: &str| request.get(key).and_then(Value::as_str);
    let response = match text("method") {
        Some("snapshot") => {
            let components: Vec<Value> = snapshot(cx).iter().map(component).collect();
            json!({ "ok": true, "components": components })
        }
        Some("query") => {
            let found: Vec<Value> = query(cx, text("text").unwrap_or_default())
                .into_iter()
                .map(|found| json!({ "ref": found.reference, "item": found.item, "label": found.label }))
                .collect();
            json!({ "ok": true, "found": found })
        }
        Some("act") => match (text("ref"), text("action")) {
            (Some(reference), Some(action)) => match act(cx, reference, action, text("item")) {
                Ok(value) => json!({ "ok": true, "value": value }),
                Err(error) => failure(error.to_string()),
            },
            _ => failure("act needs \"ref\" and \"action\"".into()),
        },
        Some("run") => {
            let steps: Vec<Step> = request
                .get("steps")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|step| Step {
                    target: step["target"]
                        .as_str()
                        .or_else(|| step["ref"].as_str())
                        .unwrap_or_default()
                        .to_string(),
                    action: step["action"].as_str().unwrap_or_default().to_string(),
                    item: step["item"].as_str().map(str::to_string),
                })
                .collect();
            let outcome = run(cx, &steps, |_, _| false);
            let changes: Vec<Value> = outcome
                .changes
                .iter()
                .map(|change| {
                    json!({
                        "ref": change.reference,
                        "label": change.label,
                        "before": change.before,
                        "after": change.after,
                    })
                })
                .collect();
            let failed = outcome
                .failed
                .as_ref()
                .map(|(step, error)| json!({ "step": step, "error": error.to_string() }));
            json!({ "ok": outcome.ok(), "changes": changes, "appeared": outcome.appeared, "failed": failed })
        }
        Some("history") => {
            let entries: Vec<Value> = history(cx)
                .iter()
                .map(|entry| {
                    json!({
                        "text": describe_entry(entry),
                        "label": entry.label,
                        "before": entry.before,
                        "after": entry.after,
                        "source": if entry.source == Source::Agent { "agent" } else { "user" },
                    })
                })
                .collect();
            json!({ "ok": true, "entries": entries })
        }
        Some("tools") => {
            let tools: Vec<Value> = TOOLS
                .iter()
                .map(|tool| {
                    let schema: Value = serde_json::from_str(tool.input_schema).unwrap_or(Value::Null);
                    json!({ "name": tool.name, "description": tool.description, "inputSchema": schema })
                })
                .collect();
            json!({ "ok": true, "tools": tools })
        }
        _ => failure(
            "unknown method; valid methods: snapshot, query, act, run, history, tools".into(),
        ),
    };
    response.to_string()
}

fn component((reference, snapshot): &(String, Snapshot)) -> Value {
    let items: Vec<Value> = snapshot
        .items
        .iter()
        .map(|item| {
            json!({
                "key": item.key,
                "label": item.label,
                "disabled": item.disabled,
                "selected": item.selected,
            })
        })
        .collect();
    json!({
        "ref": reference,
        "name": snapshot.name,
        "label": snapshot.label,
        "value": snapshot.value,
        "expanded": snapshot.expanded,
        "actions": snapshot.actions,
        "items": items,
    })
}

type Request = (String, Sender<String>);

/// Lets an agent outside the process reach the bridge over a Unix socket that only
/// this user can open. Each line sent is a request for [`handle`]; each gets one
/// line back.
///
/// # Errors
///
/// When the socket cannot be made at `path`.
#[cfg(unix)]
pub fn serve(path: impl AsRef<Path>, cx: &mut App) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;

    let path = path.as_ref();
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;

    // Sockets block, so they live on their own threads and hand each request to the
    // main thread, the only place the app's state can be touched.
    let (requests, mut inbox) = unbounded::<Request>();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let requests = requests.clone();
            std::thread::spawn(move || {
                let _ = converse(stream, &requests);
            });
        }
    });
    cx.spawn(async move |cx| {
        while let Some((request, reply)) = inbox.next().await {
            let response = cx.update(|cx| handle(&request, cx));
            let _ = reply.send(response);
        }
    })
    .detach();
    Ok(())
}

#[cfg(unix)]
fn converse(
    stream: std::os::unix::net::UnixStream,
    requests: &UnboundedSender<Request>,
) -> io::Result<()> {
    let mut writer = stream.try_clone()?;
    for line in BufReader::new(stream).lines() {
        let (reply, response) = std::sync::mpsc::channel();
        if requests.unbounded_send((line?, reply)).is_err() {
            break;
        }
        let Ok(response) = response.recv() else { break };
        writeln!(writer, "{response}")?;
    }
    Ok(())
}
