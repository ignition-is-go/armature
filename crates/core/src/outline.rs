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
    let block = &rows[from..subtree_end(rows, from)];
    let below = block.iter().map(|row| row.depth).max().unwrap_or(0) - block[0].depth;
    max_depth.map(|max| max.saturating_sub(below))
}

/// Moves a row and its children before `before`, or to the end, as near `depth`
/// as fits between its new neighbours, in one step. `None` when that changes
/// nothing or cannot be done, such as moving a row into itself.
pub fn move_row(
    rows: &[OutlineRow],
    key: &str,
    before: Option<&str>,
    depth: usize,
    max_depth: Option<usize>,
) -> Option<Landing> {
    let from = rows.iter().position(|row| row.key == key)?;
    let end = subtree_end(rows, from);
    let rest: Vec<&OutlineRow> = rows[..from].iter().chain(&rows[end..]).collect();
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
    pub fn grab(rows: &[OutlineRow], key: &str, max_depth: Option<usize>) -> Option<Self> {
        let at = rows.iter().position(|row| row.key == key)?;
        let keys = rows[at..subtree_end(rows, at)]
            .iter()
            .map(|row| row.key.clone())
            .collect();
        Some(OutlineMove {
            key: key.to_string(),
            keys,
            rows: rows.to_vec(),
            aim: rows[at].depth,
            max_depth,
        })
    }

    /// The rows as they stand with the move so far.
    pub fn rows(&self) -> &[OutlineRow] {
        &self.rows
    }

    /// The rows being moved: the one picked up, then those under it.
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
        let slot = self.rows[..self.at()].iter().filter(other).count();
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
            slot + 1
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
            depth + 1
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
    pub fn dropped(self, before: &[OutlineRow]) -> Option<Landing> {
        (self.rows != before)
            .then(|| landing(self.rows, &self.key))
            .flatten()
    }
}

/// The slot a dragged row belongs in: after every other row whose middle the
/// pointer has passed.
pub fn slot_at(middles: &[f64], y: f64) -> usize {
    middles.iter().filter(|middle| **middle < y).count()
}

/// The depth a drag aims for: one level for every indent the pointer has moved
/// sideways from where the drag began.
pub fn depth_at(start_depth: usize, moved_x: f64, indent: f64) -> usize {
    let levels = (moved_x / indent).round() as i64;
    (start_depth as i64 + levels).max(0) as usize
}
