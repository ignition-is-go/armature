use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::Gate;
use crate::items::Items;
use crate::keys::{Dir, Key, Orientation};
use crate::primitive::{Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics};
use crate::roving::{self, Ends, Nav};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    /// One event, rendered per item.
    Event,
}

/// Where an event stands in a process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Done,
    /// The one in progress.
    Current,
    Upcoming,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub orientation: Orientation,
    pub dir: Dir,
    /// Whether the arrows come round from the last event to the first.
    pub wrap: bool,
    /// Whether the events are a feed that loads more as it is read. `PageDown` and
    /// `PageUp` then move between events from anywhere inside one.
    pub feed: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            orientation: Orientation::Vertical,
            dir: Dir::Ltr,
            wrap: true,
            feed: false,
        }
    }
}

/// Events in order:
///
/// the steps of a process, a history, or a feed of activity. The arrows move between
/// them. It has no value. The owner marks the step in progress with `set("current",
/// key)` and a feed that is loading with `set("busy", "true")`.
#[derive(Clone, Debug, Default)]
pub struct EventList {
    opts: Options,
    events: Items,
    current: Option<String>,
    busy: bool,
    focused: Option<String>,
}

impl EventList {
    #[must_use]
    pub fn new(opts: Options) -> Self {
        Self {
            opts,
            ..Default::default()
        }
    }

    #[must_use]
    pub const fn is_busy(&self) -> bool {
        self.busy
    }

    /// Whether an event is done, in progress or still to come. Every event is to
    /// come until one is marked as current.
    #[must_use]
    pub fn status(&self, key: &str) -> Status {
        let at = |key: &str| self.events.index_of(key);
        match (self.current.as_deref().and_then(at), at(key)) {
            (Some(current), Some(event)) if event < current => Status::Done,
            (Some(current), Some(event)) if event == current => Status::Current,
            _ => Status::Upcoming,
        }
    }

    fn tab_stop(&self) -> Option<&str> {
        let known = |key: &&str| self.events.get(key).is_some();
        self.focused
            .as_deref()
            .filter(known)
            .or_else(|| self.current.as_deref().filter(known))
            .or_else(|| self.events.first_enabled())
    }
}

impl PartOf for Part {
    type Primitive = EventList;
}

