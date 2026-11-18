//! Laying out panels that share a space and are resized by the handles between
//! them. Sizes are percentages of the space and always add up to a hundred.

use crate::convert::Cast;

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
#[must_use]
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
        Self {
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
#[must_use]
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

const fn lowest(panel: &PanelConstraints) -> f64 {
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

/// The size a panel takes when asked for `target` from `current`.
///
/// Below its minimum it either stays at the minimum or shuts, depending on whether the
/// target is past `threshold` of the way: half for a drag, all of it for a key.
#[must_use]
pub fn settle(panel: &PanelConstraints, target: f64, current: f64, threshold: f64) -> f64 {
    if target >= panel.min - EPSILON {
        return target.clamp(panel.min, panel.max);
    }
    if !panel.collapsible {
        return panel.min;
    }
    let span = panel.min - panel.collapsed;
    if current <= panel.collapsed + EPSILON {
        return if target > span.mul_add(1.0 - threshold, panel.collapsed) + EPSILON {
            panel.min
        } else {
            panel.collapsed
        };
    }
    if target < span.mul_add(threshold, panel.collapsed) - EPSILON {
        panel.collapsed
    } else {
        panel.min
    }
}

/// The size of the panel at `index`. A panel that is not there has none.
fn size_at(layout: &[f64], index: usize) -> f64 {
    layout.get(index).copied().unwrap_or_default()
}

/// The limits of the panel at `index`. A panel that is not there has the widest.
fn limits_at(panels: &[PanelConstraints], index: usize) -> PanelConstraints {
    panels.get(index).copied().unwrap_or_default()
}

fn set_size(layout: &mut [f64], index: usize, size: f64) {
    if let Some(slot) = layout.get_mut(index) {
        *slot = size;
    }
}

/// Whether a handle is fixed in place. Handles past the end of `locked` are not.
fn is_locked(locked: &[bool], handle: usize) -> bool {
    locked.get(handle).copied().unwrap_or(false)
}

/// The panels a handle can take from, nearest first. It stops at a locked handle.
fn shrinkable(count: usize, handle: usize, forward: bool, locked: &[bool]) -> Vec<usize> {
    if forward {
        (handle.saturating_add(1)..count)
            .take_while(|index| {
                *index == handle.saturating_add(1) || !is_locked(locked, index.saturating_sub(1))
            })
            .collect()
    } else {
        (0..=handle)
            .rev()
            .take_while(|index| *index == handle || !is_locked(locked, *index))
            .collect()
    }
}

/// The layout after moving a handle by `delta` percent.
///
/// The panel before it grows for a positive move and the one after for a negative one,
/// taking from the panels on the other side in turn: one at its minimum passes the rest
/// on. `locked` marks handles that cannot move, which the taking does not cross. `None`
/// when nothing can move.
#[must_use]
pub fn resize_at(
    layout: &[f64],
    panels: &[PanelConstraints],
    handle: usize,
    delta: f64,
    threshold: f64,
    locked: &[bool],
) -> Option<Vec<f64>> {
    if delta.abs() < EPSILON || layout.len() < 2 || handle >= layout.len().saturating_sub(1) {
        return None;
    }
    let forward = delta > 0.0;
    let pivot = if forward {
        handle
    } else {
        handle.saturating_add(1)
    };
    let shrinking = shrinkable(layout.len(), handle, forward, locked);
    let start = size_at(layout, pivot);
    let room = limits_at(panels, pivot).max - start;
    let capacity: f64 = shrinking
        .iter()
        .map(|index| (size_at(layout, *index) - lowest(&limits_at(panels, *index))).max(0.0))
        .sum();
    let asked = delta.abs().min(room).min(capacity);
    let wanted = settle(&limits_at(panels, pivot), start + asked, start, threshold) - start;
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
        let (size, panel) = (size_at(layout, index), &limits_at(panels, index));
        let mut settled = size.min(settle(panel, size - remaining, size, threshold));
        if given + size - settled > room + EPSILON {
            settled = size.min(panel.min.max(size - remaining));
        }
        set_size(&mut next, index, settled);
        given += size - settled;
    }
    if given <= EPSILON || !allowed(&limits_at(panels, pivot), start + given) {
        return None;
    }
    set_size(&mut next, pivot, start + given);
    Some(next)
}

/// How far Home moves a handle: until the panel before it is as small as it goes.
#[must_use]
pub fn home_delta(layout: &[f64], panels: &[PanelConstraints], handle: usize) -> f64 {
    lowest(&limits_at(panels, handle)) - size_at(layout, handle)
}

/// How far End moves a handle: until the panel before it is as large as it goes.
#[must_use]
pub fn end_delta(layout: &[f64], panels: &[PanelConstraints], handle: usize) -> f64 {
    limits_at(panels, handle).max - size_at(layout, handle)
}

/// The least and greatest size the panel before a handle can reach by moving it,
/// for the handle's value range.
#[must_use]
pub fn handle_bounds(
    layout: &[f64],
    panels: &[PanelConstraints],
    handle: usize,
    locked: &[bool],
) -> (f64, f64) {
    let reach = |delta: f64| {
        resize_at(layout, panels, handle, delta, 1.0, locked)
            .map_or_else(|| size_at(layout, handle), |next| size_at(&next, handle))
    };
    (
        reach(home_delta(layout, panels, handle)),
        reach(end_delta(layout, panels, handle)),
    )
}

/// Every handle's bounds. Where no panel can collapse and all are within their
/// limits this is worked out in one pass.
#[must_use]
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
    // How much each panel, and those after it that a resize can reach, can give up.
    let mut spare = vec![0.0; layout.len().saturating_add(1)];
    for index in (0..layout.len()).rev() {
        let own = (size_at(layout, index) - limits_at(panels, index).min).max(0.0);
        let passed_on = if is_locked(locked, index) {
            0.0
        } else {
            size_at(&spare, index.saturating_add(1))
        };
        set_size(&mut spare, index, own + passed_on);
    }
    handles
        .map(|handle| {
            let (before, after) = (handle, handle.saturating_add(1));
            let size = size_at(layout, before);
            let can_shrink = (size - limits_at(panels, before).min)
                .min(limits_at(panels, after).max - size_at(layout, after));
            let can_grow = (limits_at(panels, before).max - size).min(size_at(&spare, after));
            (size - can_shrink.max(0.0), size + can_grow.max(0.0))
        })
        .collect()
}

