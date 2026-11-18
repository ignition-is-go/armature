use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::Key;
use crate::primitive::{Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics};

/// One entry in a row of page links.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageItem {
    Page(usize),
    /// Pages left out before the current page's neighbours.
    EllipsisStart,
    /// Pages left out after them.
    EllipsisEnd,
}

/// The pages to show for `page` of `count`, with `siblings` on each side of the
/// current page and `boundaries` at each end.
///
/// The row is the same length on every page, so links do not shift under the pointer
/// while paging, and a gap of a single page shows that page.
pub fn page_range(page: usize, count: usize, siblings: usize, boundaries: usize) -> Vec<PageItem> {
    if count == 0 {
        return Vec::new();
    }
    let current = page.clamp(1, count);
    let slots = ((boundaries.saturating_mul(2)).saturating_add(siblings.saturating_mul(2)))
        .saturating_add(3);
    if count <= slots {
        return (1..=count).map(PageItem::Page).collect();
    }
    // The window around the current page slides to stay clear of the ends.
    let start = current
        .saturating_sub(siblings)
        .max(boundaries.saturating_add(2))
        .min(
            ((count.saturating_sub(boundaries)).saturating_sub(1))
                .saturating_sub(siblings.saturating_mul(2)),
        );
    let end = start.saturating_add(siblings.saturating_mul(2));
    let mut row: Vec<PageItem> = (1..=boundaries).map(PageItem::Page).collect();
    row.push(if start > boundaries.saturating_add(2) {
        PageItem::EllipsisStart
    } else {
        PageItem::Page(boundaries.saturating_add(1))
    });
    row.extend((start..=end).map(PageItem::Page));
    row.push(
        if end < (count.saturating_sub(boundaries)).saturating_sub(1) {
            PageItem::EllipsisEnd
        } else {
            PageItem::Page(count.saturating_sub(boundaries))
        },
    );
    row.extend(((count.saturating_sub(boundaries)).saturating_add(1)..=count).map(PageItem::Page));
    row
}

