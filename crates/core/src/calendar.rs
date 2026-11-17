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
    /// Whether one day, several days or a span of days is chosen.
    pub mode: Selecting,
    /// Days that cannot be chosen, such as weekends or days already booked.
    pub unavailable: Option<fn(Date) -> bool>,
    /// Whether choosing the chosen day again clears it.
    pub deselectable: bool,
    /// How many months show side by side. The month buttons page by this many.
    pub months: usize,
}

/// What a calendar chooses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Selecting {
    #[default]
    Single,
    /// Any number of separate days. Choosing a chosen day lets it go.
    Multiple,
    /// A first and a last day. The value is both once the second is chosen.
    Range,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            week_start: 0,
            dir: Dir::Ltr,
            min: None,
            max: None,
            mode: Selecting::Single,
            unavailable: None,
            deselectable: false,
            months: 1,
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
    /// The chosen days in order. For a span, its first day and then its last.
    selected: Vec<Date>,
    /// The month button holding focus, if one does.
    on_button: Option<Part>,
}

impl Calendar {
    /// Opens on the month containing `showing`.
    pub fn new(opts: Options, showing: Date) -> Self {
        Calendar {
            opts,
            focused: showing,
            selected: Vec::new(),
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
        self.shown_months().contains(&(date.year, date.month))
    }

    /// The months on show, in order. With several, they are fixed runs of that many
    /// months, so the view only changes when focus leaves the run.
    pub fn shown_months(&self) -> Vec<(i32, u8)> {
        let run = self.opts.months.max(1) as i32;
        let index = self.focused.year * 12 + self.focused.month as i32 - 1;
        let first = index.div_euclid(run) * run;
        (first..first + run)
            .map(|month| (month.div_euclid(12), (month.rem_euclid(12) + 1) as u8))
            .collect()
    }

    /// One shown month's weeks, padded with its neighbours' days. Where several
    /// months show, draw a padding day only if `in_month` says no for it, and leave
    /// it out, so no day is drawn twice.
    pub fn weeks_of(&self, year: i32, month: u8) -> Vec<[Date; 7]> {
        month_grid(year, month, self.opts.week_start)
    }

    fn page(&self) -> i32 {
        self.opts.months.max(1) as i32
    }

    fn allowed(&self, date: Date) -> bool {
        self.opts.min.is_none_or(|min| date >= min)
            && self.opts.max.is_none_or(|max| date <= max)
            && !self
                .opts
                .unavailable
                .is_some_and(|unavailable| unavailable(date))
    }

    /// Whether a day is chosen, or lies inside the chosen span.
    pub fn is_selected(&self, date: Date) -> bool {
        match (self.opts.mode, self.selected.as_slice()) {
            (Selecting::Range, [first, last]) => (*first..=*last).contains(&date),
            _ => self.selected.contains(&date),
        }
    }

    /// The first and last day of the span, once both are chosen.
    pub fn range(&self) -> Option<(Date, Date)> {
        match (self.opts.mode, self.selected.as_slice()) {
            (Selecting::Range, [first, last]) => Some((*first, *last)),
            _ => None,
        }
    }

    fn select(&mut self, date: Date, source: Source, reason: Reason, gate: Gate) -> bool {
        let chosen = self.selected.contains(&date);
        let mut next = match self.opts.mode {
            Selecting::Single if chosen && self.opts.deselectable => Vec::new(),
            Selecting::Single if chosen => return true,
            Selecting::Single => vec![date],
            Selecting::Multiple if chosen => self
                .selected
                .iter()
                .copied()
                .filter(|day| *day != date)
                .collect(),
            Selecting::Multiple => self.selected.iter().copied().chain([date]).collect(),
            // The second day closes the span. A third starts another.
            Selecting::Range if self.selected.len() == 1 => vec![self.selected[0], date],
            Selecting::Range => vec![date],
        };
        next.sort();
        // A span may not reach across a day that cannot be chosen.
        if let (Selecting::Range, [first, last]) = (self.opts.mode, next.as_slice()) {
            let days = first.days_until(*last);
            if (0..=days).any(|offset| !self.allowed(first.add_days(offset))) {
                next = vec![date];
            }
        }
        let allowed = gate(&ValueChange {
            value: next.iter().map(|day| day.format()).collect(),
            source,
            reason,
        });
        if allowed {
            self.selected = next;
        }
        allowed
    }

    fn move_to(&mut self, date: Date) -> Reaction<Part> {
        // The keys stop at the earliest and latest days allowed.
        let date = date.clamped(self.opts.min, self.opts.max);
        if date == self.focused {
            return Reaction::handled();
        }
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
                let months = self.shown_months();
                let shown = months.iter().any(|(year, month)| {
                    self.weeks_of(*year, *month)
                        .iter()
                        .flatten()
                        .any(|day| *day == date)
                });
                Semantics {
                    role: Some(Role::GridCell),
                    selected: Some(self.is_selected(date)),
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
            // PageUp and PageDown turn the month, and with Shift the year.
            Input::Key {
                key: Key::PageUp,
                mods,
                ..
            } => self.move_to(self.focused.add_months(if mods.shift { -12 } else { -1 })),
            Input::Key {
                key: Key::PageDown,
                mods,
                ..
            } => self.move_to(self.focused.add_months(if mods.shift { 12 } else { 1 })),
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
                self.focused = self.focused.add_months(-self.page());
                Reaction::handled()
            }
            Input::Press {
                part: Part::NextMonth,
                ..
            } => {
                self.focused = self.focused.add_months(self.page());
                Reaction::handled()
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        self.selected.iter().map(|date| date.format()).collect()
    }

    fn set_value(&mut self, value: Vec<String>) {
        let mut days: Vec<Date> = value.iter().filter_map(|text| Date::parse(text)).collect();
        days.sort();
        days.dedup();
        match self.opts.mode {
            Selecting::Single => days.truncate(1),
            Selecting::Range => days.truncate(2),
            Selecting::Multiple => {}
        }
        if let Some(first) = days.first() {
            self.focused = *first;
        }
        if !days.is_empty() || value.is_empty() {
            self.selected = days;
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
            "next_month" => self.focused = self.focused.add_months(self.page()),
            "previous_month" => self.focused = self.focused.add_months(-self.page()),
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
    fn a_span_is_two_presses_and_a_third_starts_another() {
        let start = Date::new(2026, 11, 17).unwrap();
        let mut calendar = Calendar::new(
            Options {
                mode: Selecting::Range,
                ..Default::default()
            },
            start,
        );
        let press = |calendar: &mut Calendar, day: &str| {
            calendar.send(
                Input::Press {
                    part: Part::Day,
                    item: Some(day.into()),
                    mods: Modifiers::default(),
                },
                &mut |_| true,
            );
        };
        press(&mut calendar, "2026-11-20");
        assert_eq!(calendar.range(), None, "one end is not a span yet");
        press(&mut calendar, "2026-11-12");
        assert_eq!(
            calendar.value(),
            ["2026-11-12", "2026-11-20"],
            "in order, whichever was pressed first"
        );
        assert!(
            calendar
                .semantics(Part::Day, Some("2026-11-15"))
                .selected
                .unwrap(),
            "days between are inside it"
        );
        press(&mut calendar, "2026-11-25");
        assert_eq!(calendar.value(), ["2026-11-25"]);
    }

    #[test]
    fn several_days_can_be_chosen_and_booked_days_cannot() {
        let start = Date::new(2026, 11, 17).unwrap();
        let weekend: fn(Date) -> bool = |date| matches!(date.weekday(), 0 | 6);
        let opts = Options {
            mode: Selecting::Multiple,
            unavailable: Some(weekend),
            ..Default::default()
        };
        let mut calendar = Calendar::new(opts, start);
        for day in ["2026-11-18", "2026-11-16", "2026-11-18", "2026-11-19"] {
            calendar.act("select", Some(day), &mut |_| true).unwrap();
        }
        assert_eq!(
            calendar.value(),
            ["2026-11-16", "2026-11-19"],
            "the day chosen twice was let go"
        );
        assert!(
            calendar.semantics(Part::Day, Some("2026-11-21")).disabled,
            "a Saturday"
        );
        assert!(
            calendar
                .act("select", Some("2026-11-21"), &mut |_| true)
                .is_err()
        );

        let mut span = Calendar::new(
            Options {
                mode: Selecting::Range,
                unavailable: Some(weekend),
                ..Default::default()
            },
            start,
        );
        span.act("select", Some("2026-11-19"), &mut |_| true)
            .unwrap();
        span.act("select", Some("2026-11-24"), &mut |_| true)
            .unwrap();
        assert_eq!(
            span.value(),
            ["2026-11-24"],
            "a span cannot reach across the weekend"
        );
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

    #[test]
    fn several_months_show_as_a_fixed_run_and_the_buttons_page_a_run_at_a_time() {
        let mut calendar = Calendar::new(
            Options {
                months: 2,
                ..Default::default()
            },
            Date::new(2026, 11, 17).unwrap(),
        );
        assert_eq!(calendar.shown_months(), [(2026, 11), (2026, 12)]);
        assert!(calendar.in_month(Date::new(2026, 12, 25).unwrap()));
        assert!(!calendar.semantics(Part::Day, Some("2026-12-25")).hidden);
        calendar.act("next_month", None, &mut |_| true).unwrap();
        assert_eq!(calendar.shown_months(), [(2027, 1), (2027, 2)]);
        assert!(
            calendar
                .weeks_of(2027, 2)
                .iter()
                .flatten()
                .any(|day| day.format() == "2027-02-28")
        );
    }
}
