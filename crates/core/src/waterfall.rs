use std::collections::HashSet;

use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::{Dir, Key};
use crate::primitive::{Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics};

/// One step of a trace: a request, a call, anything with a start and a length. Its
/// `parent` is the span that started it.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub key: String,
    pub label: String,
    pub start_ms: f64,
    pub duration_ms: f64,
    pub parent: Option<String>,
}

impl Span {
    pub fn new(key: &str, start_ms: f64, duration_ms: f64, parent: Option<&str>) -> Self {
        Span {
            key: key.into(),
            label: key.into(),
            start_ms,
            duration_ms,
            parent: parent.map(str::to_string),
        }
    }
}

/// A row to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub key: String,
    /// How deep it is nested, from 1.
    pub level: usize,
    pub has_children: bool,
    pub expanded: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    /// One span's row. Render one per entry in `rows`.
    Row,
    /// The span's bar on the time axis. Place it with `bar`.
    Bar,
    /// The time axis. Draw its marks from `ticks`.
    Axis,
}

const ACTIONS: &[&str] = &["select", "expand", "collapse", "expand_all", "collapse_all"];

/// A trace drawn as nested rows against a time axis, as a network or profiler
/// waterfall is. Rows open and close like a tree. The value is the selected span.
#[derive(Clone, Debug, Default)]
pub struct Waterfall {
    spans: Vec<Span>,
    dir: Dir,
    /// The stretch of time the axis shows.
    domain: (f64, f64),
    expanded: HashSet<String>,
    selected: Option<String>,
    focused: Option<String>,
}

impl Waterfall {
    /// Opens every span, on an axis that just holds them all.
    pub fn new(spans: Vec<Span>, dir: Dir) -> Self {
        let start = spans
            .iter()
            .map(|span| span.start_ms)
            .fold(f64::INFINITY, f64::min);
        let end = spans
            .iter()
            .map(|span| span.start_ms + span.duration_ms)
            .fold(f64::NEG_INFINITY, f64::max);
        let domain = if spans.is_empty() {
            (0.0, 1.0)
        } else {
            (start, end.max(start + 1.0))
        };
        let expanded = spans.iter().map(|span| span.key.clone()).collect();
        Waterfall {
            spans,
            dir,
            domain,
            expanded,
            selected: None,
            focused: None,
        }
    }

    /// Shows another stretch of time, to zoom or pan. Bars outside it shrink to its
    /// edges.
    pub fn showing(mut self, start_ms: f64, end_ms: f64) -> Self {
        self.domain = (start_ms, end_ms.max(start_ms + f64::MIN_POSITIVE));
        self
    }

    fn span(&self, key: &str) -> Option<&Span> {
        self.spans.iter().find(|span| span.key == key)
    }

    fn children(&self, key: Option<&str>) -> Vec<&Span> {
        self.spans
            .iter()
            .filter(|span| span.parent.as_deref() == key)
            .collect()
    }

    /// The rows on show, parents before the spans they started.
    pub fn rows(&self) -> Vec<Row> {
        fn walk(waterfall: &Waterfall, parent: Option<&str>, level: usize, rows: &mut Vec<Row>) {
            for span in waterfall.children(parent) {
                let has_children = !waterfall.children(Some(&span.key)).is_empty();
                let expanded = has_children && waterfall.expanded.contains(&span.key);
                rows.push(Row {
                    key: span.key.clone(),
                    level,
                    has_children,
                    expanded,
                });
                if expanded {
                    walk(waterfall, Some(&span.key), level + 1, rows);
                }
            }
        }
        let mut rows = Vec::new();
        walk(self, None, 1, &mut rows);
        rows
    }

    /// Where a span's bar starts and how long it is, as fractions of the axis.
    pub fn bar(&self, key: &str) -> Option<(f64, f64)> {
        let span = self.span(key)?;
        let (start, end) = self.domain;
        let at = |time: f64| ((time - start) / (end - start)).clamp(0.0, 1.0);
        let from = at(span.start_ms);
        Some((from, at(span.start_ms + span.duration_ms) - from))
    }

