//! An outline: rows in reading order, each with how deep it sits. A row's children
//! are the deeper rows right after it. Moving a row takes its children along.

/// One row of an outline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineRow {
    pub key: String,
    pub depth: usize,
}

/// Where a row sits: under which row, and which of its children it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    /// The index of the parent row, or `None` at the top level.
    pub parent: Option<usize>,
    pub position: usize,
}

/// The index just past a row and everything under it.
#[must_use]
pub fn subtree_end(rows: &[OutlineRow], index: usize) -> usize {
    let depth = rows[index].depth;
    rows.get(index.saturating_add(1)..)
        .unwrap_or_default()
        .iter()
        .position(|row| row.depth <= depth)
        .map_or(rows.len(), |offset| {
            (index.saturating_add(1)).saturating_add(offset)
        })
}

/// The parent of the row at `index` and its place among that parent's children.
#[must_use]
pub fn place_of(rows: &[OutlineRow], index: usize) -> Place {
    let depth = rows[index].depth;
    let parent = rows
        .get(..index)
        .unwrap_or_default()
        .iter()
        .rposition(|row| row.depth < depth);
    let first = parent.map_or(0, |parent| parent.saturating_add(1));
    let position = rows
        .get(first..index)
        .unwrap_or_default()
        .iter()
        .filter(|row| row.depth == depth)
        .count();
    Place { parent, position }
}

/// How shallow and how deep a row may sit between two neighbours: no deeper than
/// a child of the row above, and no shallower than the row below, which it would
/// otherwise take from its parent. Returns `(min, max)`.
#[must_use]
pub fn depth_range(
    above: Option<&OutlineRow>,
    below: Option<&OutlineRow>,
    max_depth: Option<usize>,
) -> (usize, usize) {
    let max = above
        .map_or(0, |row| row.depth.saturating_add(1))
        .min(max_depth.unwrap_or(usize::MAX));
    let min = below.map_or(0, |row| row.depth);
    (min.min(max), max)
}

/// The outline with a row and its children moved before the row `before`, or to
/// the end, and set to `depth`. `None` when nothing would change or a key is
/// unknown.
#[must_use]
pub fn moved_rows(
    rows: &[OutlineRow],
    key: &str,
    before: Option<&str>,
    depth: usize,
) -> Option<Vec<OutlineRow>> {
    let from = rows.iter().position(|row| row.key == key)?;
    let end = subtree_end(rows, from);
    let was = rows[from].depth;
    let block: Vec<OutlineRow> = if depth >= was {
        rows.get(from..end)
            .unwrap_or_default()
            .iter()
            .map(|row| OutlineRow {
                key: row.key.clone(),
                depth: row.depth.saturating_add(depth.saturating_sub(was)),
            })
            .collect()
    } else {
        rows.get(from..end)
            .unwrap_or_default()
            .iter()
            .map(|row| OutlineRow {
                key: row.key.clone(),
                depth: row.depth.saturating_sub(was.saturating_sub(depth)),
            })
            .collect()
    };
    let mut rest: Vec<OutlineRow> = rows
        .get(..from)
        .unwrap_or_default()
        .iter()
        .chain(rows.get(end..).unwrap_or_default())
        .cloned()
        .collect();
    let at = match before {
        None => rest.len(),
        Some(before) => rest.iter().position(|row| row.key == before)?,
    };
    if depth == was && at == from {
        return None;
    }
    rest.splice(at..at, block);
    Some(rest)
}

/// Where a moved row ended up, for telling the owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Landing {
    pub rows: Vec<OutlineRow>,
    pub key: String,
    /// The row it now sits under, or `None` at the top level.
    pub parent: Option<String>,
    /// Which of that parent's children it is.
    pub index: usize,
}

fn landing(rows: Vec<OutlineRow>, key: &str) -> Option<Landing> {
    let at = rows.iter().position(|row| row.key == key)?;
    let place = place_of(&rows, at);
    let parent = place.parent.map(|parent| rows[parent].key.clone());
    Some(Landing {
        key: key.to_string(),
        parent,
        index: place.position,
        rows,
    })
}

/// The deepest a row can go so that the deepest row under it stays within
/// `max_depth`.
fn block_max_depth(rows: &[OutlineRow], from: usize, max_depth: Option<usize>) -> Option<usize> {
    let block = rows.get(from..subtree_end(rows, from)).unwrap_or_default();
    let below = block
        .iter()
        .map(|row| row.depth)
        .max()
        .unwrap_or(0)
        .saturating_sub(block[0].depth);
    max_depth.map(|max| max.saturating_sub(below))
}

