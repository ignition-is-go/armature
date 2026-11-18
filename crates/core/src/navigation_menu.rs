use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::{Dir, Key, Orientation};
use crate::popup::{Align, Placement, Side};
use crate::primitive::{
    Effect, Focusable, Input, Item, PartOf, Primitive, Reaction, Relations, Role, Semantics,
};
use crate::roving;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    /// Opens the panel with the same key.
    Trigger,
    /// A plain link among the triggers, keyed like them.
    Link,
    /// The panel a trigger opens, keyed like its trigger and placed against it.
    Panel,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Whether resting the pointer on a trigger opens its panel. A press always does.
    pub hover: bool,
    pub open_delay_ms: u64,
    pub close_delay_ms: u64,
    pub orientation: Orientation,
    pub dir: Dir,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            hover: true,
            open_delay_ms: 50,
            close_delay_ms: 50,
            orientation: Orientation::Horizontal,
            dir: Dir::Ltr,
        }
    }
}

const ACTIONS: &[&str] = &["open", "close", "follow"];

/// A site's main navigation: links, and triggers that open panels of more links.
/// The value is the key of the open panel. Following a link offers `link:<key>` to
/// the gate.
#[derive(Clone, Debug, Default)]
pub struct NavigationMenu {
    opts: Options,
    /// The top-level entries in order, and whether each opens a panel.
    entries: Vec<(String, bool)>,
    labels: Vec<(String, String)>,
    /// The link for the page being shown.
    current: Option<String>,
    open: Option<String>,
    focused: Option<String>,
    /// A panel to open or close once the pointer has rested, and when.
    due: Option<(u64, Option<String>)>,
}

impl NavigationMenu {
    /// `entries` are the top-level keys in order, with whether each opens a panel.
    #[must_use]
    pub fn new(opts: Options, entries: &[(&str, bool)]) -> Self {
        let entries = entries
            .iter()
            .map(|(key, panel)| (key.to_string(), *panel))
            .collect();
        Self {
            opts,
            entries,
            ..Default::default()
        }
    }

    #[must_use]
    pub fn current(mut self, key: &str) -> Self {
        self.current = Some(key.to_string());
        self
    }

    #[must_use]
    pub fn open_panel(&self) -> Option<&str> {
        self.open.as_deref()
    }

    fn has_panel(&self, key: &str) -> bool {
        self.entries
            .iter()
            .any(|(entry, panel)| entry == key && *panel)
    }

    fn set(&mut self, open: Option<String>, source: Source, reason: Reason, gate: Gate) -> bool {
        self.due = None;
        if open == self.open {
            return true;
        }
        let allowed = gate(&ValueChange {
            value: open.iter().cloned().collect(),
            source,
            reason,
        });
        if allowed {
            self.open = open;
        }
        allowed
    }

    fn tab_stop(&self) -> Option<&str> {
        self.focused
            .as_deref()
            .or(self.entries.first().map(|(key, _)| key.as_str()))
    }

    fn part_of(&self, key: &str) -> Part {
        if self.has_panel(key) {
            Part::Trigger
        } else {
            Part::Link
        }
    }
}

impl PartOf for Part {
    type Primitive = NavigationMenu;
}

