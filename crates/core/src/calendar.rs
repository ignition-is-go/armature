use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::date::{Date, date_for_key, month_grid};
use crate::keys::{Dir, Key};
use crate::primitive::{Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The grid of days.
    Grid,
    /// One day. Render one per date in `weeks`, with the date as its item.
    Day,
    PreviousMonth,
    NextMonth,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// The first day of the week, with Sunday as 0.
    pub week_start: u8,
    pub dir: Dir,
    /// The earliest and latest days that may be chosen.
    pub min: Option<Date>,
    pub max: Option<Date>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            week_start: 0,
            dir: Dir::Ltr,
            min: None,
            max: None,
        }
    }
}

const ACTIONS: &[&str] = &["select", "next_month", "previous_month"];

/// A month view for choosing a day. The value is the chosen day as `YYYY-MM-DD`.
#[derive(Clone, Debug)]
pub struct Calendar {
    opts: Options,
    /// The day the keyboard is on. Its month is the month shown.
    focused: Date,
    selected: Option<Date>,
    /// The month button holding focus, if one does.
    on_button: Option<Part>,
}

impl Calendar {
    /// Opens on the month containing `showing`.
    pub fn new(opts: Options, showing: Date) -> Self {
        Calendar {
            opts,
            focused: showing,
            selected: None,
            on_button: None,
        }
    }

    /// The year and month on show.
    pub fn month(&self) -> (i32, u8) {
        (self.focused.year, self.focused.month)
    }

    /// The weeks on show, padded with the neighbouring months' days.
    pub fn weeks(&self) -> Vec<[Date; 7]> {
        month_grid(self.focused.year, self.focused.month, self.opts.week_start)
    }

    /// Whether a day belongs to the month on show, for dimming the padding days.
    pub fn in_month(&self, date: Date) -> bool {
        (date.year, date.month) == self.month()
    }

    fn allowed(&self, date: Date) -> bool {
        self.opts.min.is_none_or(|min| date >= min) && self.opts.max.is_none_or(|max| date <= max)
    }

    fn select(&mut self, date: Date, source: Source, reason: Reason, gate: Gate) -> bool {
        if self.selected == Some(date) {
            return true;
        }
        let allowed = gate(&ValueChange {
            value: vec![date.format()],
            source,
            reason,
        });
        if allowed {
            self.selected = Some(date);
        }
        allowed
    }

    fn move_to(&mut self, date: Date) -> Reaction<Part> {
        self.focused = date;
        Reaction::item(Part::Day, true, Some(date.format()))
    }
}

impl PartOf for Part {
    type Primitive = Calendar;
}

impl Primitive for Calendar {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Grid, Part::Day, Part::PreviousMonth, Part::NextMonth]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match part {
            Part::Grid => Semantics {
                role: Some(Role::Grid),
                ..Default::default()
            },
            Part::PreviousMonth | Part::NextMonth => Semantics {
                role: Some(Role::Button),
                focusable: Focusable::TabStop,
                ..Default::default()
            },
            Part::Day => {
                let Some(date) = item.and_then(Date::parse) else {
                    return Semantics {
                        role: Some(Role::GridCell),
                        ..Default::default()
                    };
                };
                let shown = self.weeks().iter().flatten().any(|day| *day == date);
                Semantics {
                    role: Some(Role::GridCell),
                    selected: Some(self.selected == Some(date)),
                    disabled: !self.allowed(date),
                    hidden: !shown,
                    focusable: if date == self.focused {
                        Focusable::TabStop
                    } else {
                        Focusable::Programmatic
                    },
                    active: self.in_month(date),
                    ..Default::default()
                }
            }
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            // A month button is a button, so Enter and Space press it.
            Input::Key {
                key: Key::Enter | Key::Space,
                mods,
                ..
            } if self.on_button.is_some() => {
                let part = self.on_button.unwrap_or(Part::NextMonth);
                self.send(
                    Input::Press {
                        part,
                        item: None,
                        mods,
                    },
                    gate,
                )
            }
            Input::Focus {
                part: part @ (Part::PreviousMonth | Part::NextMonth),
                ..
            } => {
                self.on_button = Some(part);
                Reaction::default()
            }
            Input::Key {
                key: Key::Enter | Key::Space,
                ..
            } => {
                if self.allowed(self.focused) {
                    self.select(self.focused, Source::User, Reason::Keyboard, gate);
                }
                Reaction::handled()
            }
            Input::Key {
                key: Key::PageUp, ..
            } => self.move_to(self.focused.add_months(-1)),
            Input::Key {
                key: Key::PageDown, ..
            } => self.move_to(self.focused.add_months(1)),
            Input::Key { key, mods, .. } => {
                match date_for_key(self.focused, key, mods, self.opts.dir, self.opts.week_start) {
                    Some(date) => self.move_to(date),
                    None => Reaction::default(),
                }
            }
            Input::Focus {
                part: Part::Day,
                item: Some(day),
            } => {
                self.on_button = None;
                if let Some(date) = Date::parse(&day) {
                    self.focused = date;
                }
                Reaction::default()
            }
            Input::Press {
                part: Part::Day,
                item: Some(day),
                ..
            } => match Date::parse(&day).filter(|date| self.allowed(*date)) {
                Some(date) => {
                    self.select(date, Source::User, Reason::Press, gate);
                    self.move_to(date)
                }
                None => Reaction::default(),
            },
            // The month buttons keep focus, so a person can press them repeatedly.
            Input::Press {
                part: Part::PreviousMonth,
                ..
            } => {
                self.focused = self.focused.add_months(-1);
                Reaction::handled()
            }
            Input::Press {
                part: Part::NextMonth,
                ..
            } => {
                self.focused = self.focused.add_months(1);
                Reaction::handled()
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        self.selected.iter().map(|date| date.format()).collect()
    }