/// Moves a row and its children before `before`, or to the end, as near `depth`
/// as fits between its new neighbours, in one step. `None` when that changes
/// nothing or cannot be done, such as moving a row into itself.
#[must_use]
pub fn move_row(
    rows: &[OutlineRow],
    key: &str,
    before: Option<&str>,
    depth: usize,
    max_depth: Option<usize>,
) -> Option<Landing> {
    let from = rows.iter().position(|row| row.key == key)?;
    let end = subtree_end(rows, from);
    let rest: Vec<&OutlineRow> = rows
        .get(..from)
        .unwrap_or_default()
        .iter()
        .chain(rows.get(end..).unwrap_or_default())
        .collect();
    let at = match before {
        None => rest.len(),
        Some(before) => rest.iter().position(|row| row.key == before)?,
    };
    let above = at.checked_sub(1).map(|above| rest[above]);
    let (min, max) = depth_range(
        above,
        rest.get(at).copied(),
        block_max_depth(rows, from, max_depth),
    );
    landing(moved_rows(rows, key, before, depth.clamp(min, max))?, key)
}

/// A row picked up to be moved with the keyboard or a drag. The rows are shown as
/// they would be if it were dropped now; dropping reports where it landed, and
/// forgetting the move puts everything back.
#[derive(Clone, Debug)]
pub struct OutlineMove {
    key: String,
    /// The row and everything under it, which move together.
    keys: Vec<String>,
    rows: Vec<OutlineRow>,
    /// The depth the person is aiming for, kept while passing through places
    /// where it does not fit.
    aim: usize,
    max_depth: Option<usize>,
}

impl OutlineMove {
    /// Picks up a row. `None` if there is no such row.
    #[must_use]
    pub fn grab(rows: &[OutlineRow], key: &str, max_depth: Option<usize>) -> Option<Self> {
        let at = rows.iter().position(|row| row.key == key)?;
        let keys = rows
            .get(at..subtree_end(rows, at))
            .unwrap_or_default()
            .iter()
            .map(|row| row.key.clone())
            .collect();
        Some(Self {
            key: key.to_string(),
            keys,
            rows: rows.to_vec(),
            aim: rows[at].depth,
            max_depth,
        })
    }

    /// The rows as they stand with the move so far.
    #[must_use]
    pub fn rows(&self) -> &[OutlineRow] {
        &self.rows
    }

    /// The rows being moved: the one picked up, then those under it.
    #[must_use]
    pub fn keys(&self) -> &[String] {
        &self.keys
    }

    fn at(&self) -> usize {
        self.rows
            .iter()
            .position(|row| row.key == self.key)
            .unwrap_or(0)
    }

    /// The other rows on screen, and how many of them come before the moved row.
    fn others(&self, shown: &dyn Fn(&str) -> bool) -> (Vec<&OutlineRow>, usize) {
        let other = |row: &&OutlineRow| !self.keys.contains(&row.key) && shown(&row.key);
        let slot = self
            .rows
            .get(..self.at())
            .unwrap_or_default()
            .iter()
            .filter(other)
            .count();
        (self.rows.iter().filter(other).collect(), slot)
    }

    /// Puts the moved row at `slot` among the other rows on screen, as near
    /// `depth` as fits there. `shown` says which rows are on screen: those inside
    /// a collapsed row are not. Returns whether anything moved.
    pub fn place(&mut self, slot: usize, depth: usize, shown: &dyn Fn(&str) -> bool) -> bool {
        let (others, _) = self.others(shown);
        let to = slot.min(others.len());
        let above = to.checked_sub(1).map(|above| others[above]);
        let (min, max) = depth_range(
            above,
            others.get(to).copied(),
            block_max_depth(&self.rows, self.at(), self.max_depth),
        );
        let before = others.get(to).map(|row| row.key.clone());
        match moved_rows(
            &self.rows,
            &self.key,
            before.as_deref(),
            depth.clamp(min, max),
        ) {
            Some(rows) => {
                self.rows = rows;
                true
            }
            None => false,
        }
    }

    /// Moves it up or down by one row on screen, keeping the depth aimed for.
    pub fn step(&mut self, down: bool, shown: &dyn Fn(&str) -> bool) -> bool {
        let (_, slot) = self.others(shown);
        let to = if down {
            slot.saturating_add(1)
        } else {
            slot.saturating_sub(1)
        };
        self.place(to, self.aim, shown)
    }

