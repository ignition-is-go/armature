//! Laying out panels that share a space and are resized by the handles between
//! them. Sizes are percentages of the space and always add up to a hundred.

const EPSILON: f64 = 1e-6;

fn same(a: f64, b: f64) -> bool {
    (a - b).abs() < EPSILON
}

/// A size as it is written for a panel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PanelSize {
    Percent(f64),
    /// Pixels, which only mean something once the space is measured.
    Px(f64),
    /// Multiples of the root font size.
    Rem(f64),
}

/// A size as a percentage of `space`. `None` for a pixel or rem size while the
/// space is not yet measured.
pub fn to_percent(size: PanelSize, space: f64, rem: f64) -> Option<f64> {
    match size {
        PanelSize::Percent(value) => Some(value),
        _ if space <= 0.0 => None,
        PanelSize::Px(value) => Some(value / space * 100.0),
        PanelSize::Rem(value) => Some(value * rem / space * 100.0),
    }
}

/// What a panel is written with. Anything left out takes its default.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PanelSizes {
    pub default_size: Option<PanelSize>,
    pub min_size: Option<PanelSize>,
    pub max_size: Option<PanelSize>,
    pub collapsible: bool,
    pub collapsed_size: Option<PanelSize>,
}

/// The limits a panel is held to, in percent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelConstraints {
    pub min: f64,
    pub max: f64,
    /// Whether it can shut to `collapsed`, below its minimum.
    pub collapsible: bool,
    pub collapsed: f64,
    /// The size it starts at, if it says.
    pub initial: Option<f64>,
}

impl Default for PanelConstraints {
    fn default() -> Self {
        PanelConstraints {
            min: 0.0,
            max: 100.0,
            collapsible: false,
            collapsed: 0.0,
            initial: None,
        }
    }
}

/// A panel's limits in percent of `space`. The collapsed size is kept at most the
/// minimum, and the maximum at least the minimum.
pub fn constraints_of(sizes: PanelSizes, space: f64, rem: f64) -> PanelConstraints {
    let percent = |size: Option<PanelSize>| size.and_then(|size| to_percent(size, space, rem));
    let min = percent(sizes.min_size).unwrap_or(0.0).clamp(0.0, 100.0);
    PanelConstraints {
        min,
        max: percent(sizes.max_size).unwrap_or(100.0).clamp(min, 100.0),
        collapsible: sizes.collapsible,
        collapsed: percent(sizes.collapsed_size).unwrap_or(0.0).clamp(0.0, min),
        initial: percent(sizes.default_size).map(|initial| initial.clamp(0.0, 100.0)),
    }
}

fn lowest(panel: &PanelConstraints) -> f64 {
    if panel.collapsible {
        panel.collapsed
    } else {
        panel.min
    }
}

fn is_collapsed(panel: &PanelConstraints, size: f64) -> bool {
    panel.collapsible && size <= panel.collapsed + EPSILON && panel.collapsed < panel.min
}

fn allowed(panel: &PanelConstraints, size: f64) -> bool {
    (size >= panel.min - EPSILON && size <= panel.max + EPSILON)
        || (panel.collapsible && same(size, panel.collapsed))
}

/// The size a panel takes when asked for `target` from `current`. Below its
/// minimum it either stays at the minimum or shuts, depending on whether the
/// target is past `threshold` of the way: half for a drag, all of it for a key.
pub fn settle(panel: &PanelConstraints, target: f64, current: f64, threshold: f64) -> f64 {
    if target >= panel.min - EPSILON {
        return target.clamp(panel.min, panel.max);
    }
    if !panel.collapsible {
        return panel.min;
    }
    let span = panel.min - panel.collapsed;
    if current <= panel.collapsed + EPSILON {
        return if target > panel.collapsed + span * (1.0 - threshold) + EPSILON {
            panel.min
        } else {
            panel.collapsed
        };
    }
    if target < panel.collapsed + span * threshold - EPSILON {
        panel.collapsed
    } else {
        panel.min
    }
}