/// A layout brought to a total of a hundred with every panel inside its limits.
/// What is left over goes to the panels that can take it. Collapsed panels stay
/// collapsed.
#[must_use]
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
#[must_use]
pub fn initial_layout(panels: &[PanelConstraints]) -> Vec<f64> {
    let given: Vec<f64> = panels.iter().filter_map(|panel| panel.initial).collect();
    let left = (100.0 - given.iter().sum::<f64>()).max(0.0);
    let share = if panels.len() > given.len() {
        left / (panels.len().saturating_sub(given.len())).cast::<f64>()
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
#[must_use]
pub fn carry_layout(
    previous: &[(&str, f64)],
    ids: &[&str],
    panels: &[PanelConstraints],
) -> Vec<f64> {
    let kept_size = |id: &str| {
        previous
            .iter()
            .find(|(known, _)| *known == id)
            .map(|(_, size)| *size)
    };
    if !ids.iter().any(|id| kept_size(id).is_some()) {
        return initial_layout(panels);
    }
    // A new panel takes its default, or an even share.
    let even_share = 100.0 / ids.len().cast::<f64>();
    let new_size = |panel: &PanelConstraints| panel.initial.unwrap_or(even_share);
    let new_total: f64 = ids
        .iter()
        .zip(panels)
        .filter(|(id, _)| kept_size(id).is_none())
        .map(|(_, panel)| new_size(panel))
        .sum();
    // The panels that stay share what is left, in the proportions they had.
    let kept_total: f64 = ids.iter().filter_map(|id| kept_size(id)).sum();
    let room = (100.0 - new_total).max(0.0);
    let scale = if kept_total > 0.0 {
        room / kept_total
    } else {
        0.0
    };
    let wanted: Vec<f64> = ids
        .iter()
        .zip(panels)
        .map(|(id, panel)| match kept_size(id) {
            Some(size) => size * scale,
            None => new_size(panel),
        })
        .collect();
    fit_layout(&wanted, panels)
}

/// The layout after setting one panel to `target`, through the handle after it,
/// or the one before it where that gets closer. `None` when it cannot change.
#[must_use]
pub fn resize_panel(
    layout: &[f64],
    panels: &[PanelConstraints],
    index: usize,
    target: f64,
    threshold: f64,
    locked: &[bool],
) -> Option<Vec<f64>> {
    let delta = target - size_at(layout, index);
    let after = (index.saturating_add(1) < layout.len() && !is_locked(locked, index))
        .then(|| resize_at(layout, panels, index, delta, threshold, locked))
        .flatten();
    let before = (index > 0 && !is_locked(locked, index.saturating_sub(1)))
        .then(|| {
            resize_at(
                layout,
                panels,
                index.saturating_sub(1),
                -delta,
                threshold,
                locked,
            )
        })
        .flatten();
    let mut best: Option<Vec<f64>> = None;
    for candidate in [after, before].into_iter().flatten() {
        let closest = best.as_ref().map_or(size_at(layout, index), |best| {
            best.get(index).copied().unwrap_or_default()
        });
        if (candidate.get(index).copied().unwrap_or_default() - target).abs()
            < (closest - target).abs() - EPSILON
        {
            best = Some(candidate);
        }
    }
    best
}

/// Which panel Enter on a handle collapses or opens: the one before it, or the
/// one after when only that one can collapse.
#[must_use]
pub fn toggle_target(panels: &[PanelConstraints], handle: usize) -> Option<usize> {
    [handle, handle.saturating_add(1)]
        .into_iter()
        .find(|index| panels.get(*index).is_some_and(|panel| panel.collapsible))
}

/// The key a group's layouts are stored under, for an id the owner gives it.
#[must_use]
pub fn storage_key(save_id: &str) -> String {
    format!("armature-panels:{save_id}")
}

/// The stored text being read, and how far the reading has got.
struct Reader<'a> {
    rest: &'a str,
}

impl Reader<'_> {
    /// Steps past `expected`, and any space before it. `false` if it is not next.
    fn take(&mut self, expected: char) -> bool {
        match self.rest.trim_start().strip_prefix(expected) {
            Some(rest) => {
                self.rest = rest;
                true
            }
            None => false,
        }
    }

    /// A quoted string, with `\"` and `\\` unescaped.
    fn string(&mut self) -> Option<String> {
        if !self.take('"') {
            return None;
        }
        let mut text = String::new();
        let mut chars = self.rest.char_indices();
        loop {
            match chars.next()? {
                (_, '\\') => text.push(chars.next()?.1),
                (end, '"') => {
                    self.rest = self.rest.get(end.saturating_add(1)..).unwrap_or_default();
                    return Some(text);
                }
                (_, ch) => text.push(ch),
            }
        }
    }

    /// A list of numbers in square brackets.
    fn numbers(&mut self) -> Option<Vec<f64>> {
        if !self.take('[') {
            return None;
        }
        let end = self.rest.find(']')?;
        let (list, rest) = (
            self.rest.get(..end).unwrap_or_default(),
            self.rest.get(end.saturating_add(1)..).unwrap_or_default(),
        );
        self.rest = rest;
        list.split(',')
            .map(str::trim)
            .filter(|number| !number.is_empty())
            .map(|number| number.parse::<f64>().ok().filter(|size| size.is_finite()))
            .collect()
    }

    /// One `"ids": [sizes]` pair.
    fn entry(&mut self) -> Option<(String, Vec<f64>)> {
        let ids = self.string()?;
        if !self.take(':') {
            return None;
        }
        Some((ids, self.numbers()?))
    }
}

