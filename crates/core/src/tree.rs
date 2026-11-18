//! A tree as data: lookups, the rows visible for a set of open branches, and checked
//! states that roll up from leaves.

use crate::convert::Cast;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeItem {
    pub key: String,
    pub label: String,
    pub children: Vec<Self>,
}

struct Node {
    key: String,
    parent: Option<usize>,
    /// Depth from the root, starting at 1.
    level: usize,
    children: Vec<usize>,
    /// Index just past this node's subtree, in display order.
    end: usize,
}

/// A row of the tree as displayed, with what an accessible tree needs to know.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub key: String,
    pub level: usize,
    /// Position among its siblings, starting at 1.
    pub position: usize,
    pub siblings: usize,
    pub has_children: bool,
    pub expanded: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checked {
    Checked,
    Unchecked,
    /// Some but not all of the leaves beneath are checked.
    Mixed,
}

pub struct Tree {
    nodes: Vec<Node>,
    index: HashMap<String, usize>,
}

impl Tree {
    #[must_use]
    pub fn new(items: &[TreeItem]) -> Self {
        let mut tree = Self {
            nodes: Vec::new(),
            index: HashMap::new(),
        };
        for item in items {
            tree.insert(item, None, 1);
        }
        tree
    }

    fn insert(&mut self, item: &TreeItem, parent: Option<usize>, level: usize) -> usize {
        let at = self.nodes.len();
        self.index.insert(item.key.clone(), at);
        self.nodes.push(Node {
            key: item.key.clone(),
            parent,
            level,
            children: Vec::new(),
            end: 0,
        });
        for child in &item.children {
            let child = self.insert(child, Some(at), level.saturating_add(1));
            self.nodes[at].children.push(child);
        }
        self.nodes[at].end = self.nodes.len();
        at
    }

    #[must_use]
    pub fn parent(&self, key: &str) -> Option<&str> {
        let parent = self.nodes[*self.index.get(key)?].parent?;
        Some(&self.nodes[parent].key)
    }

