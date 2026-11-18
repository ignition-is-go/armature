use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::grid::{Cell, CellRange, cells_to_text, nav_for_key, step, text_to_cells};
use crate::keys::{Dir, Key};
use crate::primitive::{
    Effect, Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    /// One cell. Its item is `row,column`, counted from 0.
    Cell,
}

const ACTIONS: &[&str] = &["select", "paste", "clear"];

/// A grid of text cells with a rectangular selection that copies and pastes like a
/// spreadsheet.
///
/// The value is the selected cells. Edits to the cells themselves are offered to the
/// owner as `row,column=text` with `Reason::Edit`.
#[derive(Clone, Debug)]
pub struct DataGrid {
    cells: Vec<Vec<String>>,
    dir: Dir,
    range: CellRange,
}

fn key_of(cell: Cell) -> String {
    format!("{},{}", cell.row, cell.col)
}

impl Default for DataGrid {
    fn default() -> Self {
        Self::new(vec![vec![String::new(); 3]; 3], Dir::Ltr)
    }
}

impl DataGrid {
    #[must_use]
    pub const fn new(cells: Vec<Vec<String>>, dir: Dir) -> Self {
        Self {
            cells,
            dir,
            range: CellRange::new(Cell { row: 0, col: 0 }),
        }
    }

    #[must_use]
    pub const fn rows(&self) -> usize {
        self.cells.len()
    }

    pub fn cols(&self) -> usize {
        self.cells.first().map_or(0, Vec::len)
    }

    pub fn text_of(&self, cell: Cell) -> &str {
        self.cells
            .get(cell.row)
            .and_then(|row| row.get(cell.col))
            .map_or("", String::as_str)
    }

    /// The selected cells as text for the clipboard.
    #[must_use]
    pub fn copied(&self) -> String {
        let rows: Vec<Vec<String>> = self
            .range
            .rows()
            .map(|row| {
                self.range
                    .cols()
                    .map(|col| self.text_of(Cell { row, col }).to_string())
                    .collect()
            })
            .collect();
        cells_to_text(&rows)
    }

    fn parse(&self, key: Option<&str>) -> Option<Cell> {
        let (row, col) = key?.split_once(',')?;
        let cell = Cell {
            row: row.trim().parse().ok()?,
            col: col.trim().parse().ok()?,
        };
        (cell.row < self.rows() && cell.col < self.cols()).then_some(cell)
    }

    /// Selects a range, if the owner allows.
    fn select(&mut self, range: CellRange, source: Source, reason: Reason, gate: Gate) -> bool {
        let next: Vec<String> = range.cells().map(key_of).collect();
        if next == self.value() {
            self.range = range;
            return true;
        }
        let allowed = gate(&ValueChange {
            value: next,
            source,
            reason,
        });
        if allowed {
            self.range = range;
        }
        allowed
    }

    fn go_to(&mut self, cell: Cell, extend: bool, reason: Reason, gate: Gate) -> Reaction<Part> {
        let range = if extend {
            CellRange {
                anchor: self.range.anchor,
                focus: cell,
            }
        } else {
            CellRange::new(cell)
        };
        if self.select(range, Source::User, reason, gate) {
            Reaction::item(Part::Cell, true, Some(key_of(cell)))
        } else {
            Reaction::handled()
        }
    }

    /// Writes cells, if the owner allows. `edits` are the cells that would change.
    fn write(&mut self, edits: Vec<(Cell, String)>, source: Source, gate: Gate) -> bool {
        let changed: Vec<(Cell, String)> = edits
            .into_iter()
            .filter(|(cell, text)| {
                self.parse(Some(&key_of(*cell))).is_some() && self.text_of(*cell) != text
            })
            .collect();
        if changed.is_empty() {
            return true;
        }
        let value = changed
            .iter()
            .map(|(cell, text)| format!("{}={text}", key_of(*cell)))
            .collect();
        let allowed = gate(&ValueChange {
            value,
            source,
            reason: Reason::Edit,
        });
        if allowed {
            for (cell, text) in changed {
                let slot = self
                    .cells
                    .get_mut(cell.row)
                    .and_then(|row| row.get_mut(cell.col));
                if let Some(slot) = slot {
                    *slot = text;
                }
            }
        }
        allowed
    }

