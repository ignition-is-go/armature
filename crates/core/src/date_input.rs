use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::convert::Cast;
use crate::date::{Date, days_in_month};
use crate::keys::Key;
use crate::primitive::{
    Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics, ValueRange,
};

/// One editable piece of a date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Segment {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
    /// Morning or afternoon, where the hours run to twelve.
    Period,
}

impl Segment {
    pub const DATE: [Self; 3] = [Self::Year, Self::Month, Self::Day];
    pub const DATE_TIME: [Self; 5] = [Self::Year, Self::Month, Self::Day, Self::Hour, Self::Minute];
    const ALL: [Self; 7] = [
        Self::Year,
        Self::Month,
        Self::Day,
        Self::Hour,
        Self::Minute,
        Self::Second,
        Self::Period,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Year => "year",
            Self::Month => "month",
            Self::Day => "day",
            Self::Hour => "hour",
            Self::Minute => "minute",
            Self::Second => "second",
            Self::Period => "period",
        }
    }

    fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|segment| segment.name() == name)
    }

    fn width(self) -> usize {
        if self == Self::Year { 4 } else { 2 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    /// One piece of the date, keyed by its name: `year`, `month`, `day`, `hour` or
    /// `minute`. Fill it from `text`.
    Segment,
}

const ACTIONS: &[&str] = &["set", "clear"];

/// What has been given for each piece so far. The hour is on the 24-hour clock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Fields {
    year: Option<u32>,
    month: Option<u32>,
    day: Option<u32>,
    hour: Option<u32>,
    minute: Option<u32>,
    second: Option<u32>,
}

impl Fields {
    /// Sets a piece that is kept as a number. Morning or afternoon is not one.
    const fn set(&mut self, segment: Segment, number: Option<u32>) {
        match segment {
            Segment::Year => self.year = number,
            Segment::Month => self.month = number,
            Segment::Day => self.day = number,
            Segment::Hour => self.hour = number,
            Segment::Minute => self.minute = number,
            Segment::Second => self.second = number,
            Segment::Period => {}
        }
    }

    /// The date, once the year, month and day are all given.
    fn date(self) -> Option<(u32, u32, u32)> {
        Some((self.year?, self.month?, self.day?))
    }
}

/// An hour as shown on a twelve-hour clock, on the 24-hour clock.
const fn to_24_hour(shown: u32, pm: bool) -> u32 {
    (shown % 12).saturating_add(if pm { 12 } else { 0 })
}

/// A date typed one piece at a time.
///
/// Up and Down step a piece, digits fill it and move on when it is full, and Left and
/// Right move between pieces. The value is the date as `YYYY-MM-DD`, with `THH:MM` when
/// it carries a time, once every piece holds something.
#[derive(Clone, Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent setting"
)]
pub struct DateInput {
    with_time: bool,
    fields: Fields,
    focused: Segment,
    /// The digits typed into the focused piece so far.
    typed: String,
    /// The earliest and latest dates taken. A date outside them is refused.
    earliest: Option<Date>,
    latest: Option<Date>,
    /// Whether seconds are shown.
    seconds: bool,
    /// Whether the hours run to twelve with a morning or afternoon after them.
    twelve: bool,
    /// Whether it is afternoon, for when the hour has yet to be given.
    pm: bool,
}

impl Default for DateInput {
    fn default() -> Self {
        Self::new(false)
    }
}

impl DateInput {
    #[must_use]
    pub const fn new(with_time: bool) -> Self {
        Self {
            with_time,
            fields: Fields {
                year: None,
                month: None,
                day: None,
                hour: None,
                minute: None,
                second: None,
            },
            focused: Segment::Year,
            typed: String::new(),
            earliest: None,
            latest: None,
            seconds: false,
            twelve: false,
            pm: false,
        }
    }

    #[must_use]
    pub const fn between(mut self, earliest: Option<Date>, latest: Option<Date>) -> Self {
        self.earliest = earliest;
        self.latest = latest;
        self
    }

    /// Whether a complete date lies outside the earliest and latest taken.
    fn out_of_range(&self, fields: Fields) -> bool {
        let Some((year, month, day)) = fields.date() else {
            return false;
        };
        let Some(date) = Date::new(year.cast::<i32>(), month.cast::<u8>(), day.cast::<u8>()) else {
            return true;
        };
        self.earliest.is_some_and(|earliest| date < earliest)
            || self.latest.is_some_and(|latest| date > latest)
    }