/// The panels a handle can take from, nearest first. It stops at a locked handle.
fn shrinkable(count: usize, handle: usize, forward: bool, locked: &[bool]) -> Vec<usize> {
    let is_locked = |handle: usize| locked.get(handle).copied().unwrap_or(false);
    if forward {
        (handle + 1..count)
            .take_while(|index| *index == handle + 1 || !is_locked(index - 1))
            .collect()
    } else {
        (0..=handle)
            .rev()
            .take_while(|index| *index == handle || !is_locked(*index))
            .collect()
    }
}

/// The layout after moving a handle by `delta` percent. The panel before it grows
/// for a positive move and the one after for a negative one, taking from the
/// panels on the other side in turn: one at its minimum passes the rest on.
/// `locked` marks handles that cannot move, which the taking does not cross.
/// `None` when nothing can move.
pub fn resize_at(
    layout: &[f64],
    panels: &[PanelConstraints],
    handle: usize,
    delta: f64,
    threshold: f64,
    locked: &[bool],
) -> Option<Vec<f64>> {
    if delta.abs() < EPSILON || layout.len() < 2 || handle >= layout.len() - 1 {
        return None;
    }
    let forward = delta > 0.0;
    let pivot = if forward { handle } else { handle + 1 };
    let shrinking = shrinkable(layout.len(), handle, forward, locked);
    let start = layout[pivot];
    let room = panels[pivot].max - start;
    let capacity: f64 = shrinking
        .iter()
        .map(|index| (layout[*index] - lowest(&panels[*index])).max(0.0))
        .sum();
    let asked = delta.abs().min(room).min(capacity);
    let wanted = settle(&panels[pivot], start + asked, start, threshold) - start;
    if wanted <= EPSILON {
        return None;
    }
    let mut next = layout.to_vec();
    let mut given = 0.0;
    for index in shrinking {
        let remaining = wanted - given;
        if remaining <= EPSILON {
            break;
        }
        let (size, panel) = (layout[index], &panels[index]);
        let mut settled = size.min(settle(panel, size - remaining, size, threshold));
        if given + size - settled > room + EPSILON {
            settled = size.min(panel.min.max(size - remaining));
        }
        next[index] = settled;
        given += size - settled;
    }
    if given <= EPSILON || !allowed(&panels[pivot], start + given) {
        return None;
    }
    next[pivot] = start + given;
    Some(next)
}

/// How far Home moves a handle: until the panel before it is as small as it goes.
pub fn home_delta(layout: &[f64], panels: &[PanelConstraints], handle: usize) -> f64 {
    lowest(&panels[handle]) - layout[handle]
}

/// How far End moves a handle: until the panel before it is as large as it goes.
pub fn end_delta(layout: &[f64], panels: &[PanelConstraints], handle: usize) -> f64 {
    panels[handle].max - layout[handle]
}

/// The least and greatest size the panel before a handle can reach by moving it,
/// for the handle's value range.
pub fn handle_bounds(
    layout: &[f64],
    panels: &[PanelConstraints],
    handle: usize,
    locked: &[bool],
) -> (f64, f64) {
    let reach = |delta: f64| {
        resize_at(layout, panels, handle, delta, 1.0, locked)
            .map_or(layout[handle], |next| next[handle])
    };
    (
        reach(home_delta(layout, panels, handle)),
        reach(end_delta(layout, panels, handle)),
    )
}

/// Every handle's bounds. Where no panel can collapse and all are within their
/// limits this is worked out in one pass.
pub fn all_handle_bounds(
    layout: &[f64],
    panels: &[PanelConstraints],
    locked: &[bool],
) -> Vec<(f64, f64)> {
    let handles = 0..layout.len().saturating_sub(1);
    let simple = panels
        .iter()
        .zip(layout)
        .all(|(panel, size)| !panel.collapsible && allowed(panel, *size));
    if !simple {
        return handles
            .map(|handle| handle_bounds(layout, panels, handle, locked))
            .collect();
    }
    let is_locked = |handle: usize| locked.get(handle).copied().unwrap_or(false);
    let mut spare = vec![0.0; layout.len() + 1];
    for index in (0..layout.len()).rev() {
        spare[index] = (layout[index] - panels[index].min).max(0.0)
            + if is_locked(index) {
                0.0
            } else {
                spare[index + 1]
            };
    }
    handles
        .map(|handle| {
            let size = layout[handle];
            let shrink =
                (size - panels[handle].min).min(panels[handle + 1].max - layout[handle + 1]);
            let grow = (panels[handle].max - size).min(spare[handle + 1]);
            (size - shrink.max(0.0), size + grow.max(0.0))
        })
        .collect()
}