/// How many pages `total` items fill at `page_size` to a page. Never less than one.
#[must_use]
pub fn page_count(total: usize, page_size: usize) -> usize {
    if page_size == 0 {
        1
    } else {
        total.div_ceil(page_size).max(1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    First,
    Previous,
    /// One page link. Render one per `PageItem::Page` in `range`, keyed by its number.
    Page,
    Next,
    Last,
}

const ACTIONS: &[&str] = &["go_to", "next", "previous", "first", "last"];

/// Links for moving between pages. The value is the current page, counted from 1.
#[derive(Clone, Debug)]
pub struct Pagination {
    page: usize,
    count: usize,
    siblings: usize,
    boundaries: usize,
    focused: Option<(Part, Option<String>)>,
}

impl Default for Pagination {
    fn default() -> Self {
        Self::new(1, 10)
    }
}

impl Pagination {
    #[must_use]
    pub fn new(page: usize, count: usize) -> Self {
        let count = count.max(1);
        Self {
            page: page.clamp(1, count),
            count,
            siblings: 1,
            boundaries: 1,
            focused: None,
        }
    }

    #[must_use]
    pub const fn page(&self) -> usize {
        self.page
    }

    /// The row of links to draw.
    #[must_use]
    pub fn range(&self) -> Vec<PageItem> {
        page_range(self.page, self.count, self.siblings, self.boundaries)
    }

    fn go(&mut self, page: usize, source: Source, reason: Reason, gate: Gate) -> bool {
        let page = page.clamp(1, self.count);
        if page == self.page {
            return false;
        }
        let allowed = gate(&ValueChange {
            value: vec![page.to_string()],
            source,
            reason,
        });
        if allowed {
            self.page = page;
        }
        allowed
    }

    fn target(&self, part: Part, item: Option<&str>) -> Option<usize> {
        match part {
            Part::First => Some(1),
            Part::Previous => Some(self.page.saturating_sub(1).max(1)),
            Part::Next => Some((self.page.saturating_add(1)).min(self.count)),
            Part::Last => Some(self.count),
            Part::Page => item?.parse().ok(),
            Part::Root => None,
        }
    }
}

impl PartOf for Part {
    type Primitive = Pagination;
}

impl Primitive for Pagination {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[
            Part::Root,
            Part::First,
            Part::Previous,
            Part::Page,
            Part::Next,
            Part::Last,
        ]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        if part == Part::Root {
            return Semantics {
                role: Some(Role::Navigation),
                ..Default::default()
            };
        }
        let target = self.target(part, item);
        let current = part == Part::Page && target == Some(self.page);
        // A link that would go nowhere is disabled: back on the first page, forward on the last.
        let disabled = part != Part::Page && target == Some(self.page);
        Semantics {
            role: Some(Role::Button),
            current,
            disabled,
            focusable: if disabled || (part == Part::Page && target.is_none()) {
                Focusable::No
            } else {
                Focusable::TabStop
            },
            active: current,
            ..Default::default()
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Focus { part, item } => {
                self.focused = Some((part, item));
                Reaction::default()
            }
            Input::Press { part, item, .. } if part != Part::Root => {
                if let Some(page) = self.target(part, item.as_deref()) {
                    self.go(page, Source::User, Reason::Press, gate);
                }
                Reaction::handled()
            }
            Input::Key {
                key: Key::Enter | Key::Space,
                ..
            } => match self.focused.clone() {
                Some((part, item)) if part != Part::Root => {
                    if let Some(page) = self.target(part, item.as_deref()) {
                        self.go(page, Source::User, Reason::Keyboard, gate);
                    }
                    Reaction::handled()
                }
                _ => Reaction::default(),
            },
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        vec![self.page.to_string()]
    }

    fn set_value(&mut self, value: Vec<String>) {
        if let Some(page) = value.first().and_then(|text| text.parse::<usize>().ok()) {
            self.page = page.clamp(1, self.count);
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "pagination",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let page = match action {
            "next" => self.page.saturating_add(1),
            "previous" => self.page.saturating_sub(1),
            "first" => 1,
            "last" => self.count,
            "go_to" => {
                let text = item.ok_or(ActError::MissingItem { action: "go_to" })?;
                text.parse()
                    .map_err(|_| ActError::UnknownItem(text.to_string()))?
            }
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        let page = page.clamp(1, self.count);
        if page == self.page || self.go(page, Source::Agent, Reason::Action, gate) {
            Ok(())
        } else {
            Err(ActError::Refused)
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::too_many_lines,
    reason = "a test fails by panicking, and its arithmetic is on values it wrote itself"
)]
mod tests {
    use super::*;
    use crate::keys::Modifiers;
    use PageItem::{EllipsisEnd, EllipsisStart, Page};

    #[test]
    fn the_row_keeps_its_length_and_shows_a_page_in_place_of_a_gap_of_one() {
        assert_eq!(
            page_range(5, 10, 1, 1),
            [
                Page(1),
                EllipsisStart,
                Page(4),
                Page(5),
                Page(6),
                EllipsisEnd,
                Page(10)
            ]
        );
        assert_eq!(
            page_range(1, 10, 1, 1),
            [
                Page(1),
                Page(2),
                Page(3),
                Page(4),
                Page(5),
                EllipsisEnd,
                Page(10)
            ]
        );
        assert_eq!(
            page_range(10, 10, 1, 1),
            [
                Page(1),
                EllipsisStart,
                Page(6),
                Page(7),
                Page(8),
                Page(9),
                Page(10)
            ]
        );
        assert!((1..=10).all(|page| page_range(page, 10, 1, 1).len() == 7));
        assert_eq!(page_range(2, 4, 1, 1).len(), 4, "few pages are all shown");
        assert_eq!((page_count(0, 10), page_count(41, 10)), (1, 5));
    }

    #[test]
    fn links_page_and_the_ends_disable_the_ones_that_go_nowhere() {
        let mut pages = Pagination::new(1, 3);
        assert!(pages.semantics(Part::Previous, None).disabled);
        let press = |pages: &mut Pagination, part, item: Option<&str>| {
            pages.send(
                Input::Press {
                    part,
                    item: item.map(str::to_string),
                    mods: Modifiers::default(),
                },
                &mut |_| true,
            );
        };
        press(&mut pages, Part::Next, None);
        press(&mut pages, Part::Page, Some("3"));
        assert_eq!(pages.value(), ["3"]);
        assert!(pages.semantics(Part::Page, Some("3")).current);
        assert!(pages.semantics(Part::Next, None).disabled);
        pages.act("first", None, &mut |_| true).unwrap();
        assert_eq!(pages.page(), 1);
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(Pagination::default, &[]);
    }
}