    #[must_use]
    pub fn children(&self, key: &str) -> Vec<&str> {
        self.index
            .get(key)
            .map(|at| {
                self.nodes[*at]
                    .children
                    .iter()
                    .map(|c| self.nodes[*c].key.as_str())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[must_use]
    pub fn level(&self, key: &str) -> Option<usize> {
        Some(self.nodes[*self.index.get(key)?].level)
    }

    /// Keys of a node and everything beneath it, in display order.
    #[must_use]
    pub fn subtree(&self, key: &str) -> Vec<&str> {
        self.index
            .get(key)
            .map(|at| {
                self.nodes[*at..self.nodes[*at].end]
                    .iter()
                    .map(|n| n.key.as_str())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The rows on screen when the branches in `expanded` are open.
    #[must_use]
    pub fn visible_rows(&self, expanded: &HashSet<String>) -> Vec<Row> {
        let roots = self.roots();
        let mut rows = Vec::new();
        let mut at = 0;
        while at < self.nodes.len() {
            let node = &self.nodes[at];
            let siblings = match node.parent {
                Some(parent) => &self.nodes[parent].children,
                None => &roots,
            };
            let open = expanded.contains(&node.key);
            rows.push(Row {
                key: node.key.clone(),
                level: node.level,
                position: siblings
                    .iter()
                    .position(|s| *s == at)
                    .map_or(1, |p| p.saturating_add(1)),
                siblings: siblings.len(),
                has_children: !node.children.is_empty(),
                expanded: open,
            });
            at = if open { at.saturating_add(1) } else { node.end };
        }
        rows
    }

    /// The state of every node given the checked leaves. A branch is checked when all
    /// its leaves are, and mixed when only some are.
    #[must_use]
    pub fn checked_states(&self, checked: &HashSet<String>) -> HashMap<String, Checked> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(at, node)| {
                let leaves = self.leaves(at);
                let on = leaves
                    .iter()
                    .filter(|leaf| checked.contains(**leaf))
                    .count();
                let state = match on {
                    0 => Checked::Unchecked,
                    all if all == leaves.len() => Checked::Checked,
                    _ => Checked::Mixed,
                };
                (node.key.clone(), state)
            })
            .collect()
    }

    /// The checked leaves after toggling a node. Toggling a branch checks every leaf
    /// beneath it, unless they all already are, in which case it clears them.
    #[must_use]
    pub fn toggle_checked(&self, key: &str, checked: &HashSet<String>) -> HashSet<String> {
        let mut next = checked.clone();
        let Some(at) = self.index.get(key) else {
            return next;
        };
        let leaves = self.leaves(*at);
        if leaves.iter().all(|leaf| checked.contains(*leaf)) {
            leaves.iter().for_each(|leaf| {
                next.remove(*leaf);
            });
        } else {
            next.extend(leaves.iter().map(std::string::ToString::to_string));
        }
        next
    }

    /// Every node's key, in the order the tree lists them with everything open.
    #[must_use]
    pub fn subtree_order(&self) -> Vec<String> {
        fn walk(tree: &Tree, at: usize, keys: &mut Vec<String>) {
            keys.push(tree.nodes[at].key.clone());
            for child in &tree.nodes[at].children {
                walk(tree, *child, keys);
            }
        }
        let mut keys = Vec::new();
        for root in self.roots() {
            walk(self, root, &mut keys);
        }
        keys
    }

    /// The checked nodes as `strategy` reports them, in display order.
    #[must_use]
    pub fn checked_value(&self, checked: &HashSet<String>, strategy: CheckStrategy) -> Vec<String> {
        let states = self.checked_states(checked);
        let full = |at: usize| states.get(&self.nodes[at].key) == Some(&Checked::Checked);
        (0..self.nodes.len())
            .filter(|at| match strategy {
                CheckStrategy::Leaf => self.nodes[*at].children.is_empty() && full(*at),
                CheckStrategy::Branch => full(*at),
                CheckStrategy::Trunk => full(*at) && !self.nodes[*at].parent.is_some_and(full),
            })
            .map(|at| self.nodes[at].key.clone())
            .collect()
    }

    fn roots(&self) -> Vec<usize> {
        (0..self.nodes.len())
            .filter(|at| self.nodes[*at].parent.is_none())
            .collect()
    }

    fn leaves(&self, at: usize) -> Vec<&str> {
        self.nodes[at..self.nodes[at].end]
            .iter()
            .filter(|node| node.children.is_empty())
            .map(|node| node.key.as_str())
            .collect()
    }
}

/// Where a dragged node lands relative to the row it is dropped on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropPosition {
    Before,
    After,
    /// As the last child of the row.
    Inside,
}

/// Which drop the pointer means at `fraction` of the way down a row, from 0 at its top
/// to 1 at its bottom. The middle half nests when the row can take children.
#[must_use]
pub fn drop_position(fraction: f64, can_nest: bool) -> DropPosition {
    match fraction {
        f if can_nest && (0.25..=0.75).contains(&f) => DropPosition::Inside,
        f if f < 0.5 => DropPosition::Before,
        _ => DropPosition::After,
    }
}

/// Moves node `key`, with everything beneath it, next to or into `target`. Returns
/// `false` and changes nothing if either is missing or `target` is inside the moved
/// subtree.
pub fn move_node(
    items: &mut Vec<TreeItem>,
    key: &str,
    target: &str,
    position: DropPosition,
) -> bool {
    fn contains(item: &TreeItem, key: &str) -> bool {
        item.key == key || item.children.iter().any(|child| contains(child, key))
    }
    fn find<'a>(items: &'a [TreeItem], key: &str) -> Option<&'a TreeItem> {
        items.iter().find_map(|item| {
            if item.key == key {
                Some(item)
            } else {
                find(&item.children, key)
            }
        })
    }
    fn take(items: &mut Vec<TreeItem>, key: &str) -> Option<TreeItem> {
        if let Some(at) = items.iter().position(|item| item.key == key) {
            return Some(items.remove(at));
        }
        items
            .iter_mut()
            .find_map(|item| take(&mut item.children, key))
    }
    fn place(
        items: &mut Vec<TreeItem>,
        node: TreeItem,
        target: &str,
        position: DropPosition,
    ) -> Result<(), TreeItem> {
        if let Some(at) = items.iter().position(|item| item.key == target) {
            match position {
                DropPosition::Before => items.insert(at, node),
                DropPosition::After => items.insert(at.saturating_add(1), node),
                DropPosition::Inside => items[at].children.push(node),
            }
            return Ok(());
        }
        let mut node = node;
        for item in items.iter_mut() {
            match place(&mut item.children, node, target, position) {
                Ok(()) => return Ok(()),
                Err(returned) => node = returned,
            }
        }
        Err(node)
    }

    let movable = find(items, key).is_some_and(|node| !contains(node, target))
        && find(items, target).is_some();
    if !movable {
        return false;
    }
    let node = take(items, key).expect("found above");
    place(items, node, target, position).is_ok()
}

/// What to tell a screen reader after a node is moved.
#[must_use]
pub fn tree_reorder_message(label: &str, target: &str, position: DropPosition) -> String {
    match position {
        DropPosition::Before => format!("{label} moved before {target}."),
        DropPosition::After => format!("{label} moved after {target}."),
        DropPosition::Inside => format!("{label} moved into {target}."),
    }
}

/// How far sideways a dragged row moves to change depth by one level.
pub const TREE_INDENT: f64 = 24.0;

/// The depth a dragged row lands at. Dragging sideways changes it one level per
/// `indent`. It can be no deeper than a child of the row above, and no shallower than
/// the row below, which would otherwise be adopted. Depths start at 1.
#[must_use]
pub fn drop_depth(
    start: usize,
    sideways: f64,
    indent: f64,
    above: Option<usize>,
    below: Option<usize>,
) -> usize {
    let wanted = start.cast::<f64>() + (sideways / indent.max(f64::MIN_POSITIVE)).round();
    let deepest = above.map_or(1, |above| above.saturating_add(1));
    let shallowest = below.unwrap_or(1).min(deepest);
    (wanted.max(1.0).cast::<usize>()).clamp(shallowest, deepest)
}

/// Where a row dropped at `slot` among `rows`, at `depth`, belongs: its parent and its
/// place among that parent's children. `rows` are the rows on screen without the row
/// being moved or anything beneath it.
#[must_use]
pub fn slot_target(
    rows: &[Row],
    slot: usize,
    depth: usize,
    tree: &Tree,
) -> (Option<String>, usize) {
    let above = rows.get(..slot.min(rows.len())).unwrap_or_default();
    // The parent is the nearest row above that is one level shallower.
    let parent = above
        .iter()
        .rev()
        .find(|row| row.level < depth)
        .filter(|row| row.level.saturating_add(1) == depth);
    let siblings: Vec<&str> = match parent {
        Some(parent) => tree.children(&parent.key),
        None => tree
            .roots()
            .into_iter()
            .map(|at| tree.nodes[at].key.as_str())
            .collect(),
    };
    // A closed parent shows none of its children, so the row goes after them all.
    let closed = parent.is_some() && !rows.iter().any(|row| siblings.contains(&row.key.as_str()));
    let before = if closed {
        siblings.len()
    } else {
        above
            .iter()
            .filter(|row| siblings.contains(&row.key.as_str()))
            .count()
    };
    (parent.map(|parent| parent.key.clone()), before)
}

/// Moves node `key`, with everything beneath it, to be child number `index` of
/// `parent`, or a root when there is none. Returns `false` and changes nothing if
/// either is missing or `parent` is inside the moved subtree.
pub fn place_node(
    items: &mut Vec<TreeItem>,
    key: &str,
    parent: Option<&str>,
    index: usize,
) -> bool {
    fn contains(item: &TreeItem, key: &str) -> bool {
        item.key == key || item.children.iter().any(|child| contains(child, key))
    }
    fn find<'a>(items: &'a [TreeItem], key: &str) -> Option<&'a TreeItem> {
        items.iter().find_map(|item| {
            if item.key == key {
                Some(item)
            } else {
                find(&item.children, key)
            }
        })
    }
    fn take(items: &mut Vec<TreeItem>, key: &str) -> Option<TreeItem> {
        if let Some(at) = items.iter().position(|item| item.key == key) {
            return Some(items.remove(at));
        }
        items
            .iter_mut()
            .find_map(|item| take(&mut item.children, key))
    }
    fn children_of<'a>(items: &'a mut [TreeItem], key: &str) -> Option<&'a mut Vec<TreeItem>> {
        for item in items.iter_mut() {
            if item.key == key {
                return Some(&mut item.children);
            }
            if let Some(found) = children_of(&mut item.children, key) {
                return Some(found);
            }
        }
        None
    }

    let Some(node) = find(items, key) else {
        return false;
    };
    if parent.is_some_and(|parent| contains(node, parent) || find(items, parent).is_none()) {
        return false;
    }
    let node = take(items, key).expect("found above");
    let siblings = match parent {
        Some(parent) => children_of(items, parent).expect("found above"),
        None => items,
    };
    siblings.insert(index.min(siblings.len()), node);
    true
}