impl Primitive for NavigationMenu {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Trigger, Part::Link, Part::Panel]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn hovers(part: Part) -> bool {
        matches!(part, Part::Trigger | Part::Panel)
    }

    fn popup_of(part: Part) -> Option<Placement<Part>> {
        (part == Part::Panel).then_some(Placement {
            anchor: Part::Trigger,
            side: Side::Bottom,
            align: Align::Start,
            gap: 4.0,
        })
    }

    fn relations(part: Part) -> Relations<Part> {
        match part {
            Part::Trigger => Relations {
                controls: Some(Part::Panel),
                ..Default::default()
            },
            Part::Panel => Relations {
                labelled_by: Some(Part::Trigger),
                ..Default::default()
            },
            _ => Relations::default(),
        }
    }

    fn upsert_item(&mut self, item: Item) {
        self.labels.retain(|(key, _)| *key != item.key);
        self.labels.push((item.key, item.label));
    }

    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        let stop = |key: &str| {
            if self.tab_stop() == Some(key) {
                Focusable::TabStop
            } else {
                Focusable::Programmatic
            }
        };
        match (part, item) {
            (Part::Root, _) => Semantics {
                role: Some(Role::Navigation),
                orientation: Some(self.opts.orientation),
                ..Default::default()
            },
            (Part::Trigger, Some(key)) => Semantics {
                role: Some(Role::Button),
                expanded: Some(self.open.as_deref() == Some(key)),
                focusable: stop(key),
                active: self.open.as_deref() == Some(key),
                ..Default::default()
            },
            (Part::Link, Some(key)) => Semantics {
                role: Some(Role::Link),
                current: self.current.as_deref() == Some(key),
                focusable: stop(key),
                ..Default::default()
            },
            (Part::Panel, Some(key)) => Semantics {
                role: Some(Role::Group),
                hidden: self.open.as_deref() != Some(key),
                ..Default::default()
            },
            (Part::Panel, None) => Semantics {
                hidden: true,
                ..Default::default()
            },
            (other, None) => Semantics {
                role: Some(if other == Part::Link {
                    Role::Link
                } else {
                    Role::Button
                }),
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        if roving::is_platform_arrow(&input) {
            return Reaction::default();
        }
        match input {
            Input::Focus {
                part: Part::Trigger | Part::Link,
                item,
            } => {
                self.focused = item;
                Reaction::default()
            }
            Input::Press {
                part: Part::Trigger,
                item: Some(key),
                ..
            } => {
                let next = (self.open.as_deref() != Some(key.as_str())).then_some(key);
                self.set(next, Source::User, Reason::Press, gate);
                Reaction::handled()
            }
            Input::Press {
                part: Part::Link,
                item: Some(key),
                ..
            } => {
                // Following a link closes whatever is open. The platform follows it.
                gate(&ValueChange {
                    value: vec![format!("link:{key}")],
                    source: Source::User,
                    reason: Reason::Press,
                });
                self.open = None;
                Reaction::default()
            }
            Input::Hover {
                part,
                item: Some(key),
                inside,
                now_ms,
            } if self.opts.hover => {
                let wanted = if inside { Some(key) } else { None };
                // Moving to another trigger while a panel is open switches at once.
                if inside && part == Part::Trigger && self.open.is_some() {
                    self.set(wanted, Source::User, Reason::Press, gate);
                    return Reaction::default();
                }
                if wanted == self.open {
                    self.due = None;
                    return Reaction::default();
                }
                let delay = if inside {
                    self.opts.open_delay_ms
                } else {
                    self.opts.close_delay_ms
                };
                let at_ms = now_ms.saturating_add(delay);
                self.due = Some((at_ms, wanted));
                Reaction::default().with(Effect::Wake { at_ms })
            }
            Input::Tick { now_ms } => {
                if let Some((_, wanted)) = self.due.clone().filter(|(at_ms, _)| now_ms >= *at_ms) {
                    self.set(wanted, Source::User, Reason::Press, gate);
                }
                Reaction::default()
            }
            Input::Key {
                key: Key::Escape, ..
            } if self.open.is_some() => {
                let trigger = self.open.clone();
                self.set(None, Source::User, Reason::Keyboard, gate);
                Reaction::item(Part::Trigger, true, trigger)
            }
            Input::Key {
                key: Key::Enter | Key::Space,
                mods,
                ..
            } => match self.focused.clone() {
                Some(key) if self.has_panel(&key) => self.send(
                    Input::Press {
                        part: Part::Trigger,
                        item: Some(key),
                        mods,
                    },
                    gate,
                ),
                _ => Reaction::default(),
            },
            // The down arrow opens the focused trigger's panel, as in a menu.
            Input::Key {
                key: Key::ArrowDown,
                ..
            } if self.opts.orientation == Orientation::Horizontal
                && self
                    .focused
                    .as_deref()
                    .is_some_and(|key| self.has_panel(key)) =>
            {
                self.set(self.focused.clone(), Source::User, Reason::Keyboard, gate);
                Reaction::handled()
            }
            Input::Key { key, .. } => {
                let Some(nav) = roving::nav_for_key(key, self.opts.orientation, self.opts.dir)
                else {
                    return Reaction::default();
                };
                let at = self
                    .tab_stop()
                    .and_then(|key| self.entries.iter().position(|(entry, _)| entry == key));
                let Some(next) = roving::step(&vec![true; self.entries.len()], at, nav, true)
                else {
                    return Reaction::default();
                };
                let key = self.entries[next].0.clone();
                self.focused = Some(key.clone());
                Reaction::item(self.part_of(&key), true, Some(key))
            }
            Input::PressOutside | Input::WindowBlur | Input::Blur { .. } => {
                self.set(None, Source::User, Reason::Press, gate);
                Reaction::default()
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        self.open.iter().cloned().collect()
    }

    fn set_value(&mut self, value: Vec<String>) {
        self.open = value.into_iter().find(|key| self.has_panel(key));
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .entries
            .iter()
            .map(|(key, _)| SnapshotItem {
                label: self
                    .labels
                    .iter()
                    .find(|(k, _)| k == key)
                    .map_or_else(|| key.clone(), |(_, label)| label.clone()),
                disabled: false,
                selected: self.open.as_deref() == Some(key.as_str()),
                key: key.clone(),
            })
            .collect();
        Snapshot {
            name: "navigation-menu",
            label: label.to_string(),
            value: self.value(),
            expanded: Some(self.open.is_some()),
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let action: &'static str = match action {
            "close" => {
                return self
                    .set(None, Source::Agent, Reason::Action, gate)
                    .then_some(())
                    .ok_or(ActError::Refused);
            }
            "open" => "open",
            "follow" => "follow",
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        let key = item.ok_or(ActError::MissingItem { action })?;
        if !self.entries.iter().any(|(entry, _)| entry == key) {
            return Err(ActError::UnknownItem(key.to_string()));
        }
        let done = match (action, self.has_panel(key)) {
            ("open", true) => self.set(Some(key.to_string()), Source::Agent, Reason::Action, gate),
            ("follow", false) => gate(&ValueChange {
                value: vec![format!("link:{key}")],
                source: Source::Agent,
                reason: Reason::Action,
            }),
            _ => false,
        };
        if done { Ok(()) } else { Err(ActError::Refused) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Modifiers;

    fn nav() -> NavigationMenu {
        NavigationMenu::new(
            Options::default(),
            &[("products", true), ("docs", true), ("pricing", false)],
        )
        .current("pricing")
    }

    fn hover(nav: &mut NavigationMenu, part: Part, key: &str, inside: bool, now_ms: u64) {
        nav.send(
            Input::Hover {
                part,
                item: Some(key.into()),
                inside,
                now_ms,
            },
            &mut |_| true,
        );
        nav.send(Input::Tick { now_ms }, &mut |_| true);
    }

    #[test]
    fn resting_on_a_trigger_opens_its_panel_and_the_next_trigger_switches_at_once() {
        let mut nav = nav();
        hover(&mut nav, Part::Trigger, "products", true, 0);
        assert_eq!(nav.open_panel(), None);
        nav.send(Input::Tick { now_ms: 50 }, &mut |_| true);
        assert_eq!(nav.open_panel(), Some("products"));
        hover(&mut nav, Part::Trigger, "products", false, 60);
        hover(&mut nav, Part::Trigger, "docs", true, 70);
        assert_eq!(nav.open_panel(), Some("docs"));
        assert!(nav.semantics(Part::Panel, Some("products")).hidden);
    }

    #[test]
    fn moving_from_the_trigger_into_its_panel_keeps_it_open() {
        let mut nav = nav();
        nav.act("open", Some("docs"), &mut |_| true).unwrap();
        hover(&mut nav, Part::Trigger, "docs", false, 0);
        hover(&mut nav, Part::Panel, "docs", true, 20);
        nav.send(Input::Tick { now_ms: 200 }, &mut |_| true);
        assert_eq!(nav.open_panel(), Some("docs"));
        hover(&mut nav, Part::Panel, "docs", false, 300);
        nav.send(Input::Tick { now_ms: 350 }, &mut |_| true);
        assert_eq!(nav.open_panel(), None);
    }

    #[test]
    fn arrows_move_along_the_bar_and_down_opens_a_panel() {
        let mut nav = nav();
        let key = |nav: &mut NavigationMenu, key| {
            nav.send(
                Input::Key {
                    key,
                    mods: Modifiers::default(),
                    now_ms: 0,
                },
                &mut |_| true,
            )
        };
        assert_eq!(key(&mut nav, Key::ArrowRight).focus_item(), Some("docs"));
        key(&mut nav, Key::ArrowDown);
        assert_eq!(nav.open_panel(), Some("docs"));
        let closed = key(&mut nav, Key::Escape);
        assert_eq!(
            (nav.open_panel(), closed.focus_item()),
            (None, Some("docs"))
        );
        let link = key(&mut nav, Key::ArrowRight);
        assert_eq!(link.focus.unwrap().part, Part::Link);
        assert!(nav.semantics(Part::Link, Some("pricing")).current);
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(nav, &[]);
    }
}