    /// Pasted cells land with their top-left corner on the selection's.
    fn pasted(&self, text: &str) -> Vec<(Cell, String)> {
        let (top, left) = (*self.range.rows().start(), *self.range.cols().start());
        let mut edits = Vec::new();
        for (r, row) in text_to_cells(text).into_iter().enumerate() {
            for (c, value) in row.into_iter().enumerate() {
                edits.push((
                    Cell {
                        row: top.saturating_add(r),
                        col: left.saturating_add(c),
                    },
                    value,
                ));
            }
        }
        edits
    }
}

impl PartOf for Part {
    type Primitive = DataGrid;
}

impl Primitive for DataGrid {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Cell]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match (part, self.parse(item)) {
            (Part::Root, _) => Semantics {
                role: Some(Role::Grid),
                multiselectable: Some(true),
                ..Default::default()
            },
            (Part::Cell, Some(cell)) => Semantics {
                role: Some(Role::GridCell),
                selected: Some(self.range.contains(cell)),
                focusable: if cell == self.range.focus {
                    Focusable::TabStop
                } else {
                    Focusable::Programmatic
                },
                active: cell == self.range.focus,
                ..Default::default()
            },
            (Part::Cell, None) => Semantics {
                role: Some(Role::GridCell),
                hidden: item.is_some(),
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Key {
                key: Key::Char('a' | 'A'),
                mods,
                ..
            } if mods.ctrl => {
                let last = Cell {
                    row: self.rows().saturating_sub(1),
                    col: self.cols().saturating_sub(1),
                };
                let all = CellRange {
                    anchor: Cell { row: 0, col: 0 },
                    focus: last,
                };
                self.select(all, Source::User, Reason::Keyboard, gate);
                Reaction::handled()
            }
            Input::Key {
                key: Key::Char('c' | 'C'),
                mods,
                ..
            } if mods.ctrl => Reaction::handled().with(Effect::Copy {
                text: self.copied(),
            }),
            Input::Key {
                key: Key::Backspace,
                ..
            } => {
                let edits = self
                    .range
                    .cells()
                    .map(|cell| (cell, String::new()))
                    .collect();
                self.write(edits, Source::User, gate);
                Reaction::handled()
            }
            Input::Key { key, mods, .. } => {
                let Some(nav) = nav_for_key(key, mods, self.dir) else {
                    return Reaction::default();
                };
                step(self.rows(), self.cols(), self.range.focus, nav, |_| true)
                    .map_or_else(Reaction::handled, |cell| {
                        self.go_to(cell, mods.shift, Reason::Keyboard, gate)
                    })
            }
            Input::Paste { text } => {
                let edits = self.pasted(&text);
                self.write(edits, Source::User, gate);
                Reaction::handled()
            }
            Input::Press {
                part: Part::Cell,
                item,
                mods,
            } => self
                .parse(item.as_deref())
                .map_or_else(Reaction::default, |cell| {
                    self.go_to(cell, mods.shift, Reason::Press, gate)
                }),
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        self.range.cells().map(key_of).collect()
    }