    fn set_value(&mut self, value: Vec<String>) {
        if let Some(date) = value.first().and_then(|text| Date::parse(text)) {
            self.selected = Some(date);
            self.focused = date;
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "calendar",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        match action {
            "next_month" => self.focused = self.focused.add_months(1),
            "previous_month" => self.focused = self.focused.add_months(-1),
            "select" => {
                let text = item.ok_or(ActError::MissingItem { action: "select" })?;
                let date =
                    Date::parse(text).ok_or_else(|| ActError::UnknownItem(text.to_string()))?;
                if !self.allowed(date) {
                    return Err(ActError::Disabled(text.to_string()));
                }
                if !self.select(date, Source::Agent, Reason::Action, gate) {
                    return Err(ActError::Refused);
                }
                self.focused = date;
            }
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
    use crate::keys::Modifiers;

    fn date(text: &str) -> Date {
        Date::parse(text).unwrap()
    }

    fn calendar() -> Calendar {
        Calendar::new(Options::default(), date("2026-11-17"))
    }

    fn key(calendar: &mut Calendar, key: Key) -> Reaction<Part> {
        calendar.send(
            Input::Key {
                key,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        )
    }

    #[test]
    fn arrows_move_a_day_or_a_week_and_enter_chooses() {
        let mut calendar = calendar();
        assert_eq!(
            key(&mut calendar, Key::ArrowRight).focus_item(),
            Some("2026-11-18")
        );
        assert_eq!(
            key(&mut calendar, Key::ArrowDown).focus_item(),
            Some("2026-11-25")
        );
        key(&mut calendar, Key::Enter);
        assert_eq!(calendar.value(), ["2026-11-25"]);
    }

    #[test]
    fn moving_past_the_end_of_the_month_shows_the_next_one() {
        let mut calendar = calendar();
        key(&mut calendar, Key::ArrowDown);
        key(&mut calendar, Key::ArrowDown);
        assert_eq!(calendar.month(), (2026, 12));
        assert_eq!(
            key(&mut calendar, Key::PageUp).focus_item(),
            Some("2026-11-01")
        );
        assert!(
            calendar
                .weeks()
                .iter()
                .flatten()
                .any(|day| *day == date("2026-11-01"))
        );
    }

    #[test]
    fn only_the_focused_day_is_a_tab_stop_and_other_months_are_dimmed() {
        let calendar = calendar();
        assert_eq!(
            calendar.semantics(Part::Day, Some("2026-11-17")).focusable,
            Focusable::TabStop
        );
        assert_eq!(
            calendar.semantics(Part::Day, Some("2026-11-18")).focusable,
            Focusable::Programmatic
        );
        assert!(
            !calendar.semantics(Part::Day, Some("2026-12-01")).active,
            "padding day of the next month"
        );
        assert!(
            calendar.semantics(Part::Day, Some("2027-03-01")).hidden,
            "not on show at all"
        );
    }

    #[test]
    fn days_outside_the_limits_cannot_be_chosen() {
        let opts = Options {
            min: Some(date("2026-11-10")),
            max: Some(date("2026-11-20")),
            ..Default::default()
        };
        let mut calendar = Calendar::new(opts, date("2026-11-17"));
        let press = Input::Press {
            part: Part::Day,
            item: Some("2026-11-05".into()),
            mods: Modifiers::default(),
        };
        calendar.send(press, &mut |_| true);
        assert!(calendar.value().is_empty());
        assert!(calendar.semantics(Part::Day, Some("2026-11-05")).disabled);
        assert_eq!(
            calendar.act("select", Some("2026-11-25"), &mut |_| true),
            Err(ActError::Disabled("2026-11-25".into()))
        );
        calendar
            .act("select", Some("2026-11-12"), &mut |_| true)
            .unwrap();
        assert_eq!(calendar.value(), ["2026-11-12"]);
    }

    #[test]
    fn the_month_buttons_page_without_choosing() {
        let mut calendar = calendar();
        calendar.send(
            Input::Press {
                part: Part::NextMonth,
                item: None,
                mods: Modifiers::default(),
            },
            &mut |_| true,
        );
        assert_eq!(calendar.month(), (2026, 12));
        assert!(calendar.value().is_empty());
    }

    #[test]
    fn enter_on_a_month_button_pages_the_month_and_chooses_nothing() {
        let mut calendar = calendar();
        calendar.send(
            Input::Focus {
                part: Part::NextMonth,
                item: None,
            },
            &mut |_| true,
        );
        key(&mut calendar, Key::Enter);
        assert_eq!(calendar.month(), (2026, 12));
        assert!(calendar.value().is_empty());
        calendar.send(
            Input::Focus {
                part: Part::Day,
                item: Some("2026-12-17".into()),
            },
            &mut |_| true,
        );
        key(&mut calendar, Key::Enter);
        assert_eq!(calendar.value(), ["2026-12-17"]);
    }

    #[test]
    fn it_conforms() {
        let days: Vec<Item> = ["2026-11-17", "2026-11-18", "2026-12-01"]
            .iter()
            .map(|day| Item {
                key: day.to_string(),
                label: day.to_string(),
                disabled: false,
            })
            .collect();
        crate::testing::assert_conforms(calendar, &days);
    }
}
