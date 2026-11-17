use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::items::Items;
use crate::keys::Key;
use crate::popup::{Align, Placement, Side};
use crate::primitive::{
    Focusable, Input, Item, PartOf, Primitive, Reaction, Relations, Role, Semantics,
};
use crate::roving::{self, Nav};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The text field. Focus stays here while the arrows move through the options.
    Input,
    /// The popup of options.
    List,
    Option,
    /// One chosen option, when several can be chosen. Pressing it removes it.
    Chip,
    /// Empties the field and the choice.
    Clear,
    /// Shown in the list when nothing matches.
    Empty,
    /// Shown in the list while results are loading, or when loading failed.
    Status,
}

/// Where results that come from elsewhere are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Loading {
    #[default]
    Idle,
    Loading,
    Error,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    pub multiple: bool,
    /// Whether text that matches no option is a value in its own right, as in a
    /// search box with suggestions. Otherwise only an option can be the value.
    pub custom: bool,
    /// Whether the first match is highlighted as soon as there is one, so Enter
    /// takes it.
    pub auto_highlight: bool,
    /// The most options that may be chosen at once, where several can be.
    pub max_selected: Option<usize>,
    /// Whether the list opens as soon as the field takes focus.
    pub open_on_focus: bool,
    /// Whether reaching the last option asks the owner for more, by offering
    /// `more:<text>` to the gate.
    pub load_more: bool,
    /// Whether the options come from elsewhere as the person types. Each change
    /// of text then offers `load:<request>:<text>` to the gate, and the owner
    /// answers with `set("loaded", request)` or `set("failed", request)`. An
    /// answer to a request that has since been replaced is ignored.
    pub remote: bool,
    /// Shown but not to be changed: typing, choosing and clearing are ignored.
    pub read_only: bool,
}

const ACTIONS: &[&str] = &["select", "remove", "clear", "type", "open", "close"];

/// A text field with a list of options that narrows as the person types. With
/// `custom` it is an autocomplete: the text itself is the value. The value is the
/// chosen keys, or the text.
#[derive(Clone, Debug, Default)]
pub struct Combobox {
    opts: Options,
    items: Items,
    text: String,
    open: bool,
    selected: Vec<String>,
    active: Option<String>,
    on_clear: bool,
    loading: Loading,
    /// Whether the highlight has just reached the last option.
    at_end: bool,
    requests: crate::async_options::Requests,
    /// The value that deleting the text took away, which Escape brings back.
    deleted: Option<String>,
}

impl Combobox {
    /// Whether results are loading or failed. The owner says so with
    /// `set("status", "loading")`, `"error"` or `"idle"`.
    pub fn loading(&self) -> Loading {
        self.loading
    }

