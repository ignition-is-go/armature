use crate::agent::{ActError, Snapshot};
use crate::change::Gate;
use crate::keys::{Key, Modifiers};
use crate::listbox::{self, Listbox};
use crate::popup::{Align, Placement, Side};
use crate::primitive::{
    Focusable, Input, Item, PartOf, Primitive, Reaction, Relations, Role, Semantics,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Trigger,
    /// The popup that holds the options.
    Content,
    Option,
}

const ACTIONS: &[&str] = &["open", "close", "select", "clear"];

/// A single-choice select: a trigger that opens a popup listbox.
#[derive(Clone, Debug, Default)]
pub struct Select {
    list: Listbox,
    open: bool,
    /// Whether several options can be chosen. The popup then stays open.
    multiple: bool,
    /// Shown but not to be changed, by the person or by an agent.
    locked: bool,
}

impl Select {
    /// A select that takes several options. Choosing one leaves the popup open for
    /// the next, and Escape or a press elsewhere closes it.
    #[must_use]
    pub fn multiple() -> Self {
        let opts = listbox::Options {
            mode: crate::selection::Mode::Multiple,
            ..Default::default()
        };
        Self {
            list: Listbox::new(opts),
            open: false,
            multiple: true,
            locked: false,
        }
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// A select that shows its value and cannot be changed or cleared.
    #[must_use]
    pub const fn read_only(mut self) -> Self {
        self.locked = true;
        self
    }

    /// Opens the popup and moves focus to the chosen option, or the first one.
    fn show(&mut self) -> Reaction<Part> {
        self.open = true;
        let target = self.list.tab_stop().map(str::to_string);
        if let Some(item) = target.clone() {
            self.tell_list(Input::Focus {
                part: listbox::Part::Option,
                item: Some(item),
            });
        }
        Reaction::item(Part::Option, true, target)
    }

    /// Closes the popup and returns focus to the trigger.
    const fn dismiss(&mut self) -> Reaction<Part> {
        self.open = false;
        Reaction::focusing(Part::Trigger)
    }

    fn tell_list(&mut self, input: Input<listbox::Part>) -> Reaction<listbox::Part> {
        self.list.send(input, &mut |_| true)
    }

    fn on_open_key(
        &mut self,
        key: Key,
        mods: Modifiers,
        now_ms: u64,
        gate: Gate,
    ) -> Reaction<Part> {
        match key {
            Key::Escape => self.dismiss(),
            // Focus goes back to the trigger first, so the platform's own Tab then
            // moves on from there and not from inside the popup.
            Key::Tab => Reaction {
                handled: false,
                ..self.dismiss()
            },
            Key::Enter | Key::Space => {
                let choose = Input::Key {
                    key,
                    mods: Modifiers::default(),
                    now_ms,
                };
                self.list.send(choose, gate);
                if self.multiple {
                    Reaction::handled()
                } else {
                    self.dismiss()
                }
            }
            _ => {
                let moved = self.list.send(Input::Key { key, mods, now_ms }, gate);
                Reaction::item(
                    Part::Option,
                    moved.handled,
                    moved.focus_item().map(str::to_string),
                )
            }
        }
    }
}

impl PartOf for Part {
    type Primitive = Select;
}

impl Primitive for Select {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Trigger, Part::Content, Part::Option]
    }

    fn is_item(part: Part) -> bool {
        part == Part::Option
    }

    fn popup_of(part: Part) -> Option<Placement<Part>> {
        (part == Part::Content).then_some(Placement {
            anchor: Part::Trigger,
            side: Side::Bottom,
            align: Align::Start,
            gap: 4.0,
        })
    }

    fn relations(part: Part) -> Relations<Part> {
        match part {
            Part::Trigger => Relations {
                controls: Some(Part::Content),
                ..Default::default()
            },
            _ => Relations::default(),
        }
    }

    fn upsert_item(&mut self, item: Item) {
        self.list.upsert_item(item);
    }

    fn remove_item(&mut self, key: &str) {
        self.list.remove_item(key);
    }

    fn set_order(&mut self, rendered: &[(String, String)]) {
        self.list.set_order(rendered);
    }

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match part {
            Part::Trigger => Semantics {
                role: Some(Role::ComboBox),
                expanded: Some(self.open),
                focusable: Focusable::TabStop,
                ..Default::default()
            },
            Part::Content => Semantics {
                role: Some(Role::Listbox),
                hidden: !self.open,
                ..Default::default()
            },
            // Tab reaches the trigger, never an option inside the popup.
            Part::Option => Semantics {
                focusable: Focusable::Programmatic,
                ..self.list.semantics(listbox::Part::Option, item)
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        if self.locked && !matches!(input, Input::Focus { .. } | Input::Blur { .. }) {
            return Reaction::default();
        }
        match input {
            Input::PressOutside | Input::WindowBlur => {
                self.open = false;
                Reaction::default()
            }
            Input::Press {
                part: Part::Trigger,
                ..
            } if self.open => self.dismiss(),
            Input::Press {
                part: Part::Option,
                item,
                ..
            } if self.open => {
                // A plain press on one of several adds or removes it, as a checkbox would.
                let press = Input::Press {
                    part: listbox::Part::Option,
                    item,
                    mods: Modifiers {
                        ctrl: self.multiple,
                        ..Modifiers::default()
                    },
                };
                if self.list.send(press, gate).handled {
                    if self.multiple {
                        Reaction::handled()
                    } else {
                        self.dismiss()
                    }
                } else {
                    Reaction::default()
                }
            }
            Input::Focus {
                part: Part::Option,
                item,
            } => {
                self.tell_list(Input::Focus {
                    part: listbox::Part::Option,
                    item,
                });
                Reaction::default()
            }
            Input::Key { key, mods, now_ms } if self.open => {
                self.on_open_key(key, mods, now_ms, gate)
            }
            Input::Press {
                part: Part::Trigger,
                ..
            }
            | Input::Key {
                key: Key::Enter | Key::Space | Key::ArrowDown | Key::ArrowUp,
                ..
            } => self.show(),
            Input::Key {
                key: Key::Char(ch),
                mods,
                now_ms,
            } if !mods.ctrl => {
                let typed = Input::Key {
                    key: Key::Char(ch),
                    mods,
                    now_ms,
                };
                if !self.tell_list(typed).handled {
                    return Reaction::default();
                }
                let choose = Input::Key {
                    key: Key::Enter,
                    mods: Modifiers::default(),
                    now_ms,
                };
                self.list.send(choose, gate);
                Reaction::handled()
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        self.list.value()
    }

    fn set_value(&mut self, value: Vec<String>) {
        self.list.set_value(value);
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "select",
            expanded: Some(self.open),
            actions: ACTIONS,
            ..self.list.snapshot(label)
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        match action {
            "open" => self.open = true,
            "close" => self.open = false,
            "select" | "clear" if self.locked => return Err(ActError::Refused),
            "select" => self.list.act("select", item, gate)?,
            "clear" => self.list.act("clear", None, gate)?,
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitive::FocusTarget;

    fn select() -> Select {
        let mut select = Select::default();
        for (key, disabled) in [("small", false), ("medium", true), ("large", false)] {
            select.upsert_item(Item {
                key: key.into(),
                label: key.into(),
                disabled,
            });
        }
        select
    }

    fn key(select: &mut Select, key: Key) -> Reaction<Part> {
        select.send(
            Input::Key {
                key,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        )
    }

    fn press(select: &mut Select, part: Part, item: Option<&str>) -> Reaction<Part> {
        let item = item.map(str::to_string);
        select.send(
            Input::Press {
                part,
                item,
                mods: Modifiers::default(),
            },
            &mut |_| true,
        )
    }

    const TO_TRIGGER: Option<FocusTarget<Part>> = Some(FocusTarget {
        part: Part::Trigger,
        item: None,
    });

    #[test]
    fn the_popup_is_hidden_until_the_trigger_is_pressed() {
        let mut select = select();
        assert!(select.semantics(Part::Content, None).hidden);
        let opened = press(&mut select, Part::Trigger, None);
        assert!(!select.semantics(Part::Content, None).hidden);
        assert_eq!(select.semantics(Part::Trigger, None).expanded, Some(true));
        assert_eq!(opened.focus_item(), Some("small"));
    }

    #[test]
    fn opening_lands_on_the_chosen_option() {
        let mut select = select();
        press(&mut select, Part::Trigger, None);
        press(&mut select, Part::Option, Some("large"));
        assert_eq!(key(&mut select, Key::ArrowDown).focus_item(), Some("large"));
    }

    #[test]
    fn choosing_with_the_keyboard_closes_and_returns_focus_to_the_trigger() {
        let mut select = select();
        key(&mut select, Key::Enter);
        key(&mut select, Key::ArrowDown);
        let chosen = key(&mut select, Key::Enter);
        assert_eq!(select.value(), ["large"]);
        assert!(!select.is_open());
        assert_eq!(chosen.focus, TO_TRIGGER);
    }

    #[test]
    fn escape_closes_without_choosing() {
        let mut select = select();
        press(&mut select, Part::Trigger, None);
        key(&mut select, Key::ArrowDown);
        let closed = key(&mut select, Key::Escape);
        assert_eq!(select.value(), Vec::<String>::new());
        assert!(!select.is_open());
        assert_eq!(closed.focus, TO_TRIGGER);
    }

    #[test]
    fn a_press_outside_closes_and_leaves_focus_where_the_press_put_it() {
        let mut select = select();
        press(&mut select, Part::Trigger, None);
        let closed = select.send(Input::PressOutside, &mut |_| true);
        assert!(!select.is_open());
        assert_eq!(closed, Reaction::default());
    }

    #[test]
    fn tab_closes_and_moves_on_from_the_trigger() {
        let mut select = select();
        press(&mut select, Part::Trigger, None);
        let tabbed = key(&mut select, Key::Tab);
        assert!(!select.is_open());
        assert!(!tabbed.handled, "the platform still performs the Tab");
        assert_eq!(tabbed.focus, TO_TRIGGER);
    }

    #[test]
    fn typing_on_a_closed_select_chooses_the_matching_option() {
        let mut select = select();
        assert!(key(&mut select, Key::Char('l')).handled);
        assert_eq!(select.value(), ["large"]);
        assert!(!select.is_open());
        assert!(
            !key(&mut select, Key::Char('m')).handled,
            "medium is disabled"
        );
        assert_eq!(select.value(), ["large"]);
    }

    #[test]
    fn pressing_a_disabled_option_keeps_the_popup_open() {
        let mut select = select();
        press(&mut select, Part::Trigger, None);
        press(&mut select, Part::Option, Some("medium"));
        assert!(select.is_open());
        assert_eq!(select.value(), Vec::<String>::new());
    }

    #[test]
    fn options_are_never_tab_stops() {
        let select = select();
        assert_eq!(
            select.semantics(Part::Option, Some("small")).focusable,
            Focusable::Programmatic
        );
        assert_eq!(
            select.semantics(Part::Trigger, None).focusable,
            Focusable::TabStop
        );
    }

    #[test]
    fn an_agent_can_choose_without_opening_and_sees_whether_it_is_open() {
        let mut select = select();
        select.act("select", Some("large"), &mut |_| true).unwrap();
        assert_eq!(select.value(), ["large"]);
        assert_eq!(select.snapshot("Size").expanded, Some(false));
        select.act("open", None, &mut |_| true).unwrap();
        assert_eq!(select.snapshot("Size").expanded, Some(true));
        assert_eq!(select.act("select", Some("large"), &mut |_| false), Ok(()));
        assert_eq!(
            select.act("select", Some("small"), &mut |_| false),
            Err(ActError::Refused)
        );
    }

    #[test]
    fn several_options_can_be_chosen_and_the_popup_stays_open() {
        let mut select = Select::multiple();
        for (key, label) in [("s", "Small"), ("m", "Medium"), ("l", "Large")] {
            select.upsert_item(Item {
                key: key.into(),
                label: label.into(),
                disabled: false,
            });
        }
        press(&mut select, Part::Trigger, None);
        press(&mut select, Part::Option, Some("s"));
        press(&mut select, Part::Option, Some("l"));
        assert_eq!(select.value(), ["s", "l"]);
        assert!(select.is_open());
        press(&mut select, Part::Option, Some("s"));
        assert_eq!(select.value(), ["l"]);
    }
}
