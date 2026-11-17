//! The agent bridge: every `Machine` registers here, and an agent reads and operates
//! components through `snapshot` and `act` instead of screenshots and synthetic input.

use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::sync::mpsc::Sender;

use armature_core::agent::{Error, Registry, Snapshot};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::{App, Global};
use serde_json::{Value, json};

#[derive(Default)]
struct Agents(Registry<App>);

impl Global for Agents {}

pub(crate) fn registry(cx: &mut App) -> &mut Registry<App> {
    &mut cx.default_global::<Agents>().0
}

/// Every live component as `(ref, snapshot)`.
pub fn snapshot(cx: &App) -> Vec<(String, Snapshot)> {
    cx.try_global::<Agents>()
        .map(|agents| agents.0.snapshot(cx))
        .unwrap_or_default()
}

/// Runs one action and returns the component's value afterwards.
pub fn act(
    cx: &mut App,
    reference: &str,
    action: &str,
    item: Option<&str>,
) -> Result<Vec<String>, Error> {
    let target = registry(cx).target(reference)?;
    target.act(cx, action, item)
}

/// Answers one JSON request with one JSON response. Requests are
/// `{"method":"snapshot"}` and `{"method":"act","ref":..,"action":..,"item":..}`.
pub fn handle(request: &str, cx: &mut App) -> String {
    let failure = |error: String| json!({ "ok": false, "error": error });
    let response = match serde_json::from_str::<Value>(request) {
        Err(error) => failure(format!("invalid JSON: {error}")),
        Ok(request) => match request["method"].as_str() {
            Some("snapshot") => {
                let components: Vec<Value> = snapshot(cx).iter().map(component).collect();
                json!({ "ok": true, "components": components })
            }
            Some("act") => match (request["ref"].as_str(), request["action"].as_str()) {
                (Some(reference), Some(action)) => {
                    match act(cx, reference, action, request["item"].as_str()) {
                        Ok(value) => json!({ "ok": true, "value": value }),
                        Err(error) => failure(error.to_string()),
                    }
                }
                _ => failure("act needs \"ref\" and \"action\"".into()),
            },
            _ => failure("unknown method; valid methods: snapshot, act".into()),
        },
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
                let _ = converse(stream, requests);
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
    requests: UnboundedSender<Request>,
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