    pub fn new(opts: Options) -> Self {
        Combobox {
            opts,
            ..Default::default()
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    fn matches(&self, item: &Item) -> bool {
        // Text that is just the chosen option's label narrows nothing.
        let shows_choice = !self.opts.multiple
            && self
                .selected
                .first()
                .and_then(|key| self.items.get(key))
                .is_some_and(|chosen| chosen.label == self.text);
        shows_choice
            || item
                .label
                .to_lowercase()
                .contains(&self.text.trim().to_lowercase())
    }

    /// The rest of the first option that begins with what was typed, to show after
    /// the caret as a suggestion. Tab takes it.
    pub fn completion(&self) -> Option<String> {
        let typed = self.text.to_lowercase();
        if typed.is_empty() {
            return None;
        }
        let first = self
            .items
            .iter()
            .find(|item| !item.disabled && item.label.to_lowercase().starts_with(&typed))?;
        let rest: String = first
            .label
            .chars()
            .skip(self.text.chars().count())
            .collect();
        (!rest.is_empty()).then_some(rest)
    }

    /// The options on show, in order.
    pub fn results(&self) -> Vec<&str> {
        self.items
            .iter()
            .filter(|item| self.matches(item))
            .map(|item| item.key.as_str())
            .collect()
    }

    fn label(&self, key: &str) -> String {
        self.items
            .get(key)
            .map_or_else(|| key.to_string(), |item| item.label.clone())
    }

    fn commit(&mut self, next: Vec<String>, source: Source, reason: Reason, gate: Gate) -> bool {
        if next == self.selected {
            self.deleted = None;
            return true;
        }
        let allowed = gate(&ValueChange {
            value: next.clone(),
            source,
            reason,
        });
        if allowed {
            self.selected = next;
            self.deleted = None;
        }
        allowed
    }

    fn choose(&mut self, key: &str, source: Source, reason: Reason, gate: Gate) -> bool {
        if !self.items.is_enabled(key) {
            return false;
        }
        if self.opts.multiple {
            let mut next = self.selected.clone();
            match next.iter().position(|chosen| chosen == key) {
                Some(at) => drop(next.remove(at)),
                // Full, so nothing more is taken until one is let go.
                None if self
                    .opts
                    .max_selected
                    .is_some_and(|most| next.len() >= most) =>
                {
                    return false;
                }
                None => next.push(key.to_string()),
            }
            let done = self.commit(next, source, reason, gate);
            if done {
                self.text.clear();
            }
            return done;
        }
        let value = if self.opts.custom {
            self.label(key)
        } else {
            key.to_string()
        };
        let done = self.commit(vec![value], source, reason, gate);
        if done {
            self.text = self.label(key);
            self.open = false;
            self.active = None;
        }
        done
    }

    /// Leaves the field: text that names no option goes back to the chosen one, or
    /// becomes the value where free text is allowed.
    fn settle(&mut self, gate: Gate) {
        self.open = false;
        self.active = None;
        if self.opts.multiple {
            self.text.clear();
        } else if self.opts.custom {
            let next = if self.text.is_empty() {
                Vec::new()
            } else {
                vec![self.text.clone()]
            };
            self.commit(next, Source::User, Reason::Text, gate);
        } else {
            self.text = self
                .selected
                .first()
                .map(|key| self.label(key))
                .unwrap_or_default();
        }
    }

    fn step(&mut self, nav: Nav) -> Reaction<Part> {
        self.open = true;
        let results: Vec<String> = self.results().into_iter().map(str::to_string).collect();
        let enabled: Vec<bool> = results
            .iter()
            .map(|key| self.items.is_enabled(key))
            .collect();
        let at = self
            .active
            .as_deref()
            .and_then(|key| results.iter().position(|result| result == key));
        if let Some(next) = roving::step(&enabled, at, nav, true) {
            self.active = Some(results[next].clone());
            self.at_end = next + 1 == results.len();
        }
        Reaction::handled()
    }
}

impl PartOf for Part {
    type Primitive = Combobox;
}

impl Primitive for Combobox {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[
            Part::Input,
            Part::List,
            Part::Option,
            Part::Chip,
            Part::Clear,
            Part::Empty,
            Part::Status,
        ]
    }

    fn is_item(part: Part) -> bool {
        part == Part::Option
    }

    fn popup_of(part: Part) -> Option<Placement<Part>> {
        (part == Part::List).then_some(Placement {
            anchor: Part::Input,
            side: Side::Bottom,
            align: Align::Start,
            gap: 4.0,
        })
    }

    fn relations(part: Part) -> Relations<Part> {
        match part {
            Part::Input => Relations {
                controls: Some(Part::List),
                ..Default::default()
            },
            _ => Relations::default(),
        }
    }

    fn named_part(&self) -> Option<Part> {
        Some(Part::Input)
    }

    fn set(&mut self, name: &str, value: &str) -> bool {
        if let ("loaded" | "failed", Ok(request)) = (name, value.parse::<u64>()) {
            let newest = self.requests.finish(request, name == "loaded") || name == "failed";
            self.loading = match self.requests.status() {
                crate::async_options::Status::Idle => Loading::Idle,
                crate::async_options::Status::Loading => Loading::Loading,
                crate::async_options::Status::Error => Loading::Error,
            };
            return newest;
        }
        if name != "status" {
            return false;
        }
        self.loading = match value {
            "loading" => Loading::Loading,
            "error" => Loading::Error,
            _ => Loading::Idle,
        };
        true
    }

    fn upsert_item(&mut self, item: Item) {
        self.items.upsert(item);
    }

    fn remove_item(&mut self, key: &str) {
        self.items.remove(key);
        if !self.opts.custom {
            self.selected.retain(|chosen| chosen != key);
        }
    }

    fn set_order(&mut self, rendered: &[(String, String)]) {
        self.items.set_order(rendered);
    }

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        let nothing = self.results().is_empty();
        match (part, item) {
            (Part::Input, _) => Semantics {
                role: Some(Role::TextBox),
                expanded: Some(self.open),
                focusable: Focusable::TabStop,
                ..Default::default()
            },
            (Part::List, _) => Semantics {
                role: Some(Role::Listbox),
                multiselectable: Some(self.opts.multiple),
                hidden: !self.open,
                ..Default::default()
            },
            (Part::Option, Some(key)) => {
                let shown = self.open && self.items.get(key).is_some_and(|item| self.matches(item));
                Semantics {
                    role: Some(Role::Option),
                    selected: Some(self.selected.iter().any(|chosen| chosen == key)),
                    disabled: !self.items.is_enabled(key),
                    hidden: !shown,
                    active: shown && self.active.as_deref() == Some(key),
                    ..Default::default()
                }
            }
            (Part::Option, None) => Semantics {
                role: Some(Role::Option),
                ..Default::default()
            },
            (Part::Chip, Some(key)) => Semantics {
                role: Some(Role::Button),
                hidden: !self.opts.multiple || !self.selected.iter().any(|chosen| chosen == key),
                ..Default::default()
            },
            (Part::Chip, None) => Semantics {
                hidden: true,
                ..Default::default()
            },
            (Part::Clear, _) => {
                let hidden = self.text.is_empty() && self.selected.is_empty();
                Semantics {
                    role: Some(Role::Button),
                    hidden,
                    focusable: if hidden {
                        Focusable::No
                    } else {
                        Focusable::TabStop
                    },
                    ..Default::default()
                }
            }
            (Part::Status, _) => Semantics {
                role: Some(if self.loading == Loading::Error {
                    Role::Alert
                } else {
                    Role::Status
                }),
                hidden: !self.open || self.loading == Loading::Idle,
                ..Default::default()
            },
            // Nothing is said to be missing while results may still arrive.
            (Part::Empty, _) => Semantics {
                role: Some(Role::Status),
                hidden: !self.open || !nothing || self.loading != Loading::Idle,
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        if self.opts.read_only && !matches!(input, Input::Focus { .. } | Input::Blur { .. }) {
            return Reaction::default();
        }
        match input {
            Input::Focus { part, .. } => {
                self.on_clear = part == Part::Clear;
                if part == Part::Input && self.opts.open_on_focus {
                    self.open = true;
                }
                Reaction::default()
            }
            Input::Text {
                part: Part::Input,
                value,
                ..
            } => {
                // Deleting all the text of a single choice clears it. Escape can
                // still bring it back.
                if value.is_empty() && !self.opts.multiple && !self.opts.custom {
                    let was = self.selected.first().cloned();
                    if was.is_some() && self.commit(Vec::new(), Source::User, Reason::Text, gate) {
                        self.deleted = was;
                    }
                }
                self.text = value;
                self.open = true;
                if self.opts.remote {
                    match self.requests.begin(&self.text) {
                        Some(request) => {
                            self.loading = Loading::Loading;
                            gate(&ValueChange {
                                value: vec![format!("load:{request}:{}", self.text)],
                                source: Source::User,
                                reason: Reason::Action,
                            });
                        }
                        None => self.loading = Loading::Idle,
                    }
                }
                let results = self.results();
                let first = results
                    .iter()
                    .find(|key| self.items.is_enabled(key))
                    .map(|key| key.to_string());
                let message = match results.len() {
                    _ if self.loading == Loading::Loading => "Loading".to_string(),
                    0 => "No results".to_string(),
                    1 => "1 result".to_string(),
                    count => format!("{count} results"),
                };
                self.active = if self.opts.auto_highlight {
                    first
                } else {
                    None
                };
                Reaction::handled().announcing(message)
            }
            Input::Press {
                part: Part::Input, ..
            } => {
                self.open = true;
                Reaction::default()
            }
            Input::Press {
                part: Part::Option,
                item: Some(key),
                ..
            } => {
                self.choose(&key, Source::User, Reason::Press, gate);
                Reaction::focusing(Part::Input)
            }
            Input::Press {
                part: Part::Chip,
                item: Some(key),
                ..
            } => {
                let next = self
                    .selected
                    .iter()
                    .filter(|chosen| **chosen != key)
                    .cloned()
                    .collect();
                self.commit(next, Source::User, Reason::Press, gate);
                Reaction::focusing(Part::Input)
            }
            Input::Press {
                part: Part::Clear, ..
            } => {
                if self.commit(Vec::new(), Source::User, Reason::Press, gate) {
                    self.text.clear();
                }
                Reaction::focusing(Part::Input)
            }
            Input::Key {
                key: Key::Enter | Key::Space,
                mods,
                ..
            } if self.on_clear => self.send(
                Input::Press {
                    part: Part::Clear,
                    item: None,
                    mods,
                },
                gate,
            ),
            Input::Key {
                key: Key::ArrowDown,
                mods,
                ..
            } if !mods.alt || !self.open => {
                let reaction = self.step(Nav::Next);
                // The end of what is loaded is where more is asked for.
                if self.opts.load_more
                    && std::mem::take(&mut self.at_end)
                    && self.loading != Loading::Loading
                {
                    gate(&ValueChange {
                        value: vec![format!("more:{}", self.text)],
                        source: Source::User,
                        reason: Reason::Action,
                    });
                }
                reaction
            }
            // Tab takes the suggested completion, then moves on as usual.
            Input::Key { key: Key::Tab, .. } if self.open && self.completion().is_some() => {
                let typed = self.text.to_lowercase();
                let first = self
                    .items
                    .iter()
                    .find(|item| !item.disabled && item.label.to_lowercase().starts_with(&typed))
                    .map(|item| item.key.clone());
                if let Some(key) = first {
                    self.choose(&key, Source::User, Reason::Keyboard, gate);
                }
                Reaction::default()
            }
            Input::Key {
                key: Key::ArrowUp, ..
            } => self.step(Nav::Prev),
            Input::Key {
                key: Key::Enter, ..
            } if self.open => match self.active.clone() {
                Some(key) => {
                    self.choose(&key, Source::User, Reason::Keyboard, gate);
                    Reaction::handled()
                }
                None if self.opts.custom => {
                    self.settle(gate);
                    Reaction::handled()
                }
                None => Reaction::default(),
            },
            // Escape brings back a value that deleting the text took away.
            Input::Key {
                key: Key::Escape, ..
            } if self.deleted.is_some() => {
                let back = self.deleted.take().unwrap_or_default();
                if self.commit(vec![back.clone()], Source::User, Reason::Keyboard, gate) {
                    self.text = self.label(&back);
                }
                self.open = false;
                self.active = None;
                Reaction::handled()
            }
            // Escape closes the list first, and only then clears the field.
            Input::Key {
                key: Key::Escape, ..
            } if self.open => {
                self.open = false;
                self.active = None;
                Reaction::handled()
            }
            Input::Key {
                key: Key::Escape, ..
            } if !self.text.is_empty() => {
                if self.opts.multiple
                    || self.commit(Vec::new(), Source::User, Reason::Keyboard, gate)
                {
                    self.text.clear();
                }
                Reaction::handled()
            }
            Input::Key {
                key: Key::Backspace,
                ..
            } if self.opts.multiple && self.text.is_empty() && !self.selected.is_empty() => {
                let mut next = self.selected.clone();
                next.pop();
                self.commit(next, Source::User, Reason::Keyboard, gate);
                Reaction::handled()
            }
            Input::PressOutside | Input::WindowBlur | Input::Blur { .. } => {
                self.settle(gate);
                Reaction::default()
            }
            _ => Reaction::default(),
        }
    }

    fn text(&self, part: Part, _: Option<&str>) -> Option<String> {
        (part == Part::Input).then(|| self.text.clone())
    }

    fn value(&self) -> Vec<String> {
        self.selected.clone()
    }

    fn set_value(&mut self, value: Vec<String>) {
        let known = |key: &String| self.opts.custom || self.items.get(key).is_some();
        let mut next: Vec<String> = value.into_iter().filter(known).collect();
        if !self.opts.multiple {
            next.truncate(1);
            self.text = next.first().map(|key| self.label(key)).unwrap_or_default();
        }
        self.selected = next;
        self.deleted = None;
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .items
            .iter()
            .map(|item| SnapshotItem {
                key: item.key.clone(),
                label: item.label.clone(),
                disabled: item.disabled,
                selected: self.selected.contains(&item.key),
            })
            .collect();
        Snapshot {
            name: "combobox",
            label: label.to_string(),
            value: self.value(),
            expanded: Some(self.open),
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let action: &'static str = match action {
            "open" | "close" => {
                self.open = action == "open";
                return Ok(());
            }
            "clear" => {
                if self.opts.read_only
                    || !self.commit(Vec::new(), Source::Agent, Reason::Action, gate)
                {
                    return Err(ActError::Refused);
                }
                self.text.clear();
                return Ok(());
            }
            "type" => {
                self.text = item
                    .ok_or(ActError::MissingItem { action: "type" })?
                    .to_string();
                self.open = true;
                return Ok(());
            }
            "select" => "select",
            "remove" => "remove",
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        let key = item.ok_or(ActError::MissingItem { action })?;
        match self.items.get(key).map(|item| item.disabled) {
            None => return Err(ActError::UnknownItem(key.to_string())),
            Some(true) => return Err(ActError::Disabled(key.to_string())),
            Some(false) => {}
        }
        let chosen = self.selected.iter().any(|c| c == key);
        let done = match (action, chosen) {
            ("remove", false) => true,
            ("remove", true) => {
                let next = self
                    .selected
                    .iter()
                    .filter(|c| *c != key)
                    .cloned()
                    .collect();
                self.commit(next, Source::Agent, Reason::Action, gate)
            }
            (_, true) if self.opts.multiple => true,
            _ => self.choose(key, Source::Agent, Reason::Action, gate),
        };
        if done { Ok(()) } else { Err(ActError::Refused) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Modifiers;

    fn fruit(opts: Options) -> Combobox {
        let mut combobox = Combobox::new(opts);
        for (key, label) in [
            ("apple", "Apple"),
            ("apricot", "Apricot"),
            ("banana", "Banana"),
            ("cherry", "Cherry"),
        ] {
            combobox.upsert_item(Item {
                key: key.into(),
                label: label.into(),
                disabled: key == "apricot",
            });
        }
        combobox
    }

    fn typed(combobox: &mut Combobox, text: &str) -> Reaction<Part> {
        combobox.send(
            Input::Text {
                part: Part::Input,
                item: None,
                value: text.into(),
            },
            &mut |_| true,
        )
    }

    fn key(combobox: &mut Combobox, key: Key) -> Reaction<Part> {
        combobox.send(
            Input::Key {
                key,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        )
    }

    #[test]
    fn typing_narrows_the_list_says_how_many_and_enter_takes_the_highlight() {
        let mut combobox = fruit(Options {
            auto_highlight: true,
            ..Default::default()
        });
        let said = typed(&mut combobox, "ap");
        assert_eq!(combobox.results(), ["apple", "apricot"]);
        assert_eq!(
            said.effects,
            [crate::primitive::Effect::Announce {
                message: "2 results".into(),
                assertive: false
            }]
        );
        assert!(combobox.semantics(Part::Option, Some("banana")).hidden);
        key(&mut combobox, Key::Enter);
        assert_eq!(combobox.value(), ["apple"]);
        assert_eq!(combobox.text(Part::Input, None).as_deref(), Some("Apple"));
        assert!(!combobox.is_open());
    }

    #[test]
    fn text_that_names_no_option_goes_back_to_the_choice_when_the_field_is_left() {
        let mut combobox = fruit(Options::default());
        combobox
            .act("select", Some("banana"), &mut |_| true)
            .unwrap();
        typed(&mut combobox, "zzz");
        assert!(!combobox.semantics(Part::Empty, None).hidden);
        combobox.send(
            Input::Blur {
                part: Part::Input,
                item: None,
            },
            &mut |_| true,
        );
        assert_eq!(combobox.text(Part::Input, None).as_deref(), Some("Banana"));
        assert_eq!(combobox.value(), ["banana"]);
    }

    #[test]
    fn free_text_is_the_value_where_it_is_allowed() {
        let mut search = fruit(Options {
            custom: true,
            ..Default::default()
        });
        typed(&mut search, "dragonfruit");
        key(&mut search, Key::Enter);
        assert_eq!(search.value(), ["dragonfruit"]);
        typed(&mut search, "ch");
        key(&mut search, Key::ArrowDown);
        key(&mut search, Key::Enter);
        assert_eq!(search.value(), ["Cherry"], "a suggestion fills in its text");
    }

    #[test]
    fn several_can_be_chosen_and_backspace_removes_the_last() {
        let mut combobox = fruit(Options {
            multiple: true,
            ..Default::default()
        });
        typed(&mut combobox, "b");
        key(&mut combobox, Key::ArrowDown);
        key(&mut combobox, Key::Enter);
        key(&mut combobox, Key::ArrowDown);
        key(&mut combobox, Key::Enter);
        assert_eq!(combobox.value(), ["banana", "cherry"]);
        assert!(combobox.is_open(), "the list stays for the next choice");
        assert!(!combobox.semantics(Part::Chip, Some("banana")).hidden);
        key(&mut combobox, Key::Backspace);
        assert_eq!(combobox.value(), ["banana"]);
    }

    #[test]
    fn escape_closes_the_list_before_it_clears_the_field() {
        let mut combobox = fruit(Options::default());
        typed(&mut combobox, "ch");
        key(&mut combobox, Key::Escape);
        assert!(!combobox.is_open());
        assert_eq!(combobox.text(Part::Input, None).as_deref(), Some("ch"));
        key(&mut combobox, Key::Escape);
        assert_eq!(combobox.text(Part::Input, None).as_deref(), Some(""));
    }

    #[test]
    fn it_conforms() {
        let items: Vec<Item> = ["a", "b", "c"]
            .iter()
            .map(|k| Item {
                key: k.to_string(),
                label: k.to_string(),
                disabled: *k == "b",
            })
            .collect();
        crate::testing::assert_conforms(Combobox::default, &items);
        crate::testing::assert_conforms(
            || {
                Combobox::new(Options {
                    multiple: true,
                    ..Default::default()
                })
            },
            &items,
        );
        crate::testing::assert_conforms(
            || {
                Combobox::new(Options {
                    custom: true,
                    ..Default::default()
                })
            },
            &items,
        );
    }

    #[test]
    fn while_results_load_it_says_so_and_not_that_nothing_matches() {
        let mut combobox = fruit(Options::default());
        assert!(combobox.set("status", "loading"));
        let said = typed(&mut combobox, "zzz");
        assert_eq!(
            said.effects,
            [crate::primitive::Effect::Announce {
                message: "Loading".into(),
                assertive: false
            }]
        );
        assert!(combobox.semantics(Part::Empty, None).hidden);
        assert!(!combobox.semantics(Part::Status, None).hidden);
        combobox.set("status", "error");
        assert_eq!(
            combobox.semantics(Part::Status, None).role,
            Some(Role::Alert)
        );
        combobox.set("status", "idle");
        assert!(!combobox.semantics(Part::Empty, None).hidden);
        assert!(!combobox.set("colour", "red"), "not a setting it has");
    }

    #[test]
    fn no_more_than_the_limit_can_be_chosen_and_focus_can_open_the_list() {
        let mut combobox = fruit(Options {
            multiple: true,
            max_selected: Some(2),
            open_on_focus: true,
            ..Default::default()
        });
        combobox.send(
            Input::Focus {
                part: Part::Input,
                item: None,
            },
            &mut |_| true,
        );
        assert!(combobox.is_open());
        for key in ["apple", "banana", "cherry"] {
            combobox.send(
                Input::Press {
                    part: Part::Option,
                    item: Some(key.into()),
                    mods: Modifiers::default(),
                },
                &mut |_| true,
            );
        }
        assert_eq!(combobox.value(), ["apple", "banana"]);
    }

    #[test]
    fn the_rest_of_the_first_match_is_suggested_and_tab_takes_it() {
        let mut combobox = fruit(Options::default());
        typed(&mut combobox, "ba");
        assert_eq!(combobox.completion().as_deref(), Some("nana"));
        let tab = key(&mut combobox, Key::Tab);
        assert_eq!(combobox.value(), ["banana"]);
        assert!(!tab.handled, "focus still moves on");
        typed(&mut combobox, "ap");
        assert_eq!(
            combobox.completion().as_deref(),
            Some("ple"),
            "the disabled apricot is passed over"
        );
    }

    #[test]
    fn reaching_the_last_option_asks_for_more() {
        let mut combobox = fruit(Options {
            load_more: true,
            ..Default::default()
        });
        typed(&mut combobox, "an");
        let mut asked = Vec::new();
        let mut down = |combobox: &mut Combobox| {
            combobox.send(
                Input::Key {
                    key: Key::ArrowDown,
                    mods: Modifiers::default(),
                    now_ms: 0,
                },
                &mut |change| {
                    asked.extend(change.value.iter().cloned());
                    true
                },
            );
        };
        down(&mut combobox);
        assert_eq!(
            asked,
            ["more:an"],
            "banana is the only match, so the end is reached at once"
        );
    }

    #[test]
    fn a_slow_answer_to_an_old_search_is_ignored() {
        let mut combobox = fruit(Options {
            remote: true,
            ..Default::default()
        });
        let asked = std::cell::RefCell::new(Vec::new());
        let search = |combobox: &mut Combobox, text: &str| {
            combobox.send(
                Input::Text {
                    part: Part::Input,
                    item: None,
                    value: text.into(),
                },
                &mut |change| {
                    asked.borrow_mut().extend(change.value.iter().cloned());
                    true
                },
            );
        };
        search(&mut combobox, "a");
        search(&mut combobox, "ap");
        assert_eq!(*asked.borrow(), ["load:1:a", "load:2:ap"]);
        assert!(
            !combobox.set("loaded", "1"),
            "the answer to the first search is stale"
        );
        assert_eq!(combobox.loading(), Loading::Loading);
        assert!(combobox.set("loaded", "2"));
        assert_eq!(combobox.loading(), Loading::Idle);
        search(&mut combobox, "apx");
        combobox.set("failed", "3");
        assert_eq!(combobox.loading(), Loading::Error);
    }
}