    fn set_value(&mut self, value: Vec<String>) {
        let cells: Vec<Cell> = value
            .iter()
            .filter_map(|key| self.parse(Some(key)))
            .collect();
        let (Some(first), Some(last)) = (cells.first(), cells.last()) else {
            return;
        };
        self.range = CellRange {
            anchor: *first,
            focus: *last,
        };
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "grid",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        match action {
            // `0,0` selects one cell and `0,0:1,2` a rectangle.
            "select" => {
                let text = item.ok_or(ActError::MissingItem { action: "select" })?;
                let (from, to) = text.split_once(':').unwrap_or((text, text));
                let unknown = || ActError::UnknownItem(text.to_string());
                let anchor = self.parse(Some(from)).ok_or_else(unknown)?;
                let focus = self.parse(Some(to)).ok_or_else(unknown)?;
                if self.select(
                    CellRange { anchor, focus },
                    Source::Agent,
                    Reason::Action,
                    gate,
                ) {
                    Ok(())
                } else {
                    Err(ActError::Refused)
                }
            }
            "paste" | "clear" => {
                let edits = if action == "clear" {
                    self.range
                        .cells()
                        .map(|cell| (cell, String::new()))
                        .collect()
                } else {
                    self.pasted(item.ok_or(ActError::MissingItem { action: "paste" })?)
                };
                if self.write(edits, Source::Agent, gate) {
                    Ok(())
                } else {
                    Err(ActError::Refused)
                }
            }
            other => Err(ActError::UnknownAction {
                action: other.to_string(),
                valid: ACTIONS,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Modifiers;

    fn grid() -> DataGrid {
        let cells = [["a1", "b1", "c1"], ["a2", "b2", "c2"], ["a3", "b3", "c3"]];
        DataGrid::new(
            cells
                .iter()
                .map(|row| row.iter().map(std::string::ToString::to_string).collect())
                .collect(),
            Dir::Ltr,
        )
    }

    fn key(grid: &mut DataGrid, key: Key, mods: Modifiers) -> Reaction<Part> {
        grid.send(
            Input::Key {
                key,
                mods,
                now_ms: 0,
            },
            &mut |_| true,
        )
    }

    const NONE: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
    };
    const SHIFT: Modifiers = Modifiers {
        shift: true,
        ctrl: false,
        alt: false,
    };
    const CTRL: Modifiers = Modifiers {
        shift: false,
        ctrl: true,
        alt: false,
    };

    #[test]
    fn arrows_move_one_cell_and_shift_grows_a_rectangle() {
        let mut grid = grid();
        assert_eq!(
            key(&mut grid, Key::ArrowRight, NONE).focus_item(),
            Some("0,1")
        );
        key(&mut grid, Key::ArrowDown, SHIFT);
        key(&mut grid, Key::ArrowRight, SHIFT);
        assert_eq!(grid.value(), ["0,1", "0,2", "1,1", "1,2"]);
        assert_eq!(
            grid.semantics(Part::Cell, Some("1,2")).focusable,
            Focusable::TabStop
        );
        assert_eq!(
            grid.semantics(Part::Cell, Some("0,0")).selected,
            Some(false)
        );
    }

    #[test]
    fn copying_gives_the_selection_as_spreadsheet_text() {
        let mut grid = grid();
        key(&mut grid, Key::ArrowDown, SHIFT);
        key(&mut grid, Key::ArrowRight, SHIFT);
        let copied = key(&mut grid, Key::Char('c'), CTRL);
        assert_eq!(
            copied.effects,
            [Effect::Copy {
                text: "a1\tb1\na2\tb2".into()
            }]
        );
    }

    #[test]
    fn pasting_fills_from_the_selections_corner_and_drops_what_falls_outside() {
        let mut grid = grid();
        key(&mut grid, Key::End, CTRL);
        let mut offered = Vec::new();
        grid.send(
            Input::Paste {
                text: "X\tY\nZ\tW".into(),
            },
            &mut |change| {
                offered = change.value.clone();
                true
            },
        );
        assert_eq!(offered, ["2,2=X"]);
        assert_eq!(grid.text_of(Cell { row: 2, col: 2 }), "X");
    }

    #[test]
    fn select_all_then_backspace_empties_every_cell_unless_refused() {
        let mut grid = grid();
        key(&mut grid, Key::Char('a'), CTRL);
        assert_eq!(grid.value().len(), 9);
        grid.send(
            Input::Key {
                key: Key::Backspace,
                mods: NONE,
                now_ms: 0,
            },
            &mut |_| false,
        );
        assert_eq!(grid.text_of(Cell { row: 1, col: 1 }), "b2");
        key(&mut grid, Key::Backspace, NONE);
        assert_eq!(grid.copied(), "\t\t\n\t\t\n\t\t");
    }

    #[test]
    fn an_agent_selects_a_rectangle_and_pastes_into_it() {
        let mut grid = grid();
        grid.act("select", Some("1,0:2,1"), &mut |_| true).unwrap();
        assert_eq!(grid.value(), ["1,0", "1,1", "2,0", "2,1"]);
        grid.act("paste", Some("9\t8"), &mut |_| true).unwrap();
        assert_eq!(grid.text_of(Cell { row: 1, col: 1 }), "8");
        assert_eq!(
            grid.act("select", Some("7,7"), &mut |_| true),
            Err(ActError::UnknownItem("7,7".into()))
        );
    }

    #[test]
    fn it_conforms() {
        let cells: Vec<Item> = ["0,0", "1,2"]
            .iter()
            .map(|key| Item {
                key: key.to_string(),
                label: key.to_string(),
                disabled: false,
            })
            .collect();
        crate::testing::assert_conforms(grid, &cells);
    }
}
