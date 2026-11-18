use crate::convert::Cast;
use std::collections::{HashMap, HashSet};

use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::drag::{Drag, DragEvent};
use crate::geometry::Rect;
use crate::keys::{Dir, Key};
use crate::primitive::{
    Effect, Focusable, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive, Reaction,
    Role, Semantics,
};
use crate::tree::{
    CheckStrategy, Checked, Row, TREE_INDENT, Tree, TreeItem, drop_depth, place_node, slot_target,
};
use crate::typeahead::Typeahead;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    /// One node. Render every node, in display order; nodes inside a closed branch
    /// are hidden.
    Node,
}

const ACTIONS: &[&str] = &["select", "expand", "collapse"];

/// A tree of nodes that expand, collapse and select. The value is the selected node.
#[derive(Clone, Debug, Default)]
pub struct TreeView {
    data: Vec<TreeItem>,
    dir: Dir,
    expanded: HashSet<String>,
    disabled: HashSet<String>,
    labels: HashMap<String, String>,
    selected: Option<String>,
    /// Nodes whose children are fetched when they are first opened, and those
    /// being fetched now.
    lazy: HashSet<String>,
    loading: HashSet<String>,
    /// Whether several nodes can be selected. A press or Space then adds a node to
    /// the selection or takes it out.
    multiple: bool,
    picked: Vec<String>,
    /// How the checked nodes are reported, for a tree of checkboxes.
    check: Option<CheckStrategy>,
    /// The checked leaves.
    checked: HashSet<String>,
    focused: Option<String>,
    typeahead: Typeahead,
    drag: Drag,
    /// The node under the pointer since it went down.
    pressed: Option<String>,
    /// Where the dragged node would land: the slot among the other rows, and the depth.
    landing: Option<(usize, usize)>,
}

impl TreeView {
    #[must_use]
    pub fn new(data: Vec<TreeItem>, dir: Dir) -> Self {
        Self {
            data,
            dir,
            ..Default::default()
        }
    }

    #[must_use]
    pub fn is_expanded(&self, key: &str) -> bool {
        self.expanded.contains(key)
    }

