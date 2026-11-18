use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::Key;
use crate::primitive::{
    Effect, Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics,
};
use crate::scroll::{ScrollAlign, Virtualizer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The scrolling box. Give it a fixed height and put a spacer of `total_size`
    /// inside it.
    Viewport,
    /// One row. Render one per entry in `rows`, placed at its offset.
    Row,
}

/// A row to render.
#[derive(Clone, Debug, PartialEq)]
pub struct VirtualRow {
    pub index: usize,
    /// The row's item key, which is its index as text.
    pub key: String,
    /// Where it starts inside the scrolling content.
    pub offset: f64,
}

const ACTIONS: &[&str] = &["select", "scroll_to"];
const OVERSCAN: usize = 4;

/// A list of any length that only renders the rows in view. The value is the selected
/// row's index.
#[derive(Clone, Debug)]
pub struct VirtualList {
    rows: Virtualizer,
    count: usize,
    offset: f64,
    viewport: f64,
    focused: usize,
    selected: Option<usize>,
}

impl Default for VirtualList {
    fn default() -> Self {
        Self::new(1000, 24.0)
    }
}

impl VirtualList {
    /// A list of `count` rows, each `row_size` tall until measured.
    #[must_use]
    pub fn new(count: usize, row_size: f64) -> Self {
        Self {
            rows: Virtualizer::new(count, row_size, 0.0),
            count,
            offset: 0.0,
            viewport: 0.0,
            focused: 0,
            selected: None,
        }
    }

    /// The height of all the rows together, for the spacer that gives the scrollbar
    /// its length.
    #[must_use]
    pub fn total_size(&self) -> f64 {
        self.rows.total_size()
    }

    /// The rows to render now.
    #[must_use]
    pub fn rows(&self) -> Vec<VirtualRow> {
        self.rows
            .range(self.offset, self.viewport, OVERSCAN)
            .map(|index| VirtualRow {
                index,
                key: index.to_string(),
                offset: self.rows.offset_of(index),
            })
            .collect()
    }

    /// Records a row's real size once it has been rendered.
    pub fn measure(&mut self, index: usize, size: f64) {
        self.rows.measure(index, size);
    }

    fn index_of(&self, key: Option<&str>) -> Option<usize> {
        key?.parse::<usize>()
            .ok()
            .filter(|index| *index < self.count)
    }

    /// Moves the keyboard to a row, scrolling only as far as needed to show it.
    fn go_to(&mut self, index: usize) -> Reaction<Part> {
        self.focused = index.min(self.count.saturating_sub(1));
        let start = self.rows.offset_of(self.focused);
        let end = self.rows.offset_of(self.focused.saturating_add(1));
        let target = if start < self.offset {
            Some(
                self.rows
                    .scroll_to(self.focused, ScrollAlign::Start, self.viewport),
            )
        } else if end > self.offset + self.viewport {
            Some(
                self.rows
                    .scroll_to(self.focused, ScrollAlign::End, self.viewport),
            )
        } else {
            None
        };
        let reaction = Reaction::item(Part::Row, true, Some(self.focused.to_string()));
        match target {
            Some(offset) => {
                self.offset = offset;
                reaction.with(Effect::ScrollTo {
                    offset: offset.round() as u32,
                })
            }
            None => reaction,
        }
    }

    fn select(&mut self, index: usize, source: Source, reason: Reason, gate: Gate) -> bool {
        if self.selected == Some(index) {
            return true;
        }
        let allowed = gate(&ValueChange {
            value: vec![index.to_string()],
            source,
            reason,
        });
        if allowed {
            self.selected = Some(index);
        }
        allowed
    }
}

impl PartOf for Part {
    type Primitive = VirtualList;
}

