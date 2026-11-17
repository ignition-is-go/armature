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
pub fn subtree_end(rows: &[OutlineRow], index: usize) -> usize {
    let depth = rows[index].depth;
    rows[index + 1..]
        .iter()
        .position(|row| row.depth <= depth)
        .map_or(rows.len(), |offset| index + 1 + offset)
}

/// The parent of the row at `index` and its place among that parent's children.
pub fn place_of(rows: &[OutlineRow], index: usize) -> Place {
    let depth = rows[index].depth;
    let parent = rows[..index].iter().rposition(|row| row.depth < depth);
    let first = parent.map_or(0, |parent| parent + 1);
    let position = rows[first..index]
        .iter()
        .filter(|row| row.depth == depth)
        .count();
    Place { parent, position }
}

/// How shallow and how deep a row may sit between two neighbours: no deeper than
/// a child of the row above, and no shallower than the row below, which it would
/// otherwise take from its parent. Returns `(min, max)`.
pub fn depth_range(
    above: Option<&OutlineRow>,
    below: Option<&OutlineRow>,
    max_depth: Option<usize>,
) -> (usize, usize) {
    let max = above
        .map_or(0, |row| row.depth + 1)
        .min(max_depth.unwrap_or(usize::MAX));
    let min = below.map_or(0, |row| row.depth);
    (min.min(max), max)
}

/// The outline with a row and its children moved before the row `before`, or to
/// the end, and set to `depth`. `None` when nothing would change or a key is
/// unknown.
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
        rows[from..end]
            .iter()
            .map(|row| OutlineRow {
                key: row.key.clone(),
                depth: row.depth + (depth - was),
            })
            .collect()
    } else {
        rows[from..end]
            .iter()
            .map(|row| OutlineRow {
                key: row.key.clone(),
                depth: row.depth - (was - depth),
            })
            .collect()
    };
    let mut rest: Vec<OutlineRow> = rows[..from].iter().chain(&rows[end..]).cloned().collect();
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