    /// At most `most` marks for the axis, on round times: where each sits as a
    /// fraction of the axis, and its time.
    pub fn ticks(&self, most: usize) -> Vec<(f64, f64)> {
        let (start, end) = self.domain;
        let rough = (end - start) / most.max(1) as f64;
        let magnitude = 10f64.powf(rough.log10().floor());
        let step = [1.0, 2.0, 5.0, 10.0]
            .into_iter()
            .map(|round| round * magnitude)
            .find(|step| *step >= rough)
            .unwrap_or(rough);
        let first = (start / step).ceil() as i64;
        (first..)
            .map(|n| n as f64 * step)
            .take_while(|time| *time <= end)
            .map(|time| ((time - start) / (end - start), time))
            .collect()
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

    fn current(&self, rows: &[Row]) -> Option<String> {
        let on_show = |key: &&str| rows.iter().any(|row| row.key == *key);
        self.focused
            .as_deref()
            .filter(on_show)
            .or(rows.first().map(|row| row.key.as_str()))
            .map(str::to_string)
    }

    fn move_to(&mut self, key: Option<String>) -> Reaction<Part> {
        if let Some(key) = &key {
            self.focused = Some(key.clone());
        }
        Reaction::item(Part::Row, true, key)
    }
}

impl PartOf for Part {
    type Primitive = Waterfall;
}

impl Primitive for Waterfall {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Row, Part::Bar, Part::Axis]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn named_part(&self) -> Option<Part> {
        Some(Part::Root)
    }