impl Primitive for VirtualList {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Viewport, Part::Row]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn scrolls(part: Part) -> bool {
        part == Part::Viewport
    }

    fn wanted_scroll(&self) -> Option<f64> {
        Some(self.offset)
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match (part, self.index_of(item)) {
            (Part::Viewport, _) => Semantics {
                role: Some(Role::Listbox),
                ..Default::default()
            },
            (Part::Row, Some(index)) => Semantics {
                role: Some(Role::Option),
                selected: Some(self.selected == Some(index)),
                focusable: if index == self.focused {
                    Focusable::TabStop
                } else {
                    Focusable::Programmatic
                },
                active: index == self.focused,
                ..Default::default()
            },
            (Part::Row, None) => Semantics {
                role: Some(Role::Option),
                hidden: item.is_some(),
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        if self.count == 0 {
            return Reaction::default();
        }
        // A page is the rows that fit, less one so the last stays in view as context.
        let page = ((self.viewport / self.rows.offset_of(1).max(1.0)) as usize)
            .saturating_sub(1)
            .max(1);
        match input {
            Input::Scrolled {
                offset, viewport, ..
            } => {
                self.offset = offset;
                self.viewport = viewport;
                Reaction::default()
            }
            Input::Key {
                key: Key::ArrowDown,
                ..
            } => self.go_to(self.focused.saturating_add(1)),
            Input::Key {
                key: Key::ArrowUp, ..
            } => self.go_to(self.focused.saturating_sub(1)),
            Input::Key {
                key: Key::PageDown, ..
            } => self.go_to(self.focused.saturating_add(page)),
            Input::Key {
                key: Key::PageUp, ..
            } => self.go_to(self.focused.saturating_sub(page)),
            Input::Key { key: Key::Home, .. } => self.go_to(0),
            Input::Key { key: Key::End, .. } => self.go_to(self.count.saturating_sub(1)),
            Input::Key {
                key: Key::Enter | Key::Space,
                ..
            } => {
                self.select(self.focused, Source::User, Reason::Keyboard, gate);
                Reaction::handled()
            }
            Input::Focus {
                part: Part::Row,
                item,
            } => {
                if let Some(index) = self.index_of(item.as_deref()) {
                    self.focused = index;
                }
                Reaction::default()
            }
            Input::Press {
                part: Part::Row,
                item,
                ..
            } => match self.index_of(item.as_deref()) {
                Some(index) => {
                    self.focused = index;
                    self.select(index, Source::User, Reason::Press, gate);
                    Reaction::item(Part::Row, true, Some(index.to_string()))
                }
                None => Reaction::default(),
            },
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        self.selected
            .iter()
            .map(std::string::ToString::to_string)
            .collect()
    }

    fn set_value(&mut self, value: Vec<String>) {
        if let Some(index) = self.index_of(value.first().map(String::as_str)) {
            self.selected = Some(index);
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "virtual-list",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let action: &'static str = match action {
            "select" => "select",
            "scroll_to" => "scroll_to",
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        let text = item.ok_or(ActError::MissingItem { action })?;
        let index = self
            .index_of(Some(text))
            .ok_or_else(|| ActError::UnknownItem(text.to_string()))?;
        if action == "select" && !self.select(index, Source::Agent, Reason::Action, gate) {
            return Err(ActError::Refused);
        }
        self.focused = index;
        self.offset = self
            .rows
            .scroll_to(index, ScrollAlign::Center, self.viewport);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Modifiers;

    fn list() -> VirtualList {
        let mut list = VirtualList::new(10_000, 20.0);
        list.send(
            Input::Scrolled {
                part: Part::Viewport,
                offset: 0.0,
                viewport: 100.0,
                content: 10_000.0,
                now_ms: 0,
            },
            &mut |_| true,
        );
        list
    }

    fn key(list: &mut VirtualList, key: Key) -> Reaction<Part> {
        list.send(
            Input::Key {
                key,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        )
    }

    #[test]
    fn only_the_rows_in_view_are_rendered_however_long_the_list() {
        let mut list = list();
        assert_eq!(list.rows().len(), 5 + OVERSCAN);
        assert_eq!(list.total_size(), 200_000.0);
        list.send(
            Input::Scrolled {
                part: Part::Viewport,
                offset: 100_000.0,
                viewport: 100.0,
                content: 10_000.0,
                now_ms: 0,
            },
            &mut |_| true,
        );
        let rows = list.rows();
        assert_eq!(rows.first().unwrap().index, 5000 - OVERSCAN);
        assert_eq!(
            rows.iter().find(|row| row.index == 5000).unwrap().offset,
            100_000.0
        );
    }

    #[test]
    fn arrows_scroll_only_when_the_next_row_is_out_of_view() {
        let mut list = list();
        let within = key(&mut list, Key::ArrowDown);
        assert_eq!(within.focus_item(), Some("1"));
        assert!(within.effects.is_empty());
        for _ in 0..4 {
            key(&mut list, Key::ArrowDown);
        }
        let past = list.send(
            Input::Key {
                key: Key::ArrowDown,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        );
        assert_eq!(past.focus_item(), Some("6"));
        assert_eq!(
            past.effects,
            [Effect::ScrollTo { offset: 40 }],
            "just enough to show row 6 at the bottom"
        );
    }

    #[test]
    fn end_and_home_jump_to_the_ends() {
        let mut list = list();
        assert_eq!(key(&mut list, Key::End).focus_item(), Some("9999"));
        assert!(
            list.rows().iter().any(|row| row.index == 9999),
            "the last row is rendered"
        );
        assert_eq!(
            key(&mut list, Key::Home).effects,
            [Effect::ScrollTo { offset: 0 }]
        );
    }

    #[test]
    fn page_keys_move_by_what_fits_less_one() {
        let mut list = list();
        assert_eq!(key(&mut list, Key::PageDown).focus_item(), Some("4"));
        assert_eq!(key(&mut list, Key::PageUp).focus_item(), Some("0"));
    }

    #[test]
    fn enter_selects_the_focused_row() {
        let mut list = list();
        key(&mut list, Key::ArrowDown);
        key(&mut list, Key::Enter);
        assert_eq!(list.value(), ["1"]);
        assert_eq!(list.semantics(Part::Row, Some("1")).selected, Some(true));
        assert!(
            list.semantics(Part::Row, Some("10000")).hidden,
            "past the end"
        );
    }

    #[test]
    fn it_conforms() {
        let rows: Vec<Item> = ["0", "1", "2"]
            .iter()
            .map(|key| Item {
                key: key.to_string(),
                label: key.to_string(),
                disabled: false,
            })
            .collect();
        crate::testing::assert_conforms(VirtualList::default, &rows);
    }
}