impl Primitive for EventList {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Event]
    }

    fn is_item(part: Part) -> bool {
        part == Part::Event
    }

    fn set(&mut self, name: &str, value: &str) -> bool {
        match name {
            "current" => self.current = (!value.is_empty()).then(|| value.to_string()),
            "busy" => self.busy = value == "true",
            _ => return false,
        }
        true
    }

    fn upsert_item(&mut self, item: Item) {
        self.events.upsert(item);
    }

    fn remove_item(&mut self, key: &str) {
        self.events.remove(key);
    }

    fn set_order(&mut self, rendered: &[(String, String)]) {
        self.events.set_order(rendered);
    }

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match (part, item) {
            (Part::Root, _) => Semantics {
                role: Some(if self.opts.feed {
                    Role::Feed
                } else {
                    Role::List
                }),
                orientation: Some(self.opts.orientation),
                busy: self.opts.feed && self.busy,
                ..Default::default()
            },
            (Part::Event, Some(key)) => Semantics {
                role: Some(if self.opts.feed {
                    Role::Article
                } else {
                    Role::ListItem
                }),
                current: self.status(key) == Status::Current,
                focusable: if self.tab_stop() == Some(key) {
                    Focusable::TabStop
                } else {
                    Focusable::Programmatic
                },
                active: self.focused.as_deref() == Some(key),
                ..Default::default()
            },
            (Part::Event, None) => Semantics {
                role: Some(Role::ListItem),
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, _: Gate) -> Reaction<Part> {
        if roving::is_platform_arrow(&input) {
            return Reaction::default();
        }
        match input {
            Input::Focus {
                part: Part::Event,
                item,
            } => {
                self.focused = item;
                Reaction::default()
            }
            Input::Key { key, .. } => {
                let nav = match key {
                    Key::PageDown if self.opts.feed => Some(Nav::Next),
                    Key::PageUp if self.opts.feed => Some(Nav::Prev),
                    Key::Home => Some(Nav::First),
                    Key::End => Some(Nav::Last),
                    other => roving::nav_for_key(other, self.opts.orientation, self.opts.dir),
                };
                let Some(nav) = nav else {
                    return Reaction::default();
                };
                // A feed has an end to reach, so paging through it never comes round.
                let wrap = self.opts.wrap && !matches!(key, Key::PageDown | Key::PageUp);
                let target = self.events.step(self.tab_stop(), nav, Ends::wrapping(wrap));
                if let Some(key) = &target {
                    self.focused = Some(key.clone());
                }
                Reaction::to_item(Part::Event, target)
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        Vec::new()
    }

    fn set_value(&mut self, _: Vec<String>) {}

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .events
            .iter()
            .map(|item| SnapshotItem {
                key: item.key.clone(),
                label: item.label.clone(),
                disabled: false,
                selected: false,
            })
            .collect();
        Snapshot {
            name: "event-list",
            label: label.to_string(),
            value: Vec::new(),
            expanded: None,
            actions: &[],
            items,
        }
    }

    fn act(&mut self, action: &str, _: Option<&str>, _: Gate) -> Result<(), ActError> {
        Err(ActError::UnknownAction {
            action: action.to_string(),
            valid: &[],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps(opts: Options) -> EventList {
        let mut list = EventList::new(opts);
        for key in ["ordered", "packed", "shipped", "delivered"] {
            list.upsert_item(Item {
                key: key.into(),
                label: key.into(),
                disabled: false,
            });
        }
        list
    }

    fn key(list: &mut EventList, key: Key) -> Reaction<Part> {
        crate::testing::press_key(list, key)
    }

    #[test]
    fn events_before_the_current_one_are_done_and_those_after_are_to_come() {
        let mut list = steps(Options::default());
        assert_eq!(
            list.status("ordered"),
            Status::Upcoming,
            "nothing is under way yet"
        );
        assert!(list.set("current", "packed"));
        assert_eq!(
            [
                list.status("ordered"),
                list.status("packed"),
                list.status("shipped")
            ],
            [Status::Done, Status::Current, Status::Upcoming]
        );
        assert!(list.semantics(Part::Event, Some("packed")).current);
        assert_eq!(
            list.semantics(Part::Event, Some("packed")).focusable,
            Focusable::TabStop,
            "the way in is the step in progress"
        );
    }

    #[test]
    fn arrows_move_between_events_and_come_round() {
        let mut list = steps(Options::default());
        assert_eq!(key(&mut list, Key::ArrowDown).focus_item(), Some("packed"));
        assert_eq!(key(&mut list, Key::ArrowUp).focus_item(), Some("ordered"));
        assert_eq!(key(&mut list, Key::ArrowUp).focus_item(), Some("delivered"));
        assert!(
            !key(&mut list, Key::PageDown).handled,
            "paging is for feeds"
        );
    }

    #[test]
    fn a_feed_pages_between_articles_and_says_when_it_is_busy() {
        let mut feed = steps(Options {
            feed: true,
            ..Default::default()
        });
        assert_eq!(feed.semantics(Part::Root, None).role, Some(Role::Feed));
        assert_eq!(
            feed.semantics(Part::Event, Some("packed")).role,
            Some(Role::Article)
        );
        assert_eq!(key(&mut feed, Key::PageDown).focus_item(), Some("packed"));
        key(&mut feed, Key::End);
        assert_eq!(
            key(&mut feed, Key::PageDown).focus_item(),
            None,
            "the end of a feed is the end"
        );
        feed.set("busy", "true");
        assert!(feed.semantics(Part::Root, None).busy);
    }

    #[test]
    fn it_conforms() {
        let items: Vec<Item> = ["a", "b", "c"]
            .iter()
            .map(|k| Item {
                key: k.to_string(),
                label: k.to_string(),
                disabled: false,
            })
            .collect();
        crate::testing::assert_conforms(EventList::default, &items);
        crate::testing::assert_conforms(
            || {
                EventList::new(Options {
                    feed: true,
                    ..Default::default()
                })
            },
            &items,
        );
    }
}