/// Which nodes stand for a checked set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckStrategy {
    /// The checked leaves.
    Leaf,
    /// Every node that is fully checked.
    Branch,
    /// Only the outermost fully checked nodes.
    Trunk,
}

/// Builds a tree from slash-separated paths. Each node's key is its full path and its
/// label is the last segment.
#[must_use]
pub fn tree_from_paths(paths: &[&str]) -> Vec<TreeItem> {
    let mut roots: Vec<TreeItem> = Vec::new();
    for path in paths {
        let mut level = &mut roots;
        let mut key = String::new();
        for segment in path.split('/').filter(|s| !s.is_empty()) {
            if !key.is_empty() {
                key.push('/');
            }
            key.push_str(segment);
            let at = if let Some(at) = level.iter().position(|item| item.key == key) {
                at
            } else {
                level.push(TreeItem {
                    key: key.clone(),
                    label: segment.to_string(),
                    children: Vec::new(),
                });
                level.len().saturating_sub(1)
            };
            level = &mut level[at].children;
        }
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Tree {
        Tree::new(&tree_from_paths(&[
            "src/main.rs",
            "src/ui/button.rs",
            "src/ui/menu.rs",
            "README.md",
        ]))
    }

    fn set(keys: &[&str]) -> HashSet<String> {
        keys.iter().map(|k| k.to_string()).collect()
    }

    #[test]
    fn paths_become_nested_nodes_keyed_by_full_path() {
        let tree = tree();
        assert_eq!(tree.children("src"), ["src/main.rs", "src/ui"]);
        assert_eq!(tree.parent("src/ui/menu.rs"), Some("src/ui"));
        assert_eq!(tree.level("src/ui/menu.rs"), Some(3));
        assert_eq!(tree.parent("README.md"), None);
    }

    #[test]
    fn only_open_branches_show_their_children() {
        let tree = tree();
        let keys = |expanded: &[&str]| -> Vec<String> {
            tree.visible_rows(&set(expanded))
                .into_iter()
                .map(|row| row.key)
                .collect()
        };
        assert_eq!(keys(&[]), ["src", "README.md"]);
        assert_eq!(
            keys(&["src"]),
            ["src", "src/main.rs", "src/ui", "README.md"]
        );
        assert_eq!(
            keys(&["src/ui"]),
            ["src", "README.md"],
            "a closed parent hides an open child"
        );
    }

    #[test]
    fn rows_carry_their_level_and_place_among_siblings() {
        let rows = tree().visible_rows(&set(&["src"]));
        let ui = rows.iter().find(|row| row.key == "src/ui").unwrap();
        assert_eq!((ui.level, ui.position, ui.siblings), (2, 2, 2));
        assert!(ui.has_children && !ui.expanded);
    }

    #[test]
    fn a_branch_is_mixed_until_all_its_leaves_are_checked() {
        let tree = tree();
        let some = tree.checked_states(&set(&["src/ui/menu.rs"]));
        assert_eq!(some["src/ui"], Checked::Mixed);
        assert_eq!(some["src"], Checked::Mixed);
        assert_eq!(some["README.md"], Checked::Unchecked);
        let all = tree.checked_states(&set(&["src/ui/menu.rs", "src/ui/button.rs"]));
        assert_eq!(all["src/ui"], Checked::Checked);
    }

    #[test]
    fn toggling_a_branch_checks_or_clears_every_leaf_beneath_it() {
        let tree = tree();
        let checked = tree.toggle_checked("src", &set(&["src/main.rs"]));
        assert_eq!(
            checked,
            set(&["src/main.rs", "src/ui/button.rs", "src/ui/menu.rs"])
        );
        assert!(tree.toggle_checked("src", &checked).is_empty());
    }

    #[test]
    fn a_subtree_lists_the_node_and_its_descendants_in_order() {
        assert_eq!(
            tree().subtree("src/ui"),
            ["src/ui", "src/ui/button.rs", "src/ui/menu.rs"]
        );
    }

    #[test]
    fn a_node_moves_with_everything_beneath_it() {
        let mut items = tree_from_paths(&["src/main.rs", "src/ui/button.rs", "docs/intro.md"]);
        assert!(move_node(
            &mut items,
            "src/ui",
            "docs",
            DropPosition::Inside
        ));
        let tree = Tree::new(&items);
        assert_eq!(tree.children("docs"), ["docs/intro.md", "src/ui"]);
        assert_eq!(tree.children("src/ui"), ["src/ui/button.rs"]);
        assert_eq!(tree.children("src"), ["src/main.rs"]);
    }

    #[test]
    fn a_node_can_be_placed_before_or_after_a_sibling() {
        let mut items = tree_from_paths(&["a", "b", "c"]);
        move_node(&mut items, "c", "a", DropPosition::Before);
        move_node(&mut items, "a", "b", DropPosition::After);
        let keys: Vec<_> = items.iter().map(|item| item.key.as_str()).collect();
        assert_eq!(keys, ["c", "b", "a"]);
    }

    #[test]
    fn a_node_cannot_be_dropped_into_its_own_subtree() {
        let mut items = tree_from_paths(&["src/ui/button.rs"]);
        let before = items.clone();
        assert!(!move_node(
            &mut items,
            "src",
            "src/ui",
            DropPosition::Inside
        ));
        assert!(!move_node(&mut items, "src", "src", DropPosition::After));
        assert!(!move_node(
            &mut items,
            "src",
            "missing",
            DropPosition::After
        ));
        assert_eq!(items, before);
    }

    #[test]
    fn the_middle_of_a_row_nests_only_where_nesting_is_allowed() {
        assert_eq!(drop_position(0.1, true), DropPosition::Before);
        assert_eq!(drop_position(0.5, true), DropPosition::Inside);
        assert_eq!(drop_position(0.9, true), DropPosition::After);
        assert_eq!(drop_position(0.4, false), DropPosition::Before);
        assert_eq!(drop_position(0.6, false), DropPosition::After);
    }

    #[test]
    fn dragging_sideways_changes_depth_within_what_the_neighbours_allow() {
        assert_eq!(
            drop_depth(1, 30.0, TREE_INDENT, Some(1), None),
            2,
            "one level in, under the row above"
        );
        assert_eq!(
            drop_depth(1, 500.0, TREE_INDENT, Some(1), None),
            2,
            "no deeper than its child"
        );
        assert_eq!(
            drop_depth(3, -500.0, TREE_INDENT, Some(3), Some(2)),
            2,
            "no shallower than the row below"
        );
        assert_eq!(
            drop_depth(2, 0.0, TREE_INDENT, None, Some(1)),
            1,
            "nothing above means the top level"
        );
    }

    #[test]
    fn a_slot_and_depth_resolve_to_a_parent_and_a_place() {
        let items = tree_from_paths(&[
            "src/main.rs",
            "src/ui/button.rs",
            "docs/intro.md",
            "README.md",
        ]);
        let tree = Tree::new(&items);
        let expanded: HashSet<String> = ["src", "docs"].iter().map(|k| k.to_string()).collect();
        let rows: Vec<Row> = tree
            .visible_rows(&expanded)
            .into_iter()
            .filter(|row| row.key != "README.md")
            .collect();
        // Rows: src, src/main.rs, src/ui, docs, docs/intro.md.
        assert_eq!(slot_target(&rows, 5, 2, &tree), (Some("docs".into()), 1));
        assert_eq!(slot_target(&rows, 5, 1, &tree), (None, 2));
        assert_eq!(slot_target(&rows, 1, 2, &tree), (Some("src".into()), 0));
        assert_eq!(slot_target(&rows, 0, 1, &tree), (None, 0));
    }

    #[test]
    fn a_node_is_placed_under_a_parent_at_an_index() {
        let mut items = tree_from_paths(&["src/main.rs", "src/ui/button.rs", "README.md"]);
        assert!(place_node(&mut items, "README.md", Some("src"), 1));
        assert_eq!(
            Tree::new(&items).children("src"),
            ["src/main.rs", "README.md", "src/ui"]
        );
        assert!(place_node(&mut items, "src/ui", None, 0));
        assert_eq!(items[0].key, "src/ui");
        assert!(
            !place_node(&mut items, "src", Some("src/main.rs"), 0),
            "not into its own subtree"
        );
    }

    #[test]
    fn a_checked_set_can_be_reported_by_leaf_branch_or_trunk() {
        let tree = tree();
        let checked = set(&["src/main.rs", "src/ui/button.rs", "src/ui/menu.rs"]);
        assert_eq!(
            tree.checked_value(&checked, CheckStrategy::Leaf),
            ["src/main.rs", "src/ui/button.rs", "src/ui/menu.rs"]
        );
        assert_eq!(tree.checked_value(&checked, CheckStrategy::Trunk), ["src"]);
        assert_eq!(tree.checked_value(&checked, CheckStrategy::Branch).len(), 5);
    }
}