    /// Moves it to the top or the bottom of the rows on screen.
    pub fn to_end(&mut self, last: bool, shown: &dyn Fn(&str) -> bool) -> bool {
        let to = if last { usize::MAX } else { 0 };
        self.place(to, self.aim, shown)
    }

    /// Takes it in a level or out one, as far as the rows around it allow.
    pub fn indent(&mut self, inward: bool, shown: &dyn Fn(&str) -> bool) -> bool {
        let (_, slot) = self.others(shown);
        let depth = self.rows[self.at()].depth;
        let wanted = if inward {
            depth.saturating_add(1)
        } else {
            depth.saturating_sub(1)
        };
        let moved = self.place(slot, wanted, shown);
        if moved {
            self.aim = self.rows[self.at()].depth;
        }
        moved
    }

    /// Drops it where it is. `None` when it is back where it started.
    #[must_use]
    pub fn dropped(self, before: &[OutlineRow]) -> Option<Landing> {
        (self.rows != before)
            .then(|| landing(self.rows, &self.key))
            .flatten()
    }
}

/// The slot a dragged row belongs in: after every other row whose middle the
/// pointer has passed.
#[must_use]
pub fn slot_at(middles: &[f64], y: f64) -> usize {
    middles.iter().filter(|middle| **middle < y).count()
}

/// The depth a drag aims for: one level for every indent the pointer has moved
/// sideways from where the drag began.
#[must_use]
pub fn depth_at(start_depth: usize, moved_x: f64, indent: f64) -> usize {
    let levels = (moved_x / indent).round().cast::<i64>();
    ((start_depth.cast::<i64>()).saturating_add(levels)).max(0) as usize
}

use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::convert::Cast;
use crate::drag::{Drag, DragEvent};
use crate::keys::{Dir, Key};
use crate::primitive::{
    Effect, Focusable, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive, Reaction,
    Role, Semantics,
};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    /// One row, keyed by its key. Indent it by `depth_of`.
    Row,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// The deepest any row may sit, counting from 0.
    pub max_depth: Option<usize>,
    /// How far sideways a drag moves for one level, in the units `Measure` uses.
    pub indent: f64,
    pub dir: Dir,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_depth: None,
            indent: 24.0,
            dir: Dir::Ltr,
        }
    }
}

const ACTIONS: &[&str] = &["move_up", "move_down", "indent", "outdent"];

/// An outline whose rows are reordered and nested by dragging, or by the keyboard:
/// Space picks a row up with everything under it, Up and Down move it, Left and
/// Right take it out a level and in one, Space drops it and Escape puts it back.
/// The value is the rows in order, each written `depth:key`.
#[derive(Clone, Debug, Default)]
pub struct Outline {
    opts: Options,
    rows: Vec<OutlineRow>,
    labels: HashMap<String, String>,
    focused: Option<String>,
    moving: Option<OutlineMove>,
    drag: Drag,
    /// The row under the pointer since it went down, and its depth then.
    pressed: Option<(String, usize)>,
}

fn written(rows: &[OutlineRow]) -> Vec<String> {
    rows.iter()
        .map(|row| format!("{}:{}", row.depth, row.key))
        .collect()
}

impl Outline {
    #[must_use]
    pub fn new(opts: Options, rows: Vec<OutlineRow>) -> Self {
        Self {
            opts,
            rows,
            ..Default::default()
        }
    }

    /// The rows to draw, in order: as they would be if the row being moved were
    /// dropped now.
    #[must_use]
    pub fn rows(&self) -> &[OutlineRow] {
        self.moving
            .as_ref()
            .map_or(&self.rows, |moving| moving.rows())
    }

    #[must_use]
    pub fn depth_of(&self, key: &str) -> Option<usize> {
        self.rows()
            .iter()
            .find(|row| row.key == key)
            .map(|row| row.depth)
    }

    /// The rows being moved: the one picked up, then those under it.
    #[must_use]
    pub fn moving(&self) -> &[String] {
        self.moving.as_ref().map_or(&[], |moving| moving.keys())
    }

