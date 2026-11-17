use std::collections::{HashMap, HashSet};

use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::{Dir, Key};
use crate::primitive::{Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics};
use crate::tree::{Row, Tree, TreeItem};
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
    focused: Option<String>,
    typeahead: Typeahead,
}

impl TreeView {
    pub fn new(data: Vec<TreeItem>, dir: Dir) -> Self {
        TreeView {
            data,
            dir,
            ..Default::default()
        }
    }

    pub fn is_expanded(&self, key: &str) -> bool {
        self.expanded.contains(key)
    }

    /// The rows on screen, in order.
    pub fn rows(&self) -> Vec<Row> {
        Tree::new(&self.data).visible_rows(&self.expanded)
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
            .or(self.selected.as_ref().filter(visible))
            .cloned()
            .or_else(|| {
                rows.iter()
                    .find(|row| self.enabled(&row.key))
                    .map(|row| row.key.clone())
            })
    }

    fn select(&mut self, key: &str, source: Source, reason: Reason, gate: Gate) -> bool {
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
        match key {
            Key::ArrowDown => {
                let next = enabled_from(&mut rows[at + 1..].iter());
                self.move_to(next)
            }
            Key::ArrowUp => {
                let previous = enabled_from(&mut rows[..at].iter().rev());
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
            k if k == open_key && rows[at].has_children => {
                if rows[at].expanded {
                    let child = enabled_from(
                        &mut rows[at + 1..]
                            .iter()
                            .take_while(|row| row.level > rows[at].level),
                    );
                    self.move_to(child)
                } else {
                    self.expanded.insert(current);
                    Reaction::handled()
                }
            }
            // On an open branch this closes it; otherwise it steps out to the parent.
            k if k == close_key => {
                if rows[at].expanded {
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
                    .map(|index| rows[index].key.clone());
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
            level: Some(row.level as u32),
            expanded: row.has_children.then_some(row.expanded),
            selected: Some(self.selected.as_deref() == Some(key)),
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
        if crate::roving::is_platform_arrow(&input) {
            return Reaction::default();
        }
        match input {
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
                if !Tree::new(&self.data).children(&key).is_empty() && !self.expanded.remove(&key) {
                    self.expanded.insert(key.clone());
                }
                self.select(&key, Source::User, Reason::Press, gate);
                self.move_to(Some(key))
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        self.selected.iter().cloned().collect()
    }

    fn set_value(&mut self, value: Vec<String>) {
        let tree = Tree::new(&self.data);
        self.selected = value.into_iter().find(|key| tree.level(key).is_some());
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .rows()
            .into_iter()
            .map(|row| SnapshotItem {
                label: self.label(&row.key),
                disabled: !self.enabled(&row.key),
                selected: self.selected.as_deref() == Some(row.key.as_str()),
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
            "expand" => {
                self.expanded.insert(key.to_string());
            }
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
}