    /// The pieces shown, in order.
    #[must_use]
    pub fn segments(&self) -> Vec<Segment> {
        let mut shown = Segment::DATE.to_vec();
        if self.with_time {
            shown.extend([Segment::Hour, Segment::Minute]);
            shown.extend(self.seconds.then_some(Segment::Second));
            shown.extend(self.twelve.then_some(Segment::Period));
        }
        shown
    }

    /// Shows seconds after the minutes. Only a date with a time has them.
    #[must_use]
    pub const fn with_seconds(mut self) -> Self {
        self.seconds = true;
        self
    }

    /// Runs the hours from 1 to 12, with a piece after them for morning or afternoon.
    #[must_use]
    pub const fn twelve_hour(mut self) -> Self {
        self.twelve = true;
        self
    }

    /// What a piece shows as a number. The hour is kept on the 24-hour clock and
    /// shown on whichever clock was asked for.
    fn get(&self, segment: Segment) -> Option<u32> {
        match segment {
            Segment::Period => Some(u32::from(self.pm)),
            Segment::Hour if self.twelve => self
                .fields
                .hour
                .map(|hour| if hour % 12 == 0 { 12 } else { hour % 12 }),
            Segment::Year => self.fields.year,
            Segment::Month => self.fields.month,
            Segment::Day => self.fields.day,
            Segment::Hour => self.fields.hour,
            Segment::Minute => self.fields.minute,
            Segment::Second => self.fields.second,
        }
    }

    fn limits(&self, segment: Segment) -> (u32, u32) {
        match segment {
            Segment::Year => (1, 9999),
            Segment::Month => (1, 12),
            // A day is checked against its month once the month and year are known.
            Segment::Day => match (self.get(Segment::Year), self.get(Segment::Month)) {
                (year, Some(month)) => (
                    1,
                    u32::from(days_in_month(
                        year.unwrap_or(2000).cast::<i32>(),
                        month.cast::<u8>(),
                    )),
                ),
                _ => (1, 31),
            },
            Segment::Hour if self.twelve => (1, 12),
            Segment::Hour => (0, 23),
            Segment::Minute | Segment::Second => (0, 59),
            Segment::Period => (0, 1),
        }
    }

    /// The value these fields make: nothing while a piece is missing or the date
    /// lies outside the limits, where it stays on screen to be corrected.
    fn value_of(&self, fields: Fields) -> Vec<String> {
        if self.out_of_range(fields) {
            Vec::new()
        } else {
            self.written(fields)
        }
    }