    /// The trace owns its spans. What is rendered only renames them.
    fn upsert_item(&mut self, item: Item) {
        if let Some(span) = self
            .spans
            .iter_mut()
            .find(|span| span.key == item.key && !item.label.is_empty())
        {
            span.label = item.label;
        }
    }

    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        let rows = self.rows();
        let row = item.and_then(|key| rows.iter().find(|row| row.key == key));
        match (part, row) {
            (Part::Root, _) => Semantics {
                role: Some(Role::Tree),
                ..Default::default()
            },
            (Part::Axis, _) => Semantics::default(),
            (Part::Row, Some(row)) => Semantics {
                role: Some(Role::TreeItem),
                level: Some(row.level as u32),
                expanded: row.has_children.then_some(row.expanded),
                selected: Some(self.selected.as_deref() == Some(row.key.as_str())),
                focusable: if self.current(&rows).as_deref() == Some(row.key.as_str()) {
                    Focusable::TabStop
                } else {
                    Focusable::Programmatic
                },
                active: self.focused.as_deref() == Some(row.key.as_str()),
                ..Default::default()
            },
            (Part::Bar, Some(_)) => Semantics::default(),
            (Part::Row, None) => Semantics {
                role: Some(Role::TreeItem),
                hidden: item.is_some(),
                ..Default::default()
            },
            (Part::Bar, None) => Semantics {
                hidden: item.is_some(),
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        let rows = self.rows();
        match input {
            Input::Focus {
                part: Part::Row,
                item,
            } => {
                self.focused = item;
                Reaction::default()
            }
            Input::Press {
                part: Part::Row | Part::Bar,
                item: Some(key),
                ..
            } => {
                self.select(&key, Source::User, Reason::Press, gate);
                self.move_to(Some(key))
            }
            Input::Key { key, mods, .. } if !mods.ctrl && !mods.alt => {
                let Some(current) = self.current(&rows) else {
                    return Reaction::default();
                };
                let at = rows.iter().position(|row| row.key == current).unwrap_or(0);
                let (open, close) = if self.dir == Dir::Rtl {
                    (Key::ArrowLeft, Key::ArrowRight)
                } else {
                    (Key::ArrowRight, Key::ArrowLeft)
                };
                match key {
                    Key::ArrowDown => self.move_to(rows.get(at + 1).map(|row| row.key.clone())),
                    Key::ArrowUp => self.move_to(at.checked_sub(1).map(|at| rows[at].key.clone())),
                    Key::Home => self.move_to(rows.first().map(|row| row.key.clone())),
                    Key::End => self.move_to(rows.last().map(|row| row.key.clone())),
                    // Opening a closed row, then stepping into it. Closing an open
                    // one, then stepping out to its parent.
                    k if k == open && rows[at].has_children && !rows[at].expanded => {
                        self.expanded.insert(current);
                        Reaction::handled()
                    }
                    k if k == open && rows[at].has_children => {
                        self.move_to(rows.get(at + 1).map(|row| row.key.clone()))
                    }
                    k if k == close && rows[at].expanded => {
                        self.expanded.remove(&current);
                        Reaction::handled()
                    }
                    k if k == close => {
                        self.move_to(self.span(&current).and_then(|span| span.parent.clone()))
                    }
                    Key::Enter | Key::Space => {
                        self.select(&current, Source::User, Reason::Keyboard, gate);
                        Reaction::handled()
                    }
                    _ => Reaction::default(),
                }
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        self.selected.iter().cloned().collect()
    }

    fn set_value(&mut self, value: Vec<String>) {
        self.selected = value.into_iter().find(|key| self.span(key).is_some());
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .rows()
            .into_iter()
            .map(|row| SnapshotItem {
                label: self
                    .span(&row.key)
                    .map(|span| span.label.clone())
                    .unwrap_or_default(),
                disabled: false,
                selected: self.selected.as_deref() == Some(row.key.as_str()),
                key: row.key,
            })
            .collect();
        Snapshot {
            name: "waterfall",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let action: &'static str = match action {
            "expand_all" => {
                self.expanded = self.spans.iter().map(|span| span.key.clone()).collect();
                return Ok(());
            }
            "collapse_all" => {
                self.expanded.clear();
                return Ok(());
            }
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
        if self.span(key).is_none() {
            return Err(ActError::UnknownItem(key.to_string()));
        }
        match action {
            "expand" => drop(self.expanded.insert(key.to_string())),
            "collapse" => drop(self.expanded.remove(key)),
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

    fn trace() -> Waterfall {
        Waterfall::new(
            vec![
                Span::new("page", 0.0, 400.0, None),
                Span::new("html", 0.0, 120.0, Some("page")),
                Span::new("api", 100.0, 200.0, Some("page")),
                Span::new("db", 150.0, 100.0, Some("api")),
            ],
            Dir::Ltr,
        )
    }

    fn keys(waterfall: &Waterfall) -> Vec<String> {
        waterfall.rows().into_iter().map(|row| row.key).collect()
    }

    #[test]
    fn bars_sit_on_the_axis_and_shrink_to_its_edges_when_zoomed() {
        let waterfall = trace();
        assert_eq!(waterfall.bar("api"), Some((0.25, 0.5)));
        let zoomed = trace().showing(200.0, 400.0);
        assert_eq!(
            zoomed.bar("api"),
            Some((0.0, 0.5)),
            "it began before the axis does"
        );
        assert_eq!(zoomed.bar("html"), Some((0.0, 0.0)));
    }

    #[test]
    fn the_axis_is_marked_at_round_times() {
        let marks: Vec<f64> = trace().ticks(5).into_iter().map(|(_, time)| time).collect();
        assert_eq!(marks, [0.0, 100.0, 200.0, 300.0, 400.0]);
        let few: Vec<f64> = trace().ticks(3).into_iter().map(|(_, time)| time).collect();
        assert_eq!(few, [0.0, 200.0, 400.0]);
    }

    #[test]
    fn rows_open_and_close_like_a_tree() {
        let mut waterfall = trace();
        let key = |waterfall: &mut Waterfall, key| {
            waterfall.send(
                Input::Key {
                    key,
                    mods: Modifiers::default(),
                    now_ms: 0,
                },
                &mut |_| true,
            )
        };
        assert_eq!(keys(&waterfall), ["page", "html", "api", "db"]);
        key(&mut waterfall, Key::ArrowDown);
        assert_eq!(
            key(&mut waterfall, Key::ArrowDown).focus_item(),
            Some("api")
        );
        key(&mut waterfall, Key::ArrowLeft);
        assert_eq!(keys(&waterfall), ["page", "html", "api"]);
        assert_eq!(
            key(&mut waterfall, Key::ArrowLeft).focus_item(),
            Some("page"),
            "then out to its parent"
        );
        key(&mut waterfall, Key::Enter);
        assert_eq!(waterfall.value(), ["page"]);
        assert_eq!(waterfall.semantics(Part::Row, Some("api")).level, Some(2));
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(trace, &[]);
    }
}