/// A layout brought to a total of a hundred with every panel inside its limits.
/// What is left over goes to the panels that can take it. Collapsed panels stay
/// collapsed.
pub fn fit_layout(layout: &[f64], panels: &[PanelConstraints]) -> Vec<f64> {
    let total: f64 = layout.iter().sum();
    let scale = if total > EPSILON && !same(total, 100.0) {
        100.0 / total
    } else {
        1.0
    };
    let mut next: Vec<f64> = layout
        .iter()
        .zip(panels)
        .map(|(size, panel)| settle(panel, size * scale, size * scale, 0.5))
        .collect();
    let mut left = 100.0 - next.iter().sum::<f64>();
    for (size, panel) in next.iter_mut().zip(panels) {
        if left.abs() <= EPSILON {
            break;
        }
        if is_collapsed(panel, *size) {
            continue;
        }
        let change = if left > 0.0 {
            left.min(panel.max - *size)
        } else {
            left.max(panel.min - *size)
        };
        *size += change;
        left -= change;
    }
    if left.abs() > EPSILON
        && let Some(last) = next.last_mut()
    {
        *last += left;
    }
    next
}

/// The layout panels start at: each at its own default, and those without one
/// sharing what is left evenly.
pub fn initial_layout(panels: &[PanelConstraints]) -> Vec<f64> {
    let given: Vec<f64> = panels.iter().filter_map(|panel| panel.initial).collect();
    let left = (100.0 - given.iter().sum::<f64>()).max(0.0);
    let share = if panels.len() > given.len() {
        left / (panels.len() - given.len()) as f64
    } else {
        0.0
    };
    let wanted: Vec<f64> = panels
        .iter()
        .map(|panel| panel.initial.unwrap_or(share))
        .collect();
    fit_layout(&wanted, panels)
}

/// The layout after panels come or go. Those that stay keep their sizes relative
/// to each other, and a new one takes its default.
pub fn carry_layout(
    previous: &[(&str, f64)],
    ids: &[&str],
    panels: &[PanelConstraints],
) -> Vec<f64> {
    let kept = |id: &str| {
        previous
            .iter()
            .find(|(known, _)| *known == id)
            .map(|(_, size)| *size)
    };
    if !ids.iter().any(|id| kept(id).is_some()) {
        return initial_layout(panels);
    }
    let fresh: Vec<Option<f64>> = ids
        .iter()
        .zip(panels)
        .map(|(id, panel)| {
            kept(id)
                .is_none()
                .then(|| panel.initial.unwrap_or(100.0 / ids.len() as f64))
        })
        .collect();
    let kept_total: f64 = ids.iter().filter_map(|id| kept(id)).sum();
    let room = (100.0 - fresh.iter().flatten().sum::<f64>()).max(0.0);
    let wanted: Vec<f64> = ids
        .iter()
        .zip(&fresh)
        .map(|(id, fresh)| {
            fresh.unwrap_or_else(|| {
                kept(id).unwrap_or(0.0) / if kept_total == 0.0 { 1.0 } else { kept_total } * room
            })
        })
        .collect();
    fit_layout(&wanted, panels)
}