    /// The rows on screen, in order.
    #[must_use]
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Tree::new(&self.data).visible_rows(&self.expanded);
        // A node still to be loaded can be opened, though it has nothing yet.
        for row in rows.iter_mut().filter(|row| self.lazy.contains(&row.key)) {
            row.has_children = true;
        }
        rows
    }

    fn enabled(&self, key: &str) -> bool {
        !self.disabled.contains(key)
    }

    fn label(&self, key: &str) -> String {
        self.labels
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    fn current(&self, rows: &[Row]) -> Option<String> {
        let visible = |key: &&String| rows.iter().any(|row| row.key == **key);
        self.focused
            .as_ref()
            .filter(visible)
            .or_else(|| self.selected.as_ref().filter(visible))
            .cloned()
            .or_else(|| {
                rows.iter()
                    .find(|row| self.enabled(&row.key))
                    .map(|row| row.key.clone())
            })
    }

    /// Marks nodes whose children are loaded on demand. Opening one offers
    /// `load:<key>` to the gate, and the owner answers with
    /// `set("children", "<key>=child,child")` when they arrive.
    #[must_use]
    pub fn lazy(mut self, keys: &[&str]) -> Self {
        self.lazy = keys.iter().map(std::string::ToString::to_string).collect();
        self
    }

    #[must_use]
    pub fn is_loading(&self, key: &str) -> bool {
        self.loading.contains(key)
    }

    /// Opens a node. One whose children have yet to be loaded asks for them first.
    fn open_node(&mut self, key: &str, gate: Gate) {
        if !self.lazy.contains(key) {
            self.expanded.insert(key.to_string());
        } else if !self.loading.contains(key) {
            let asked = gate(&ValueChange {
                value: vec![format!("load:{key}")],
                source: Source::User,
                reason: Reason::Action,
            });
            if asked {
                self.loading.insert(key.to_string());
            }
        }
    }

    /// Lets several nodes be selected at once.
    #[must_use]
    pub const fn multiple(mut self) -> Self {
        self.multiple = true;
        self
    }

    fn is_selected(&self, key: &str) -> bool {
        if self.multiple {
            self.picked.iter().any(|picked| picked == key)
        } else {
            self.selected.as_deref() == Some(key)
        }
    }

    /// Turns the tree into one of checkboxes. Choosing a node checks or unchecks it
    /// and everything beneath it, and the value becomes the checked nodes as
    /// `strategy` reports them.
    #[must_use]
    pub const fn checkable(mut self, strategy: CheckStrategy) -> Self {
        self.check = Some(strategy);
        self
    }

    /// Whether a node is checked, unchecked, or checked only in part.
    #[must_use]
    pub fn checked_state(&self, key: &str) -> Checked {
        let states = Tree::new(&self.data).checked_states(&self.checked);
        states.get(key).copied().unwrap_or(Checked::Unchecked)
    }

    fn select(&mut self, key: &str, source: Source, reason: Reason, gate: Gate) -> bool {
        if let Some(strategy) = self.check {
            let tree = Tree::new(&self.data);
            let next = tree.toggle_checked(key, &self.checked);
            let value = tree.checked_value(&next, strategy);
            let allowed = gate(&ValueChange {
                value,
                source,
                reason,
            });
            if allowed {
                self.checked = next;
            }
            return allowed;
        }
        if self.multiple {
            let mut next = self.picked.clone();
            match next.iter().position(|picked| picked == key) {
                Some(at) => drop(next.remove(at)),
                None => next.push(key.to_string()),
            }
            let order: Vec<String> = Tree::new(&self.data).subtree_order();
            next.sort_by_key(|key| order.iter().position(|node| node == key));
            let allowed = gate(&ValueChange {
                value: next.clone(),
                source,
                reason,
            });
            if allowed {
                self.picked = next;
            }
            return allowed;
        }
        if self.selected.as_deref() == Some(key) {
            return true;
        }
        let allowed = gate(&ValueChange {
            value: vec![key.to_string()],
            source,
            reason,
        });
        if allowed {
            self.selected = Some(key.to_string());
        }
        allowed
    }

    /// Where a node being dragged would land, as the slot among the other rows and
    /// the depth, for drawing an indicator.
    #[must_use]
    pub const fn landing(&self) -> Option<(usize, usize)> {
        self.landing
    }

    /// The rows on screen without `key` or anything beneath it.
    fn others(&self, key: &str) -> Vec<Row> {
        let tree = Tree::new(&self.data);
        let moving: Vec<String> = tree.subtree(key).into_iter().map(str::to_string).collect();
        self.rows()
            .into_iter()
            .filter(|row| !moving.contains(&row.key))
            .collect()
    }

    /// The slot and depth a node currently occupies among the other rows.
    fn place_of(&self, key: &str) -> Option<(usize, usize)> {
        let rows = self.rows();
        let (at, row) = rows.iter().enumerate().find(|(_, row)| row.key == key)?;
        Some((at, row.level))
    }

    /// The depth allowed at `slot`, as near to `wanted` as the rows around it permit.
    fn depth_at(&self, others: &[Row], slot: usize, start: usize, sideways: f64) -> usize {
        let above = slot
            .checked_sub(1)
            .and_then(|at| others.get(at))
            .map(|row| row.level);
        let below = others.get(slot).map(|row| row.level);
        drop_depth(start, sideways, TREE_INDENT, above, below)
    }

    /// Moves a node to a slot and depth, if that changes anything and the owner
    /// allows it. The change is offered as `key>parent#index`.
    fn relocate(
        &mut self,
        key: &str,
        slot: usize,
        depth: usize,
        source: Source,
        gate: Gate,
    ) -> bool {
        let others = self.others(key);
        let (parent, index) = slot_target(&others, slot, depth, &Tree::new(&self.data));
        let before = self.data.clone();
        if !place_node(&mut self.data, key, parent.as_deref(), index) || self.data == before {
            self.data = before;
            return false;
        }
        let value = vec![format!(
            "{key}>{}#{index}",
            parent.clone().unwrap_or_default()
        )];
        if !gate(&ValueChange {
            value,
            source,
            reason: Reason::Edit,
        }) {
            self.data = before;
            return false;
        }
        // The new parent opens so the moved node stays in view.
        self.expanded.extend(parent);
        true
    }

    /// Alt with an arrow moves the focused node: up and down among the rows, left and
    /// right between depths.
    fn move_by_key(&mut self, key: Key, gate: Gate) -> Reaction<Part> {
        let rows = self.rows();
        let Some(current) = self.current(&rows) else {
            return Reaction::default();
        };
        let Some((slot, depth)) = self.place_of(&current) else {
            return Reaction::default();
        };
        let others = self.others(&current);
        let (deeper, shallower) = if self.dir == Dir::Rtl {
            (Key::ArrowLeft, Key::ArrowRight)
        } else {
            (Key::ArrowRight, Key::ArrowLeft)
        };
        let (slot, sideways) = match key {
            Key::ArrowUp => (slot.saturating_sub(1), 0.0),
            Key::ArrowDown => ((slot.saturating_add(1)).min(others.len()), 0.0),
            k if k == deeper => (slot, TREE_INDENT),
            k if k == shallower => (slot, -TREE_INDENT),
            _ => return Reaction::default(),
        };
        let depth = self.depth_at(&others, slot, depth, sideways);
        if self.relocate(&current, slot, depth, Source::User, gate) {
            let label = self.label(&current);
            self.focused = Some(current.clone());
            return Reaction::item(Part::Node, true, Some(current))
                .alerting(format!("{label} moved to level {depth}."));
        }
        Reaction::handled()
    }

    fn move_to(&mut self, target: Option<String>) -> Reaction<Part> {
        if let Some(key) = &target {
            self.focused = Some(key.clone());
        }
        Reaction::item(Part::Node, true, target)
    }

    fn on_key(&mut self, key: Key, now_ms: u64, ctrl: bool, gate: Gate) -> Reaction<Part> {
        let rows = self.rows();
        let Some(current) = self.current(&rows) else {
            return Reaction::default();
        };
        let at = rows.iter().position(|row| row.key == current).unwrap_or(0);
        let enabled_from = |rows: &mut dyn Iterator<Item = &Row>| {
            let mut found = None;
            for row in rows {
                if self.enabled(&row.key) {
                    found = Some(row.key.clone());
                    break;
                }
            }
            found
        };
        let (open_key, close_key) = if self.dir == Dir::Rtl {
            (Key::ArrowLeft, Key::ArrowRight)
        } else {
            (Key::ArrowRight, Key::ArrowLeft)
        };
        let Some(row) = rows.get(at) else {
            return Reaction::default();
        };
        let below = rows.get(at.saturating_add(1)..).unwrap_or_default();
        match key {
            Key::ArrowDown => {
                let next = enabled_from(&mut below.iter());
                self.move_to(next)
            }
            Key::ArrowUp => {
                let previous = enabled_from(&mut rows.iter().take(at).rev());
                self.move_to(previous)
            }
            Key::Home => {
                let first = enabled_from(&mut rows.iter());
                self.move_to(first)
            }
            Key::End => {
                let last = enabled_from(&mut rows.iter().rev());
                self.move_to(last)
            }
            // On a closed branch this opens it; on an open one it steps into it.
            k if k == open_key && row.has_children => {
                if row.expanded {
                    let child =
                        enabled_from(&mut below.iter().take_while(|under| under.level > row.level));
                    self.move_to(child)
                } else {
                    self.open_node(&current, gate);
                    Reaction::handled()
                }
            }
            // On an open branch this closes it; otherwise it steps out to the parent.
            k if k == close_key => {
                if row.expanded {
                    self.expanded.remove(&current);
                    Reaction::handled()
                } else {
                    let parent = Tree::new(&self.data).parent(&current).map(str::to_string);
                    self.move_to(parent)
                }
            }
            Key::Enter | Key::Space => {
                self.select(&current, Source::User, Reason::Keyboard, gate);
                Reaction::handled()
            }
            Key::Char(ch) if !ctrl => {
                let labels: Vec<String> = rows.iter().map(|row| self.label(&row.key)).collect();
                let candidates: Vec<(&str, bool)> = rows
                    .iter()
                    .zip(&labels)
                    .map(|(row, label)| (label.as_str(), self.enabled(&row.key)))
                    .collect();
                let target = self
                    .typeahead
                    .push(ch, now_ms, &candidates, Some(at))
                    .and_then(|index| rows.get(index))
                    .map(|row| row.key.clone());
                let found = target.is_some();
                Reaction {
                    handled: found,
                    ..self.move_to(target)
                }
            }
            _ => Reaction::default(),
        }
    }
}