/// Reads stored layouts: a JSON object from each set of panel ids, joined by
/// commas, to its sizes. `None` for anything else, which reads as nothing stored.
fn parse_layouts(stored: &str) -> Option<Vec<(String, Vec<f64>)>> {
    let mut reader = Reader { rest: stored };
    if !reader.take('{') {
        return None;
    }
    let mut layouts = Vec::new();
    if reader.take('}') {
        return Some(layouts);
    }
    loop {
        layouts.push(reader.entry()?);
        if reader.take('}') {
            break;
        }
        if !reader.take(',') {
            return None;
        }
    }
    reader.rest.trim().is_empty().then_some(layouts)
}

fn stored_layouts(stored: &str) -> Vec<(String, Vec<f64>)> {
    parse_layouts(stored).unwrap_or_default()
}

/// The layout saved for exactly this set of panels, from what was stored. Each
/// set of panels keeps its own, so adding or removing one does not lose the rest.
#[must_use]
pub fn load_layout(stored: &str, ids: &[&str]) -> Option<Vec<f64>> {
    let wanted = ids.join(",");
    stored_layouts(stored)
        .into_iter()
        .find(|(key, sizes)| *key == wanted && sizes.len() == ids.len())
        .map(|(_, sizes)| sizes)
}

/// What to store after saving a layout for this set of panels, keeping the
/// layouts already stored for other sets. Sizes are rounded to a thousandth.
#[must_use]
pub fn save_layout(stored: &str, ids: &[&str], layout: &[f64]) -> String {
    let wanted = ids.join(",");
    let mut entries: Vec<(String, Vec<f64>)> = stored_layouts(stored)
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