/// The layout after setting one panel to `target`, through the handle after it,
/// or the one before it where that gets closer. `None` when it cannot change.
pub fn resize_panel(
    layout: &[f64],
    panels: &[PanelConstraints],
    index: usize,
    target: f64,
    threshold: f64,
    locked: &[bool],
) -> Option<Vec<f64>> {
    let is_locked = |handle: usize| locked.get(handle).copied().unwrap_or(false);
    let delta = target - layout[index];
    let after = (index + 1 < layout.len() && !is_locked(index))
        .then(|| resize_at(layout, panels, index, delta, threshold, locked))
        .flatten();
    let before = (index > 0 && !is_locked(index - 1))
        .then(|| resize_at(layout, panels, index - 1, -delta, threshold, locked))
        .flatten();
    let mut best: Option<Vec<f64>> = None;
    for candidate in [after, before].into_iter().flatten() {
        let closest = best.as_ref().map_or(layout[index], |best| best[index]);
        if (candidate[index] - target).abs() < (closest - target).abs() - EPSILON {
            best = Some(candidate);
        }
    }
    best
}

/// Which panel Enter on a handle collapses or opens: the one before it, or the
/// one after when only that one can collapse.
pub fn toggle_target(panels: &[PanelConstraints], handle: usize) -> Option<usize> {
    [handle, handle + 1]
        .into_iter()
        .find(|index| panels.get(*index).is_some_and(|panel| panel.collapsible))
}

/// The key a group's layouts are stored under, for an id the owner gives it.
pub fn storage_key(save_id: &str) -> String {
    format!("armature-panels:{save_id}")
}

/// Reads stored layouts: a JSON object from each set of panel ids, joined by
/// commas, to its sizes. Anything else reads as nothing stored.
fn stored_entries(stored: &str) -> Vec<(String, Vec<f64>)> {
    let mut entries = Vec::new();
    let mut rest = stored
        .trim()
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
        .unwrap_or_default()
        .trim();
    while let Some(after) = rest.strip_prefix('"') {
        // The key, with `\"` and `\\` unescaped.
        let mut key = String::new();
        let mut chars = after.char_indices();
        let end = loop {
            match chars.next() {
                Some((_, '\\')) => key.extend(chars.next().map(|(_, ch)| ch)),
                Some((at, '"')) => break at,
                Some((_, ch)) => key.push(ch),
                None => return Vec::new(),
            }
        };
        let Some(list) = after[end + 1..]
            .trim_start()
            .strip_prefix(':')
            .map(str::trim_start)
            .and_then(|list| list.strip_prefix('['))
        else {
            return Vec::new();
        };
        let Some(close) = list.find(']') else {
            return Vec::new();
        };
        let sizes: Option<Vec<f64>> = list[..close]
            .split(',')
            .filter(|size| !size.trim().is_empty())
            .map(|size| {
                size.trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|size| size.is_finite())
            })
            .collect();
        let Some(sizes) = sizes else {
            return Vec::new();
        };
        entries.push((key, sizes));
        rest = list[close + 1..]
            .trim_start()
            .strip_prefix(',')
            .unwrap_or(&list[close + 1..])
            .trim_start();
    }
    if rest.is_empty() { entries } else { Vec::new() }
}

/// The layout saved for exactly this set of panels, from what was stored. Each
/// set of panels keeps its own, so adding or removing one does not lose the rest.
pub fn load_layout(stored: &str, ids: &[&str]) -> Option<Vec<f64>> {
    let wanted = ids.join(",");
    stored_entries(stored)
        .into_iter()
        .find(|(key, sizes)| *key == wanted && sizes.len() == ids.len())
        .map(|(_, sizes)| sizes)
}

/// What to store after saving a layout for this set of panels, keeping the
/// layouts already stored for other sets. Sizes are rounded to a thousandth.
pub fn save_layout(stored: &str, ids: &[&str], layout: &[f64]) -> String {
    let wanted = ids.join(",");
    let mut entries: Vec<(String, Vec<f64>)> = stored_entries(stored)
        .into_iter()
        .filter(|(key, _)| *key != wanted)
        .collect();
    entries.push((
        wanted,
        layout
            .iter()
            .map(|size| (size * 1000.0).round() / 1000.0)
            .collect(),
    ));
    let written: Vec<String> = entries
        .iter()
        .map(|(key, sizes)| {
            let key = key.replace('\\', "\\\\").replace('"', "\\\"");
            let sizes: Vec<String> = sizes.iter().map(f64::to_string).collect();
            format!("\"{key}\":[{}]", sizes.join(","))
        })
        .collect();
    format!("{{{}}}", written.join(","))
}
