use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::items::Items;
use crate::keys::Key;
use crate::primitive::{Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckState {
    Unchecked,
    Checked,
    /// Neither, as a box standing for a group that is checked only in part.
    Indeterminate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    /// The tick. Shown while the box is checked or indeterminate.
    Indicator,
}

const ACTIONS: &[&str] = &["toggle", "check", "uncheck"];

/// A checkbox. The value is `["on"]`, `["indeterminate"]` or empty. Toggling an
/// indeterminate box checks it.
#[derive(Clone, Debug)]
pub struct Checkbox {
    state: CheckState,
    disabled: bool,
}

impl Default for Checkbox {
    fn default() -> Self {
        Self::new(CheckState::Unchecked)
    }
}

fn written(state: CheckState) -> Vec<String> {
    match state {
        CheckState::Unchecked => Vec::new(),
        CheckState::Checked => vec!["on".to_string()],
        CheckState::Indeterminate => vec!["indeterminate".to_string()],
    }
}

impl Checkbox {
    #[must_use]
    pub const fn new(state: CheckState) -> Self {
        Self {
            state,
            disabled: false,
        }
    }

    #[must_use]
    pub const fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    #[must_use]
    pub const fn state(&self) -> CheckState {
        self.state
    }

    fn set(&mut self, state: CheckState, source: Source, reason: Reason, gate: Gate) -> bool {
        if state == self.state {
            return true;
        }
        let allowed = gate(&ValueChange {
            value: written(state),
            source,
            reason,
        });
        if allowed {
            self.state = state;
        }
        allowed
    }

    fn toggled(&self) -> CheckState {
        if self.state == CheckState::Checked {
            CheckState::Unchecked
        } else {
            CheckState::Checked
        }
    }
}

impl PartOf for Part {
    type Primitive = Checkbox;
}

impl Primitive for Checkbox {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Indicator]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, _: Option<&str>) -> Semantics {
        match part {
            Part::Root => Semantics {
                role: Some(Role::Checkbox),
                checked: Some(self.state == CheckState::Checked),
                mixed: self.state == CheckState::Indeterminate,
                disabled: self.disabled,
                focusable: if self.disabled {
                    Focusable::No
                } else {
                    Focusable::TabStop
                },
                ..Default::default()
            },
            Part::Indicator => Semantics {
                hidden: self.state == CheckState::Unchecked,
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        // Space toggles a checkbox. Enter is left alone, as it submits the form around it.
        let reason = match input {
            Input::Press { .. } => Reason::Press,
            Input::Key {
                key: Key::Space, ..
            } => Reason::Keyboard,
            _ => return Reaction::default(),
        };
        if !self.disabled {
            self.set(self.toggled(), Source::User, reason, gate);
        }
        Reaction::handled()
    }

    fn value(&self) -> Vec<String> {
        written(self.state)
    }

    fn set_value(&mut self, value: Vec<String>) {
        self.state = match value.first().map(String::as_str) {
            Some("on") => CheckState::Checked,
            Some("indeterminate") => CheckState::Indeterminate,
            _ => CheckState::Unchecked,
        };
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "checkbox",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, _: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let state = match action {
            "toggle" => self.toggled(),
            "check" => CheckState::Checked,
            "uncheck" => CheckState::Unchecked,
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        if self.disabled || !self.set(state, Source::Agent, Reason::Action, gate) {
            return Err(ActError::Refused);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GroupPart {
    Root,
    /// Optional. One box that stands for the whole group: it checks or clears every
    /// box, and reads checked, unchecked or indeterminate from them.
    All,
    /// One box, rendered per item.
    Option,
}

const GROUP_ACTIONS: &[&str] = &["check", "uncheck", "toggle", "check_all", "uncheck_all"];

/// A set of checkboxes with one value: the keys of the checked boxes.
#[derive(Clone, Debug, Default)]
pub struct CheckboxGroup {
    items: Items,
    checked: Vec<String>,
    on_all: bool,
    focused: Option<String>,
}

impl CheckboxGroup {
    /// What the box that stands for the group shows.
    #[must_use]
    pub fn all_state(&self) -> CheckState {
        let enabled = self.items.enabled_keys();
        let on = enabled.iter().filter(|key| self.is_checked(key)).count();
        match on {
            0 => CheckState::Unchecked,
            n if n == enabled.len() => CheckState::Checked,
            _ => CheckState::Indeterminate,
        }
    }

    fn is_checked(&self, key: &str) -> bool {
        self.checked.iter().any(|checked| checked == key)
    }

    /// What pressing the box that stands for the group does: checks every box,
    /// or clears them all when every one is checked already.
    fn toggle_all(&mut self, reason: Reason, gate: Gate) -> bool {
        let check = self.all_state() != CheckState::Checked;
        self.set_all(check, Source::User, reason, gate)
    }

    fn commit(
        &mut self,
        mut next: Vec<String>,
        source: Source,
        reason: Reason,
        gate: Gate,
    ) -> bool {
        // The value keeps the order the boxes appear in.
        next.sort_by_key(|key| self.items.index_of(key));
        if next == self.checked {
            return true;
        }
        let allowed = gate(&ValueChange {
            value: next.clone(),
            source,
            reason,
        });
        if allowed {
            self.checked = next;
        }
        allowed
    }

    fn toggle(&mut self, key: &str, source: Source, reason: Reason, gate: Gate) -> bool {
        if !self.items.is_enabled(key) {
            return false;
        }
        let mut next = self.checked.clone();
        match next.iter().position(|c| c == key) {
            Some(at) => drop(next.remove(at)),
            None => next.push(key.to_string()),
        }
        self.commit(next, source, reason, gate)
    }

    /// Checks every enabled box, or clears them if they all are. Disabled boxes keep
    /// whatever they had.
    fn set_all(&mut self, on: bool, source: Source, reason: Reason, gate: Gate) -> bool {
        let mut next: Vec<String> = self
            .checked
            .iter()
            .filter(|key| !self.items.is_enabled(key))
            .cloned()
            .collect();
        if on {
            next.extend(self.items.enabled_keys().into_iter().map(str::to_string));
        }
        self.commit(next, source, reason, gate)
    }
}

impl PartOf for GroupPart {
    type Primitive = CheckboxGroup;
}

impl Primitive for CheckboxGroup {
    type Part = GroupPart;

    fn parts() -> &'static [GroupPart] {
        &[GroupPart::Root, GroupPart::All, GroupPart::Option]
    }

    fn is_item(part: GroupPart) -> bool {
        part == GroupPart::Option
    }

    fn upsert_item(&mut self, item: Item) {
        self.items.upsert(item);
    }

    fn remove_item(&mut self, key: &str) {
        self.items.remove(key);
        self.checked.retain(|c| c != key);
    }

    fn set_order(&mut self, rendered: &[(String, String)]) {
        self.items.set_order(rendered);
    }

    fn semantics(&self, part: GroupPart, item: Option<&str>) -> Semantics {
        match (part, item) {
            (GroupPart::Root, _) => Semantics {
                role: Some(Role::Group),
                ..Default::default()
            },
            (GroupPart::All, _) => {
                let state = self.all_state();
                Semantics {
                    role: Some(Role::Checkbox),
                    checked: Some(state == CheckState::Checked),
                    mixed: state == CheckState::Indeterminate,
                    focusable: Focusable::TabStop,
                    ..Default::default()
                }
            }
            (GroupPart::Option, Some(key)) => {
                let enabled = self.items.is_enabled(key);
                Semantics {
                    role: Some(Role::Checkbox),
                    checked: Some(self.is_checked(key)),
                    disabled: !enabled,
                    // Every box is its own Tab stop, as separate checkboxes are.
                    focusable: if enabled {
                        Focusable::TabStop
                    } else {
                        Focusable::No
                    },
                    ..Default::default()
                }
            }
            (GroupPart::Option, None) => Semantics {
                role: Some(Role::Checkbox),
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<GroupPart>, gate: Gate) -> Reaction<GroupPart> {
        match input {
            Input::Focus { part, item } => {
                self.on_all = part == GroupPart::All;
                self.focused = item;
                Reaction::default()
            }
            Input::Press {
                part: GroupPart::All,
                ..
            } => {
                self.toggle_all(Reason::Press, gate);
                Reaction::handled()
            }
            Input::Press {
                part: GroupPart::Option,
                item: Some(key),
                ..
            } => {
                self.toggle(&key, Source::User, Reason::Press, gate);
                Reaction::handled()
            }
            Input::Key {
                key: Key::Space, ..
            } if self.on_all => {
                self.toggle_all(Reason::Keyboard, gate);
                Reaction::handled()
            }
            Input::Key {
                key: Key::Space, ..
            } => self.focused.clone().map_or_else(Reaction::default, |key| {
                self.toggle(&key, Source::User, Reason::Keyboard, gate);
                Reaction::handled()
            }),
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        self.checked.clone()
    }

    fn set_value(&mut self, value: Vec<String>) {
        self.checked = value
            .into_iter()
            .filter(|key| self.items.get(key).is_some())
            .collect();
        self.checked.sort_by_key(|key| self.items.index_of(key));
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .items
            .iter()
            .map(|item| SnapshotItem {
                key: item.key.clone(),
                label: item.label.clone(),
                disabled: item.disabled,
                selected: self.checked.contains(&item.key),
            })
            .collect();
        Snapshot {
            name: "checkbox-group",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: GROUP_ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let action: &'static str = match action {
            "check_all" => {
                return self
                    .set_all(true, Source::Agent, Reason::Action, gate)
                    .then_some(())
                    .ok_or(ActError::Refused);
            }
            "uncheck_all" => {
                return self
                    .set_all(false, Source::Agent, Reason::Action, gate)
                    .then_some(())
                    .ok_or(ActError::Refused);
            }
            "check" => "check",
            "uncheck" => "uncheck",
            "toggle" => "toggle",
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: GROUP_ACTIONS,
                });
            }
        };
        let key = item.ok_or(ActError::MissingItem { action })?;
        match self.items.get(key).map(|item| item.disabled) {
            None => return Err(ActError::UnknownItem(key.to_string())),
            Some(true) => return Err(ActError::Disabled(key.to_string())),
            Some(false) => {}
        }
        let on = self.is_checked(key);
        if (action == "check" && on) || (action == "uncheck" && !on) {
            return Ok(());
        }
        if self.toggle(key, Source::Agent, Reason::Action, gate) {
            Ok(())
        } else {
            Err(ActError::Refused)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Modifiers;

    const NONE: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
    };

    fn group() -> CheckboxGroup {
        let mut group = CheckboxGroup::default();
        for (key, disabled) in [("email", false), ("sms", false), ("post", true)] {
            group.upsert_item(Item {
                key: key.into(),
                label: key.into(),
                disabled,
            });
        }
        group
    }

    #[test]
    fn space_toggles_and_an_indeterminate_box_becomes_checked() {
        let mut checkbox = Checkbox::new(CheckState::Indeterminate);
        assert!(checkbox.semantics(Part::Root, None).mixed);
        checkbox.send(
            Input::Key {
                key: Key::Space,
                mods: NONE,
                now_ms: 0,
            },
            &mut |_| true,
        );
        assert_eq!(checkbox.state(), CheckState::Checked);
        let enter = checkbox.send(
            Input::Key {
                key: Key::Enter,
                mods: NONE,
                now_ms: 0,
            },
            &mut |_| true,
        );
        assert!(!enter.handled, "Enter belongs to the form");
        assert!(!checkbox.semantics(Part::Indicator, None).hidden);
    }

    #[test]
    fn the_box_for_the_group_follows_its_boxes_and_sets_them_all() {
        let mut group = group();
        let press = |group: &mut CheckboxGroup, part, item: Option<&str>| {
            group.send(
                Input::Press {
                    part,
                    item: item.map(str::to_string),
                    mods: NONE,
                },
                &mut |_| true,
            );
        };
        press(&mut group, GroupPart::Option, Some("sms"));
        assert_eq!(group.all_state(), CheckState::Indeterminate);
        press(&mut group, GroupPart::All, None);
        assert_eq!(
            group.value(),
            ["email", "sms"],
            "the disabled box is left alone"
        );
        assert_eq!(group.all_state(), CheckState::Checked);
        press(&mut group, GroupPart::All, None);
        assert_eq!(group.value(), Vec::<String>::new());
    }

    #[test]
    fn they_conform() {
        crate::testing::assert_conforms(Checkbox::default, &[]);
        let items: Vec<Item> = ["a", "b", "c"]
            .iter()
            .map(|k| Item {
                key: k.to_string(),
                label: k.to_string(),
                disabled: *k == "b",
            })
            .collect();
        crate::testing::assert_conforms(CheckboxGroup::default, &items);
    }
}