impl PartOf for Part {
    type Primitive = TreeView;
}

impl Primitive for TreeView {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Node]
    }

    fn is_item(part: Part) -> bool {
        part == Part::Node
    }

    fn tracks_pointer(part: Part) -> bool {
        part == Part::Node
    }

    fn set(&mut self, name: &str, value: &str) -> bool {
        let Some((parent, children)) = value.split_once('=').filter(|_| name == "children") else {
            return false;
        };
        for (index, child) in children
            .split(',')
            .filter(|child| !child.is_empty())
            .enumerate()
        {
            let item = TreeItem {
                key: child.to_string(),
                label: child.rsplit('/').next().unwrap_or(child).to_string(),
                children: Vec::new(),
            };
            self.data.push(item);
            place_node(&mut self.data, child, Some(parent), index);
        }
        self.lazy.remove(parent);
        self.loading.remove(parent);
        self.expanded.insert(parent.to_string());
        true
    }

    fn upsert_item(&mut self, item: Item) {
        if item.disabled {
            self.disabled.insert(item.key.clone());
        } else {
            self.disabled.remove(&item.key);
        }
        if !item.label.is_empty() {
            self.labels.insert(item.key, item.label);
        }
    }

    fn remove_item(&mut self, _: &str) {}

    /// The tree's own data decides the order. The rendered text only fills in labels.
    fn set_order(&mut self, rendered: &[(String, String)]) {
        for (key, text) in rendered {
            self.labels
                .entry(key.clone())
                .or_insert_with(|| text.trim().to_string());
        }
    }

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        let Some(key) = item.filter(|_| part == Part::Node) else {
            return Semantics {
                role: Some(Role::Tree),
                ..Default::default()
            };
        };
        let rows = self.rows();
        let Some(row) = rows.iter().find(|row| row.key == key) else {
            return Semantics {
                role: Some(Role::TreeItem),
                hidden: true,
                ..Default::default()
            };
        };
        let current = self.current(&rows);
        Semantics {
            role: Some(Role::TreeItem),
            level: Some(row.level.cast::<u32>()),
            expanded: row.has_children.then_some(row.expanded),
            selected: self.check.is_none().then(|| self.is_selected(key)),
            checked: self
                .check
                .map(|_| self.checked_state(key) == Checked::Checked),
            disabled: !self.enabled(key),
            focusable: if current.as_deref() == Some(key) {
                Focusable::TabStop
            } else {
                Focusable::Programmatic
            },
            active: self.focused.as_deref() == Some(key),
            ..Default::default()
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        // Ctrl with an arrow is the platform's. Alt with an arrow moves the node.
        if matches!(&input, Input::Key { mods, .. } if mods.ctrl && !mods.alt)
            && crate::roving::is_platform_arrow(&input)
        {
            return Reaction::default();
        }
        match input {
            Input::Key { key, mods, .. } if mods.alt => self.move_by_key(key, gate),
            Input::Key { key, mods, now_ms } => self.on_key(key, now_ms, mods.ctrl, gate),
            Input::Focus {
                item: Some(key), ..
            } => {
                self.focused = Some(key);
                Reaction::default()
            }
            // A press selects a node and flips a branch open or closed.
            Input::Press {
                item: Some(key), ..
            } if self.enabled(&key) => {
                if self.lazy.contains(&key) {
                    self.open_node(&key, gate);
                } else if !Tree::new(&self.data).children(&key).is_empty()
                    && !self.expanded.remove(&key)
                {
                    self.expanded.insert(key.clone());
                }
                self.select(&key, Source::User, Reason::Press, gate);
                self.move_to(Some(key))
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
                self.pressed = pointer.item.filter(|key| self.enabled(key));
                Reaction::handled().with(Effect::CapturePointer)
            }
            PointerPhase::Move => {
                let (Some(event), Some(key)) = (self.drag.moved(pointer.at), self.pressed.clone())
                else {
                    return Reaction::default();
                };
                let (DragEvent::Start { total, .. } | DragEvent::Move { total, .. }) = event else {
                    return Reaction::default();
                };
                // The slot comes from the other rows' middles, the depth from how far
                // sideways the row has been carried.
                let others = self.others(&key);
                let boxes: Vec<Rect> = others
                    .iter()
                    .map(|row| measure(Part::Node, Some(&row.key)).unwrap_or_default())
                    .collect();
                let before = boxes
                    .iter()
                    .filter(|rect| rect.center().y < pointer.at.y)
                    .count();
                let start = self.place_of(&key).map_or(1, |(_, depth)| depth);
                let sideways = if self.dir == Dir::Rtl {
                    -total.x
                } else {
                    total.x
                };
                self.landing = Some((before, self.depth_at(&others, before, start, sideways)));
                Reaction::handled()
            }
            PointerPhase::Up => {
                let landing = self.landing.take();
                let pressed = self.pressed.take();
                match (self.drag.up(pointer.at), pressed, landing) {
                    (Some(DragEvent::End { .. }), Some(key), Some((slot, depth))) => {
                        self.relocate(&key, slot, depth, Source::User, gate);
                        self.move_to(Some(key))
                    }
                    // A press that never became a drag is an ordinary press.
                    (Some(DragEvent::Click), Some(key), _) => self.send(
                        Input::Press {
                            part: Part::Node,
                            item: Some(key),
                            mods: pointer.mods,
                        },
                        gate,
                    ),
                    _ => Reaction::handled(),
                }
            }
            PointerPhase::Cancel => {
                self.drag.cancel();
                self.pressed = None;
                self.landing = None;
                Reaction::handled()
            }
        }
    }

    fn value(&self) -> Vec<String> {
        match self.check {
            Some(strategy) => Tree::new(&self.data).checked_value(&self.checked, strategy),
            None if self.multiple => self.picked.clone(),
            None => self.selected.iter().cloned().collect(),
        }
    }

    fn set_value(&mut self, value: Vec<String>) {
        let tree = Tree::new(&self.data);
        if self.check.is_some() {
            // Any node named stands for every leaf beneath it.
            let leaf = |key: &&str| tree.children(key).is_empty();
            self.checked = value
                .iter()
                .flat_map(|key| tree.subtree(key))
                .filter(leaf)
                .map(str::to_string)
                .collect();
            return;
        }
        if self.multiple {
            let order = tree.subtree_order();
            self.picked = order
                .into_iter()
                .filter(|node| value.contains(node))
                .collect();
            return;
        }
        self.selected = value.into_iter().find(|key| tree.level(key).is_some());
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let value = self.value();
        let items = self
            .rows()
            .into_iter()
            .map(|row| SnapshotItem {
                label: self.label(&row.key),
                disabled: !self.enabled(&row.key),
                selected: value.contains(&row.key),
                key: row.key,
            })
            .collect();
        Snapshot {
            name: "tree",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let action: &'static str = match action {
            "select" => "select",
            "expand" => "expand",
            "collapse" => "collapse",
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        let key = item.ok_or(ActError::MissingItem { action })?;
        if Tree::new(&self.data).level(key).is_none() {
            return Err(ActError::UnknownItem(key.to_string()));
        }
        if !self.enabled(key) {
            return Err(ActError::Disabled(key.to_string()));
        }
        match action {
            "expand" => self.open_node(key, gate),
            "collapse" => {
                self.expanded.remove(key);
            }
            _ if !self.select(key, Source::Agent, Reason::Action, gate) => {
                return Err(ActError::Refused);
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Modifiers;
    use crate::tree::tree_from_paths;

    fn tree() -> TreeView {
        let data = tree_from_paths(&[
            "src/main.rs",
            "src/ui/button.rs",
            "docs/intro.md",
            "README.md",
        ]);
        TreeView::new(data, Dir::Ltr)
    }

    fn key(tree: &mut TreeView, key: Key) -> Reaction<Part> {
        tree.send(
            Input::Key {
                key,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        )
    }

    fn visible(tree: &TreeView) -> Vec<String> {
        tree.rows().into_iter().map(|row| row.key).collect()
    }

    #[test]
    fn right_opens_a_branch_then_steps_into_it_and_left_reverses_that() {
        let mut tree = tree();
        key(&mut tree, Key::ArrowRight);
        assert_eq!(
            visible(&tree),
            ["src", "src/main.rs", "src/ui", "docs", "README.md"]
        );
        assert_eq!(
            key(&mut tree, Key::ArrowRight).focus_item(),
            Some("src/main.rs")
        );
        assert_eq!(key(&mut tree, Key::ArrowLeft).focus_item(), Some("src"));
        key(&mut tree, Key::ArrowLeft);
        assert_eq!(visible(&tree), ["src", "docs", "README.md"]);
    }

    #[test]
    fn up_and_down_walk_only_the_rows_on_screen() {
        let mut tree = tree();
        assert_eq!(key(&mut tree, Key::ArrowDown).focus_item(), Some("docs"));
        assert_eq!(key(&mut tree, Key::End).focus_item(), Some("README.md"));
        assert_eq!(key(&mut tree, Key::ArrowDown).focus_item(), None);
    }

    #[test]
    fn nodes_inside_a_closed_branch_are_hidden_and_report_their_level_when_shown() {
        let mut tree = tree();
        assert!(tree.semantics(Part::Node, Some("src/main.rs")).hidden);
        tree.act("expand", Some("src"), &mut |_| true).unwrap();
        let node = tree.semantics(Part::Node, Some("src/ui"));
        assert_eq!(
            (node.hidden, node.level, node.expanded),
            (false, Some(2), Some(false))
        );
        assert_eq!(
            tree.semantics(Part::Node, Some("README.md")).expanded,
            None,
            "a leaf has nothing to expand"
        );
    }

    #[test]
    fn a_press_selects_and_toggles_a_branch() {
        let mut tree = tree();
        let press = |tree: &mut TreeView, key: &str| {
            tree.send(
                Input::Press {
                    part: Part::Node,
                    item: Some(key.into()),
                    mods: Modifiers::default(),
                },
                &mut |_| true,
            );
        };
        press(&mut tree, "src");
        assert!(tree.is_expanded("src"));
        assert_eq!(tree.value(), ["src"]);
        press(&mut tree, "src");
        assert!(!tree.is_expanded("src"));
    }

    #[test]
    fn exactly_one_visible_node_is_a_tab_stop() {
        let mut tree = tree();
        key(&mut tree, Key::ArrowRight);
        key(&mut tree, Key::ArrowDown);
        let stops: Vec<_> = visible(&tree)
            .into_iter()
            .filter(|key| tree.semantics(Part::Node, Some(key)).focusable == Focusable::TabStop)
            .collect();
        assert_eq!(stops, ["src/main.rs"]);
    }

    #[test]
    fn typing_jumps_to_a_visible_node_by_its_label() {
        let mut tree = tree();
        tree.set_order(&[
            ("docs".into(), "Documentation".into()),
            ("README.md".into(), "Read me".into()),
        ]);
        assert_eq!(
            key(&mut tree, Key::Char('r')).focus_item(),
            Some("README.md")
        );
        assert!(!key(&mut tree, Key::Char('z')).handled);
    }

    #[test]
    fn agents_expand_collapse_and_select_real_nodes_only() {
        let mut tree = tree();
        tree.act("select", Some("docs"), &mut |_| true).unwrap();
        assert_eq!(tree.value(), ["docs"]);
        assert_eq!(
            tree.act("select", Some("nope"), &mut |_| true),
            Err(ActError::UnknownItem("nope".into()))
        );
        assert_eq!(
            tree.act("select", Some("src"), &mut |_| false),
            Err(ActError::Refused)
        );
    }

    fn children(tree: &TreeView, key: &str) -> Vec<String> {
        Tree::new(&tree.data)
            .children(key)
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn alt_with_arrows_moves_the_focused_node_among_rows_and_between_depths() {
        let mut tree = tree();
        let alt = Modifiers {
            alt: true,
            ..Default::default()
        };
        let mut offered = Vec::new();
        let mut key = |tree: &mut TreeView, key| {
            tree.send(
                Input::Key {
                    key,
                    mods: alt,
                    now_ms: 0,
                },
                &mut |change| {
                    offered.extend(change.value.iter().cloned());
                    true
                },
            )
        };
        tree.send(
            Input::Focus {
                part: Part::Node,
                item: Some("README.md".into()),
            },
            &mut |_| true,
        );
        key(&mut tree, Key::ArrowUp);
        assert_eq!(visible(&tree), ["src", "README.md", "docs"]);
        let nested = key(&mut tree, Key::ArrowRight);
        assert_eq!(
            children(&tree, "src"),
            ["src/main.rs", "src/ui", "README.md"]
        );
        assert!(
            tree.is_expanded("src"),
            "the new parent opens to keep the node in view"
        );
        assert_eq!(nested.focus_item(), Some("README.md"));
        assert_eq!(offered, ["README.md>#1", "README.md>src#2"]);
    }

    #[test]
    fn dragging_a_row_down_and_sideways_nests_it_under_the_row_above() {
        let mut tree = tree();
        let rows = |part: Part, item: Option<&str>| {
            let at = ["src", "docs", "README.md"]
                .iter()
                .position(|key| Some(*key) == item)?;
            (part == Part::Node).then(|| Rect::new(0.0, at as f64 * 20.0, 200.0, 20.0))
        };
        let pointer = |tree: &mut TreeView, phase, x: f64, y: f64| {
            let pointer = Pointer {
                phase,
                part: Part::Node,
                item: Some("src".into()),
                at: crate::geometry::Point::new(x, y),
                mods: Modifiers::default(),
                now_ms: 0,
            };
            tree.pointer(pointer, &rows, &mut |_| true)
        };
        pointer(&mut tree, PointerPhase::Down, 10.0, 10.0);
        pointer(&mut tree, PointerPhase::Move, 40.0, 35.0);
        assert_eq!(tree.landing(), Some((1, 2)), "after docs, one level in");
        pointer(&mut tree, PointerPhase::Up, 40.0, 35.0);
        assert_eq!(children(&tree, "docs"), ["docs/intro.md", "src"]);
        assert_eq!(tree.landing(), None);
    }

    #[test]
    fn a_press_that_is_not_dragged_still_selects() {
        let mut tree = tree();
        let press = |phase| Pointer {
            phase,
            part: Part::Node,
            item: Some("docs".into()),
            at: crate::geometry::Point::new(5.0, 5.0),
            mods: Modifiers::default(),
            now_ms: 0,
        };
        tree.pointer(press(PointerPhase::Down), &|_, _| None, &mut |_| true);
        tree.pointer(press(PointerPhase::Up), &|_, _| None, &mut |_| true);
        assert_eq!(tree.value(), ["docs"]);
        assert!(tree.is_expanded("docs"));
    }

    #[test]
    fn a_refused_move_leaves_the_tree_as_it_was() {
        let mut tree = tree();
        let alt = Modifiers {
            alt: true,
            ..Default::default()
        };
        tree.send(
            Input::Focus {
                part: Part::Node,
                item: Some("README.md".into()),
            },
            &mut |_| true,
        );
        tree.send(
            Input::Key {
                key: Key::ArrowUp,
                mods: alt,
                now_ms: 0,
            },
            &mut |_| false,
        );
        assert_eq!(visible(&tree), ["src", "docs", "README.md"]);
    }

    #[test]
    fn a_tree_of_checkboxes_checks_a_branch_at_once_and_reports_by_strategy() {
        let data = tree_from_paths(&["src/main.rs", "src/ui/button.rs", "docs/intro.md"]);
        let mut tree = TreeView::new(data.clone(), Dir::Ltr).checkable(CheckStrategy::Trunk);
        tree.act("select", Some("src"), &mut |_| true).unwrap();
        assert_eq!(
            tree.value(),
            ["src"],
            "the outermost fully checked node stands for the rest"
        );
        assert_eq!(tree.semantics(Part::Node, Some("src")).checked, Some(true));
        assert_eq!(tree.semantics(Part::Node, Some("src")).selected, None);

        tree.act("expand", Some("src"), &mut |_| true).unwrap();
        tree.send(
            Input::Focus {
                part: Part::Node,
                item: Some("src/main.rs".into()),
            },
            &mut |_| true,
        );
        key(&mut tree, Key::Space);
        assert_eq!(tree.value(), ["src/ui"]);
        assert_eq!(tree.checked_state("src"), Checked::Mixed);

        let mut leaves = TreeView::new(data, Dir::Ltr).checkable(CheckStrategy::Leaf);
        leaves.set_value(vec!["src".into()]);
        assert_eq!(leaves.value(), ["src/main.rs", "src/ui/button.rs"]);
    }

    #[test]
    fn a_tree_of_checkboxes_conforms() {
        let make = || {
            TreeView::new(
                tree_from_paths(&["src/main.rs", "docs/intro.md", "README.md"]),
                Dir::Ltr,
            )
            .checkable(CheckStrategy::Leaf)
        };
        crate::testing::assert_conforms(make, &[]);
    }

    #[test]
    fn several_nodes_can_be_selected_and_are_reported_in_tree_order() {
        let mut tree = tree().multiple();
        tree.act("select", Some("README.md"), &mut |_| true)
            .unwrap();
        tree.act("select", Some("src"), &mut |_| true).unwrap();
        assert_eq!(tree.value(), ["src", "README.md"]);
        assert_eq!(tree.semantics(Part::Node, Some("src")).selected, Some(true));
        tree.act("select", Some("src"), &mut |_| true).unwrap();
        assert_eq!(tree.value(), ["README.md"]);
    }

    #[test]
    fn a_node_can_load_its_children_when_it_is_first_opened() {
        let mut tree =
            TreeView::new(tree_from_paths(&["remote", "README.md"]), Dir::Ltr).lazy(&["remote"]);
        assert!(
            tree.rows()[0].has_children,
            "it can be opened, though nothing is loaded"
        );
        let mut asked = Vec::new();
        tree.send(
            Input::Key {
                key: Key::ArrowRight,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |change| {
                asked.extend(change.value.iter().cloned());
                true
            },
        );
        assert_eq!(asked, ["load:remote"]);
        assert!(tree.is_loading("remote"));
        assert_eq!(visible(&tree), ["remote", "README.md"]);
        assert!(tree.set("children", "remote=remote/a.txt,remote/b.txt"));
        assert_eq!(
            visible(&tree),
            ["remote", "remote/a.txt", "remote/b.txt", "README.md"]
        );
        assert!(!tree.is_loading("remote"));
    }
}