    fn label(&self, key: &str) -> String {
        self.labels
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    /// Offers the moved rows to the owner and says where the row landed.
    fn commit(
        &mut self,
        landed: Landing,
        source: Source,
        reason: Reason,
        gate: Gate,
    ) -> Option<String> {
        if !gate(&ValueChange {
            value: written(&landed.rows),
            source,
            reason,
        }) {
            return None;
        }
        self.rows = landed.rows;
        let place = match &landed.parent {
            Some(parent) => format!("under {}", self.label(parent)),
            None => "at the top level".to_string(),
        };
        Some(format!(
            "{} moved {place}, position {}",
            self.label(&landed.key),
            landed.index.saturating_add(1)
        ))
    }

    fn on_key(&mut self, key: Key, gate: Gate) -> Reaction<Part> {
        let all = |_: &str| true;
        let Some(moving) = self.moving.as_mut() else {
            let Some(focused) = self.focused.clone() else {
                return Reaction::default();
            };
            let at = self.rows.iter().position(|row| row.key == focused);
            let to = |index: Option<usize>| match index.and_then(|index| self.rows.get(index)) {
                Some(row) => Reaction::item(Part::Row, true, Some(row.key.clone())),
                None => Reaction::handled(),
            };
            return match key {
                Key::Space | Key::Enter => {
                    self.moving = OutlineMove::grab(&self.rows, &focused, self.opts.max_depth);
                    Reaction::handled().alerting(format!(
                        "{} picked up. Arrows move it, Space drops it, Escape puts it back.",
                        self.label(&focused)
                    ))
                }
                Key::ArrowDown => to(at.map(|at| at.saturating_add(1))),
                Key::ArrowUp => to(at.and_then(|at| at.checked_sub(1))),
                Key::Home => to(Some(0)),
                Key::End => to(self.rows.len().checked_sub(1)),
                _ => Reaction::default(),
            };
        };
        let (inward, outward) = if self.opts.dir == Dir::Rtl {
            (Key::ArrowLeft, Key::ArrowRight)
        } else {
            (Key::ArrowRight, Key::ArrowLeft)
        };
        match key {
            Key::ArrowUp => drop(moving.step(false, &all)),
            Key::ArrowDown => drop(moving.step(true, &all)),
            Key::Home => drop(moving.to_end(false, &all)),
            Key::End => drop(moving.to_end(true, &all)),
            key if key == inward => drop(moving.indent(true, &all)),
            key if key == outward => drop(moving.indent(false, &all)),
            Key::Escape => {
                self.moving = None;
                return Reaction::handled().alerting("Move cancelled");
            }
            // Leaving puts the row back and lets focus move on.
            Key::Tab => {
                self.moving = None;
                return Reaction::default();
            }
            Key::Space | Key::Enter => {
                let landed = self
                    .moving
                    .take()
                    .and_then(|moving| moving.dropped(&self.rows));
                return match landed
                    .and_then(|landed| self.commit(landed, Source::User, Reason::Keyboard, gate))
                {
                    Some(message) => Reaction::handled().alerting(message),
                    None => Reaction::handled(),
                };
            }
            // Every other key is swallowed while a row is up.
            _ => {}
        }
        Reaction::item(Part::Row, true, self.focused.clone())
    }
}

impl PartOf for Part {
    type Primitive = Outline;
}

impl Primitive for Outline {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Row]
    }

    fn is_item(part: Part) -> bool {
        part == Part::Row
    }

    fn tracks_pointer(part: Part) -> bool {
        part == Part::Row
    }

    /// The outline owns its rows. An item only gives a row its label.
    fn upsert_item(&mut self, item: Item) {
        self.labels.insert(item.key, item.label);
    }

    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        let Some(key) = item.filter(|_| part == Part::Row) else {
            return Semantics {
                role: Some(Role::Tree),
                ..Default::default()
            };
        };
        let known = |key: &&str| self.rows.iter().any(|row| row.key == *key);
        let stop = self
            .focused
            .as_deref()
            .filter(known)
            .or_else(|| self.rows.first().map(|row| row.key.as_str()));
        Semantics {
            role: Some(Role::TreeItem),
            level: self
                .depth_of(key)
                .map(|depth| (depth.cast::<u32>()).saturating_add(1)),
            selected: Some(self.moving().iter().any(|moved| moved == key)),
            focusable: if stop == Some(key) {
                Focusable::TabStop
            } else {
                Focusable::Programmatic
            },
            active: self.focused.as_deref() == Some(key),
            ..Default::default()
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Key { key, mods, .. } if !mods.ctrl && !mods.alt => self.on_key(key, gate),
            Input::Focus {
                item: Some(key), ..
            } => {
                self.focused = Some(key);
                Reaction::default()
            }
            _ => Reaction::default(),
        }
    }

    fn pointer(
        &mut self,
        pointer: Pointer<Part>,
        measure: Measure<Part>,
        gate: Gate,
    ) -> Reaction<Part> {
        match pointer.phase {
            PointerPhase::Down => {
                self.drag.down(pointer.at);
                self.pressed = pointer
                    .item
                    .clone()
                    .and_then(|key| Some((key.clone(), self.depth_of(&key)?)));
                self.focused = pointer.item.clone();
                Reaction::item(Part::Row, true, pointer.item).with(Effect::CapturePointer)
            }
            PointerPhase::Move => {
                let (Some(event), Some((key, depth))) =
                    (self.drag.moved(pointer.at), self.pressed.clone())
                else {
                    return Reaction::default();
                };
                let total = match event {
                    DragEvent::Start { total, .. } | DragEvent::Move { total, .. } => total,
                    _ => return Reaction::default(),
                };
                if self.moving.is_none() {
                    self.moving = OutlineMove::grab(&self.rows, &key, self.opts.max_depth);
                }
                let Some(moving) = self.moving.as_mut() else {
                    return Reaction::default();
                };
                // The other rows' middles, where they are drawn now.
                let middles: Vec<f64> = moving
                    .rows()
                    .iter()
                    .filter(|row| !moving.keys().contains(&row.key))
                    .filter_map(|row| measure(Part::Row, Some(&row.key)))
                    .map(|rect| rect.y + rect.height / 2.0)
                    .collect();
                let sideways = if self.opts.dir == Dir::Rtl {
                    -total.x
                } else {
                    total.x
                };
                moving.place(
                    slot_at(&middles, pointer.at.y),
                    depth_at(depth, sideways, self.opts.indent),
                    &|_| true,
                );
                Reaction::handled()
            }
            PointerPhase::Up => {
                self.pressed = None;
                let ended = self.drag.up(pointer.at);
                let landed = self
                    .moving
                    .take()
                    .filter(|_| matches!(ended, Some(DragEvent::End { .. })))
                    .and_then(|moving| moving.dropped(&self.rows));
                match landed
                    .and_then(|landed| self.commit(landed, Source::User, Reason::Drag, gate))
                {
                    Some(message) => Reaction::handled().alerting(message),
                    None => Reaction::handled(),
                }
            }
            PointerPhase::Cancel => {
                self.pressed = None;
                self.moving = None;
                self.drag.cancel();
                Reaction::handled()
            }
        }
    }

    fn value(&self) -> Vec<String> {
        written(&self.rows)
    }

    fn set_value(&mut self, value: Vec<String>) {
        let parsed: Option<Vec<OutlineRow>> = value
            .iter()
            .map(|entry| {
                let (depth, key) = entry.split_once(':')?;
                Some(OutlineRow {
                    key: key.to_string(),
                    depth: depth.parse().ok()?,
                })
            })
            .collect();
        if let Some(rows) = parsed {
            self.rows = rows;
            self.moving = None;
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "outline",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: self
                .rows
                .iter()
                .map(|row| SnapshotItem {
                    key: row.key.clone(),
                    label: self.label(&row.key),
                    disabled: false,
                    selected: false,
                })
                .collect(),
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let unknown = || ActError::UnknownAction {
            action: action.to_string(),
            valid: ACTIONS,
        };
        let action = ACTIONS
            .iter()
            .copied()
            .find(|known| *known == action)
            .ok_or_else(unknown)?;
        let key = item.ok_or(ActError::MissingItem { action })?;
        let mut moving = OutlineMove::grab(&self.rows, key, self.opts.max_depth)
            .ok_or_else(|| ActError::UnknownItem(key.to_string()))?;
        let every_row = |_: &str| true;
        match action {
            "move_up" => moving.step(false, &every_row),
            "move_down" => moving.step(true, &every_row),
            "indent" => moving.indent(true, &every_row),
            _ => moving.indent(false, &every_row),
        };
        // With nowhere to go that way there is nothing to commit, or to refuse.
        let Some(landed) = moving.dropped(&self.rows) else {
            return Ok(());
        };
        match self.commit(landed, Source::Agent, Reason::Action, gate) {
            Some(_) => Ok(()),
            None => Err(ActError::Refused),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Modifiers;

    fn notes() -> Outline {
        let rows = [
            ("inbox", 0),
            ("projects", 0),
            ("site", 1),
            ("copy", 2),
            ("app", 1),
            ("archive", 0),
        ];
        Outline::new(
            Options::default(),
            rows.iter()
                .map(|(key, depth)| OutlineRow {
                    key: key.to_string(),
                    depth: *depth,
                })
                .collect(),
        )
    }

    fn key(outline: &mut Outline, key: Key) -> Reaction<Part> {
        outline.send(
            Input::Key {
                key,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        )
    }

    fn focus(outline: &mut Outline, row: &str) {
        outline.send(
            Input::Focus {
                part: Part::Row,
                item: Some(row.into()),
            },
            &mut |_| true,
        );
    }

    #[test]
    fn space_picks_a_row_up_arrows_move_it_and_space_drops_it_in_one_change() {
        let mut outline = notes();
        focus(&mut outline, "site");
        key(&mut outline, Key::Space);
        assert_eq!(outline.moving(), ["site", "copy"]);
        key(&mut outline, Key::ArrowDown);
        assert_eq!(
            outline.value()[2],
            "1:site",
            "nothing is committed while it is up"
        );
        assert_eq!(outline.depth_of("copy"), Some(2));
        let mut changes = 0;
        let dropped = outline.send(
            Input::Key {
                key: Key::Space,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| {
                changes += 1;
                true
            },
        );
        assert_eq!(changes, 1);
        assert_eq!(
            outline.value(),
            [
                "0:inbox",
                "0:projects",
                "1:app",
                "1:site",
                "2:copy",
                "0:archive"
            ]
        );
        assert!(
            matches!(&dropped.effects[0], Effect::Announce { message, .. } if message == "site moved under projects, position 2")
        );
    }

    #[test]
    fn escape_puts_it_back_and_a_refused_drop_changes_nothing() {
        let mut outline = notes();
        let before = outline.value();
        focus(&mut outline, "app");
        key(&mut outline, Key::Space);
        key(&mut outline, Key::ArrowLeft);
        assert_eq!(outline.depth_of("app"), Some(0));
        key(&mut outline, Key::Escape);
        assert_eq!(
            (outline.value(), outline.moving().len()),
            (before.clone(), 0)
        );
        key(&mut outline, Key::Space);
        key(&mut outline, Key::ArrowLeft);
        outline.send(
            Input::Key {
                key: Key::Space,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| false,
        );
        assert_eq!(outline.value(), before);
    }

    #[test]
    fn an_agent_moves_a_row_through_the_gate() {
        let mut outline = notes();
        let mut sources = Vec::new();
        outline
            .act("indent", Some("archive"), &mut |change| {
                sources.push(change.source);
                true
            })
            .unwrap();
        assert_eq!(outline.depth_of("archive"), Some(1));
        assert_eq!(sources, [Source::Agent]);
        assert_eq!(
            outline.act("outdent", Some("inbox"), &mut |_| true),
            Ok(()),
            "nowhere to go is not an error"
        );
        assert!(
            outline
                .act("move_up", Some("missing"), &mut |_| true)
                .is_err()
        );
    }

    #[test]
    fn a_drag_moves_by_the_rows_middles_and_a_level_for_each_indent() {
        let mut outline = notes();
        let at = |x: f64, y: f64, phase| Pointer {
            phase,
            part: Part::Row,
            item: Some("archive".into()),
            at: crate::geometry::Point::new(x, y),
            mods: Modifiers::default(),
            now_ms: 0,
        };
        let rows = ["inbox", "projects", "site", "copy", "app"];
        let measure = |_: Part, key: Option<&str>| {
            let index = rows.iter().position(|row| Some(*row) == key)?;
            Some(crate::geometry::Rect::new(
                0.0,
                index as f64 * 20.0,
                200.0,
                20.0,
            ))
        };
        outline.pointer(at(10.0, 110.0, PointerPhase::Down), &measure, &mut |_| true);
        outline.pointer(at(58.0, 85.0, PointerPhase::Move), &measure, &mut |_| true);
        assert_eq!(outline.depth_of("archive"), Some(2));
        outline.pointer(at(34.0, 85.0, PointerPhase::Move), &measure, &mut |_| true);
        outline.pointer(at(34.0, 85.0, PointerPhase::Up), &measure, &mut |_| true);
        assert_eq!(
            outline.value(),
            [
                "0:inbox",
                "0:projects",
                "1:site",
                "2:copy",
                "1:archive",
                "1:app"
            ]
        );
    }

    #[test]
    fn it_conforms() {
        let items: Vec<Item> = ["inbox", "projects", "site"]
            .iter()
            .map(|key| Item {
                key: key.to_string(),
                label: key.to_string(),
                disabled: false,
            })
            .collect();
        crate::testing::assert_conforms(notes, &items);
    }
}
