//! The agent bridge: every `Machine` registers here, and an agent reads and operates
//! components through `snapshot` and `act` instead of screenshots and synthetic input.

use armature_core::agent::{Error, Registry, Snapshot};
use gpui::{App, Global};

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
