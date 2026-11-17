//! Two-dimensional focus and selection over cells addressed by row and column, so they
//! work for cells that are not rendered.

use std::ops::RangeInclusive;

use crate::keys::{Dir, Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Cell {
    pub row: usize,
    pub col: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridNav {
    Up,
    Down,
    Left,
    Right,
    RowStart,
    RowEnd,
    First,
    Last,
}

/// Arrows move by one cell. Home and End go to the ends of the row, or of the whole
/// grid with Ctrl.
pub fn nav_for_key(key: Key, mods: Modifiers, dir: Dir) -> Option<GridNav> {
    Some(match (key, dir) {
        (Key::ArrowUp, _) => GridNav::Up,
        (Key::ArrowDown, _) => GridNav::Down,
        (Key::ArrowLeft, Dir::Ltr) | (Key::ArrowRight, Dir::Rtl) => GridNav::Left,
        (Key::ArrowRight, Dir::Ltr) | (Key::ArrowLeft, Dir::Rtl) => GridNav::Right,
        (Key::Home, _) if mods.ctrl => GridNav::First,
        (Key::End, _) if mods.ctrl => GridNav::Last,
        (Key::Home, _) => GridNav::RowStart,
        (Key::End, _) => GridNav::RowEnd,
        _ => return None,
    })
}

/// The cell to move to in a `rows` by `cols` grid, skipping cells `enabled` rejects.
/// `None` means there is nowhere to go.
pub fn step(
    rows: usize,
    cols: usize,
    from: Cell,
    nav: GridNav,
    enabled: impl Fn(Cell) -> bool,
) -> Option<Cell> {
    let in_row = |row: usize| (0..cols).map(move |col| Cell { row, col });
    let in_col = |col: usize| (0..rows).map(move |row| Cell { row, col });
    let all = || (0..rows).flat_map(in_row);
    let enabled = |cell: &Cell| enabled(*cell);
    match nav {
        GridNav::Up => in_col(from.col).take(from.row).filter(enabled).last(),
        GridNav::Down => in_col(from.col).skip(from.row + 1).find(enabled),
        GridNav::Left => in_row(from.row).take(from.col).filter(enabled).last(),
        GridNav::Right => in_row(from.row).skip(from.col + 1).find(enabled),
        GridNav::RowStart => in_row(from.row).find(enabled),
        GridNav::RowEnd => in_row(from.row).filter(enabled).last(),
        GridNav::First => all().find(enabled),
        GridNav::Last => all().filter(enabled).last(),
    }
}

/// A rectangular selection between the cell where it started and the cell it was
/// extended to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellRange {
    pub anchor: Cell,
    pub focus: Cell,
}

impl CellRange {
    pub fn new(anchor: Cell) -> Self {
        CellRange {
            anchor,
            focus: anchor,
        }
    }

    pub fn extend_to(&mut self, cell: Cell) {
        self.focus = cell;
    }

    pub fn rows(&self) -> RangeInclusive<usize> {
        self.anchor.row.min(self.focus.row)..=self.anchor.row.max(self.focus.row)
    }

    pub fn cols(&self) -> RangeInclusive<usize> {
        self.anchor.col.min(self.focus.col)..=self.anchor.col.max(self.focus.col)
    }

    pub fn contains(&self, cell: Cell) -> bool {
        self.rows().contains(&cell.row) && self.cols().contains(&cell.col)
    }

    /// Every cell in the range, row by row.
    pub fn cells(&self) -> impl Iterator<Item = Cell> {
        let cols = self.cols();
        self.rows()
            .flat_map(move |row| cols.clone().map(move |col| Cell { row, col }))
    }
}

/// Cell values as clipboard text that spreadsheets accept: tabs between columns,
/// newlines between rows, and quotes around a value that contains either.
pub fn cells_to_text(rows: &[Vec<String>]) -> String {
    let field = |value: &String| {
        if value.contains(['\t', '\n', '"']) {
            format!("\"{}\"", value.replace('"', "\"\""))
        } else {
            value.clone()
        }
    };
    rows.iter()
        .map(|row| row.iter().map(field).collect::<Vec<_>>().join("\t"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parses clipboard text from a spreadsheet into rows of cell values.
pub fn text_to_cells(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            '"' if quoted => quoted = false,
            '"' if field.is_empty() => quoted = true,
            '\t' if !quoted => row.push(std::mem::take(&mut field)),
            '\r' if !quoted && chars.peek() == Some(&'\n') => {}
            '\n' if !quoted => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            other => field.push(other),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(row: usize, col: usize) -> Cell {
        Cell { row, col }
    }

    #[test]
    fn arrows_move_one_cell_and_stop_at_the_edge() {
        let any = |_| true;
        assert_eq!(step(3, 3, cell(1, 1), GridNav::Down, any), Some(cell(2, 1)));
        assert_eq!(step(3, 3, cell(2, 1), GridNav::Down, any), None);
        assert_eq!(step(3, 3, cell(1, 0), GridNav::Left, any), None);
    }

    #[test]
    fn movement_skips_disabled_cells() {
        let enabled = |c: Cell| c != cell(1, 1);
        assert_eq!(
            step(3, 3, cell(1, 0), GridNav::Right, enabled),
            Some(cell(1, 2))
        );
        assert_eq!(
            step(3, 3, cell(2, 1), GridNav::Up, enabled),
            Some(cell(0, 1))
        );
    }

    #[test]
    fn home_and_end_stay_in_the_row_unless_ctrl_is_held() {
        let ctrl = Modifiers {
            shift: false,
            ctrl: true,
        };
        let none = Modifiers::default();
        assert_eq!(nav_for_key(Key::End, none, Dir::Ltr), Some(GridNav::RowEnd));
        assert_eq!(nav_for_key(Key::End, ctrl, Dir::Ltr), Some(GridNav::Last));
        assert_eq!(
            step(3, 4, cell(1, 1), GridNav::RowEnd, |_| true),
            Some(cell(1, 3))
        );
        assert_eq!(
            step(3, 4, cell(1, 1), GridNav::Last, |_| true),
            Some(cell(2, 3))
        );
    }

    #[test]
    fn horizontal_arrows_flip_in_rtl() {
        let none = Modifiers::default();
        assert_eq!(
            nav_for_key(Key::ArrowLeft, none, Dir::Rtl),
            Some(GridNav::Right)
        );
    }

    #[test]
    fn a_range_covers_the_rectangle_in_either_direction() {
        let mut range = CellRange::new(cell(2, 2));
        range.extend_to(cell(1, 0));
        assert!(range.contains(cell(1, 1)));
        assert!(!range.contains(cell(0, 1)));
        assert_eq!(range.cells().count(), 6);
        assert_eq!(range.cells().next(), Some(cell(1, 0)));
    }

    #[test]
    fn cells_survive_a_copy_and_paste_round_trip() {
        let rows = vec![
            vec!["plain".to_string(), "has\ttab".to_string()],
            vec!["say \"hi\"".to_string(), "two\nlines".to_string()],
        ];
        assert_eq!(text_to_cells(&cells_to_text(&rows)), rows);
    }

    #[test]
    fn pasted_text_may_use_windows_line_endings_and_a_trailing_newline() {
        assert_eq!(text_to_cells("a\tb\r\nc\td\r\n"), [["a", "b"], ["c", "d"]]);
    }
}