    fn written(&self, fields: Fields) -> Vec<String> {
        let Some((year, month, day)) = fields.date() else {
            return Vec::new();
        };
        let date = format!("{year:04}-{month:02}-{day:02}");
        match (self.with_time, fields.hour, fields.minute) {
            (false, _, _) => vec![date],
            (true, Some(hour), Some(minute)) => match (self.seconds, fields.second) {
                (false, _) => vec![format!("{date}T{hour:02}:{minute:02}")],
                (true, Some(second)) => vec![format!("{date}T{hour:02}:{minute:02}:{second:02}")],
                (true, None) => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// Stores a piece, pulling the day back if its month turned out shorter.
    fn put(
        &mut self,
        segment: Segment,
        number: Option<u32>,
        source: Source,
        reason: Reason,
        gate: Gate,
    ) -> bool {
        let mut fields = self.fields;
        match segment {
            // Morning or afternoon moves the hour to the other half of the day.
            Segment::Period => {
                let pm = number == Some(1);
                fields.hour = fields.hour.map(|hour| to_24_hour(hour, pm));
                self.pm = pm;
            }
            Segment::Hour if self.twelve => {
                fields.hour = number.map(|shown| to_24_hour(shown, self.pm));
            }
            other => fields.set(other, number),
        }
        let trial = Self {
            fields,
            ..self.clone()
        };
        let (_, longest) = trial.limits(Segment::Day);
        fields.day = fields.day.map(|day| day.min(longest));
        let (before, after) = (self.value_of(self.fields), self.value_of(fields));
        if before != after
            && !gate(&ValueChange {
                value: after,
                source,
                reason,
            })
        {
            return false;
        }
        self.fields = fields;
        true
    }

    fn neighbour(&self, forward: bool) -> Option<Segment> {
        let segments = self.segments();
        let at = segments
            .iter()
            .position(|segment| *segment == self.focused)?;
        if forward {
            segments.get(at.saturating_add(1)).copied()
        } else {
            at.checked_sub(1).and_then(|at| segments.get(at).copied())
        }
    }

    fn move_to(&mut self, segment: Segment) -> Reaction<Part> {
        self.focused = segment;
        self.typed.clear();
        Reaction::item(Part::Segment, true, Some(segment.name().to_string()))
    }
}

impl DateInput {
    /// A key on the focused piece: stepping it, typing into it, emptying it, or
    /// moving to the piece beside it.
    fn on_key(&mut self, key: Key, gate: Gate) -> Reaction<Part> {
        let segment = self.focused;
        match key {
            Key::ArrowUp | Key::ArrowDown => {
                let next = self.stepped(segment, key == Key::ArrowUp);
                self.typed.clear();
                self.put(segment, Some(next), Source::User, Reason::Keyboard, gate);
                Reaction::handled()
            }
            Key::ArrowRight | Key::ArrowLeft => self.move_beside(key == Key::ArrowRight),
            Key::Backspace => {
                self.typed.clear();
                self.put(segment, None, Source::User, Reason::Keyboard, gate);
                Reaction::handled()
            }
            // Morning and afternoon answer to their first letters.
            Key::Char(letter @ ('a' | 'A' | 'p' | 'P')) if segment == Segment::Period => {
                let afternoon = u32::from(matches!(letter, 'p' | 'P'));
                self.put(segment, Some(afternoon), Source::User, Reason::Text, gate);
                Reaction::handled()
            }
            Key::Char(digit) if digit.is_ascii_digit() && segment != Segment::Period => {
                self.typed_digit(segment, digit, gate)
            }
            // A separator typed out of habit moves on, as it would in plain text.
            Key::Char('/' | '-' | '.' | ':') => self.move_beside(true),
            _ => Reaction::default(),
        }
    }

    /// What a piece holds after one step up or down. Stepping wraps round, and an
    /// empty piece starts at an end.
    fn stepped(&self, segment: Segment, up: bool) -> u32 {
        let (min, max) = self.limits(segment);
        match (self.get(segment), up) {
            (None, true) => min,
            (None, false) => max,
            (Some(now), true) if now >= max => min,
            (Some(now), true) => now.saturating_add(1),
            (Some(now), false) if now <= min => max,
            (Some(now), false) => now.saturating_sub(1),
        }
    }

    /// Moves to the piece after or before the focused one, if there is one.
    fn move_beside(&mut self, forward: bool) -> Reaction<Part> {
        self.neighbour(forward)
            .map_or_else(Reaction::handled, |next| self.move_to(next))
    }

    /// A digit typed into a piece. A full piece moves on to the next.
    fn typed_digit(&mut self, segment: Segment, digit: char, gate: Gate) -> Reaction<Part> {
        let (min, max) = self.limits(segment);
        self.typed.push(digit);
        let number: u32 = self.typed.parse().unwrap_or(0);
        // The piece is full when another digit could not fit under the most it could
        // ever hold. What was typed is then brought within what this month allows.
        let most = if segment == Segment::Day { 31 } else { max };
        let full = self.typed.len() >= segment.width() || number.saturating_mul(10) > most;
        let stored = match (full, number >= min) {
            (true, _) => Some(number.clamp(min, max)),
            (false, true) => Some(number.min(max)),
            // A leading zero is not a value yet.
            (false, false) => None,
        };
        self.put(segment, stored, Source::User, Reason::Text, gate);
        match (full, self.neighbour(true)) {
            (true, Some(next)) => self.move_to(next),
            (true, None) => {
                self.typed.clear();
                Reaction::handled()
            }
            (false, _) => Reaction::handled(),
        }
    }
}

impl PartOf for Part {
    type Primitive = DateInput;
}

impl Primitive for DateInput {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Segment]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn named_part(&self) -> Option<Part> {
        Some(Part::Root)
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        let segment = item
            .and_then(Segment::named)
            .filter(|segment| self.segments().contains(segment));
        match (part, segment) {
            (Part::Root, _) => Semantics {
                role: Some(Role::Group),
                ..Default::default()
            },
            (Part::Segment, Some(segment)) => {
                let (min, max) = self.limits(segment);
                Semantics {
                    role: Some(Role::SpinButton),
                    invalid: self.out_of_range(self.fields),
                    range: self.get(segment).map(|now| ValueRange {
                        now: f64::from(now),
                        min: f64::from(min),
                        max: f64::from(max),
                    }),
                    // The first piece is the way in. The arrows reach the rest.
                    focusable: if segment == self.focused {
                        Focusable::TabStop
                    } else {
                        Focusable::Programmatic
                    },
                    ..Default::default()
                }
            }
            (Part::Segment, None) => Semantics {
                role: Some(Role::SpinButton),
                hidden: item.is_some(),
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Focus {
                part: Part::Segment,
                item: Some(name),
            } => {
                if let Some(segment) =
                    Segment::named(&name).filter(|segment| *segment != self.focused)
                {
                    self.focused = segment;
                    self.typed.clear();
                }
                Reaction::default()
            }
            Input::Key { key, mods, .. } if !mods.ctrl && !mods.alt => self.on_key(key, gate),
            _ => Reaction::default(),
        }
    }

    /// What a piece shows: its number, or dashes while it is empty.
    fn text(&self, part: Part, item: Option<&str>) -> Option<String> {
        let segment = item
            .and_then(Segment::named)
            .filter(|_| part == Part::Segment)?;
        if segment == Segment::Period {
            return Some(if self.pm { "PM" } else { "AM" }.to_string());
        }
        Some(self.get(segment).map_or_else(
            || "-".repeat(segment.width()),
            |number| format!("{number:0width$}", width = segment.width()),
        ))
    }

    fn value(&self) -> Vec<String> {
        self.value_of(self.fields)
    }

    fn set_value(&mut self, value: Vec<String>) {
        let Some(text) = value.first() else {
            self.fields = Fields::default();
            return;
        };
        let (date, time) = text.split_once('T').unwrap_or((text, ""));
        let Some(date) = Date::parse(date) else {
            return;
        };
        let mut clock = time.split(':').map(|piece| piece.parse::<u32>().ok());
        let (hour, minute, second) = (
            clock.next().flatten(),
            clock.next().flatten(),
            clock.next().flatten(),
        );
        self.pm = hour.is_some_and(|hour| hour >= 12);
        self.fields = Fields {
            year: Some(date.year.cast::<u32>()),
            month: Some(u32::from(date.month)),
            day: Some(u32::from(date.day)),
            hour,
            minute,
            second,
        };
        // A date with a time always has one: midnight, when none was written.
        if self.with_time {
            self.fields.hour = Some(hour.unwrap_or(0).min(23));
            self.fields.minute = Some(minute.unwrap_or(0).min(59));
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "date-input",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let mut next = self.clone();
        match action {
            "clear" => next.fields = Fields::default(),
            "set" => {
                let text = item.ok_or(ActError::MissingItem { action: "set" })?;
                next.fields = Fields::default();
                next.set_value(vec![text.to_string()]);
                if next.value().is_empty() {
                    return Err(ActError::UnknownItem(text.to_string()));
                }
            }
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        }
        if next.value() != self.value()
            && !gate(&ValueChange {
                value: next.value(),
                source: Source::Agent,
                reason: Reason::Action,
            })
        {
            return Err(ActError::Refused);
        }
        self.fields = next.fields;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Modifiers;

    fn key(input: &mut DateInput, key: Key) -> Reaction<Part> {
        input.send(
            Input::Key {
                key,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        )
    }

    fn type_digits(input: &mut DateInput, digits: &str) {
        for digit in digits.chars() {
            key(input, Key::Char(digit));
        }
    }

    #[test]
    fn digits_fill_a_piece_and_move_on_when_no_more_could_fit() {
        let mut input = DateInput::default();
        type_digits(&mut input, "2026");
        assert_eq!(input.focused, Segment::Month, "four digits fill the year");
        type_digits(&mut input, "2");
        assert_eq!(
            input.focused,
            Segment::Day,
            "no month starts with 2 and has two digits"
        );
        assert!(
            input.value().is_empty(),
            "not a date until every piece is in"
        );
        type_digits(&mut input, "30");
        assert_eq!(input.value(), ["2026-02-28"], "February is not that long");
        assert_eq!(
            input.text(Part::Segment, Some("month")).as_deref(),
            Some("02")
        );
    }

    #[test]
    fn arrows_step_a_piece_round_and_walk_between_pieces() {
        let mut input = DateInput::default();
        input.set_value(vec!["2026-12-31".into()]);
        assert_eq!(key(&mut input, Key::ArrowRight).focus_item(), Some("month"));
        key(&mut input, Key::ArrowUp);
        assert_eq!(input.value(), ["2026-01-31"], "December wraps to January");
        key(&mut input, Key::ArrowUp);
        assert_eq!(
            input.value(),
            ["2026-02-28"],
            "and the day is pulled back to fit"
        );
        key(&mut input, Key::Backspace);
        assert_eq!(input.value(), Vec::<String>::new());
        assert_eq!(
            input.text(Part::Segment, Some("month")).as_deref(),
            Some("--")
        );
    }

    #[test]
    fn a_time_makes_it_a_date_and_time() {
        let mut input = DateInput::new(true);
        type_digits(&mut input, "20261102");
        assert_eq!(input.value(), Vec::<String>::new());
        type_digits(&mut input, "1430");
        assert_eq!(input.value(), ["2026-11-02T14:30"]);
        input
            .act("set", Some("2027-01-05T09:15"), &mut |_| true)
            .unwrap();
        assert_eq!(
            input.text(Part::Segment, Some("hour")).as_deref(),
            Some("09")
        );
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(DateInput::default, &[]);
        crate::testing::assert_conforms(|| DateInput::new(true), &[]);
    }

    #[test]
    fn a_date_outside_the_limits_stays_on_screen_but_is_not_a_value() {
        let earliest = Date::new(2026, 6, 1);
        let mut input = DateInput::default().between(earliest, None);
        type_digits(&mut input, "20260315");
        assert_eq!(input.value(), Vec::<String>::new());
        assert!(input.semantics(Part::Segment, Some("day")).invalid);
        assert_eq!(
            input.text(Part::Segment, Some("month")).as_deref(),
            Some("03"),
            "still there to correct"
        );
        key(&mut input, Key::ArrowLeft);
        type_digits(&mut input, "07");
        assert_eq!(input.value(), ["2026-07-15"]);
    }

    #[test]
    fn a_twelve_hour_clock_keeps_the_hour_on_the_24_hour_clock_underneath() {
        let mut input = DateInput::new(true).twelve_hour();
        assert_eq!(input.segments().last(), Some(&Segment::Period));
        type_digits(&mut input, "20261102");
        type_digits(&mut input, "0230");
        assert_eq!(input.value(), ["2026-11-02T02:30"]);
        assert_eq!(input.focused, Segment::Period);
        key(&mut input, Key::Char('p'));
        assert_eq!(input.value(), ["2026-11-02T14:30"]);
        assert_eq!(
            input.text(Part::Segment, Some("hour")).as_deref(),
            Some("02")
        );
        assert_eq!(
            input.text(Part::Segment, Some("period")).as_deref(),
            Some("PM")
        );
        key(&mut input, Key::ArrowUp);
        assert_eq!(
            input.value(),
            ["2026-11-02T02:30"],
            "stepping it flips back to morning"
        );
        input.set_value(vec!["2026-11-02T00:05".into()]);
        assert_eq!(
            input.text(Part::Segment, Some("hour")).as_deref(),
            Some("12"),
            "midnight reads as twelve"
        );
    }

    #[test]
    fn seconds_are_part_of_the_value_when_they_are_shown() {
        let mut input = DateInput::new(true).with_seconds();
        type_digits(&mut input, "202611021430");
        assert!(input.value().is_empty(), "the seconds are still to come");
        type_digits(&mut input, "45");
        assert_eq!(input.value(), ["2026-11-02T14:30:45"]);
    }
}
