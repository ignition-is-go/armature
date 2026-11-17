//! Calendar dates without time zones: arithmetic, the grid a month view shows, and how
//! keys move through it.

use crate::keys::{Dir, Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl Date {
    /// `None` for a date that does not exist, such as 30 February.
    pub fn new(year: i32, month: u8, day: u8) -> Option<Date> {
        let valid = (1..=12).contains(&month) && (1..=days_in_month(year, month)).contains(&day);
        valid.then_some(Date { year, month, day })
    }

    /// Parses `YYYY-MM-DD`.
    pub fn parse(text: &str) -> Option<Date> {
        let mut parts = text.trim().splitn(3, '-');
        let year = parts.next()?.parse().ok()?;
        let month = parts.next()?.parse().ok()?;
        let day = parts.next()?.parse().ok()?;
        Date::new(year, month, day)
    }

    pub fn format(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// Days since 1 January 1970.
    fn to_days(self) -> i64 {
        let year = self.year as i64 - (self.month <= 2) as i64;
        let era = year.div_euclid(400);
        let year_of_era = year.rem_euclid(400);
        let month = self.month as i64;
        let day_of_year =
            (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + self.day as i64 - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
    }

    fn from_days(days: i64) -> Date {
        let shifted = days + 719_468;
        let era = shifted.div_euclid(146_097);
        let day_of_era = shifted.rem_euclid(146_097);
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * day_of_year + 2) / 153;
        let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u8;
        let month = if shifted_month < 10 {
            shifted_month + 3
        } else {
            shifted_month - 9
        } as u8;
        let year = (year_of_era + era * 400 + (month <= 2) as i64) as i32;
        Date { year, month, day }
    }

    /// Day of the week, with Sunday as 0.
    pub fn weekday(self) -> u8 {
        (self.to_days() + 4).rem_euclid(7) as u8
    }

    pub fn add_days(self, days: i64) -> Date {
        Date::from_days(self.to_days() + days)
    }

    /// The same day in another month. A day the month lacks becomes its last day, so
    /// 31 January plus one month is the end of February.
    pub fn add_months(self, months: i32) -> Date {
        let index = self.year * 12 + self.month as i32 - 1 + months;
        let (year, month) = (index.div_euclid(12), index.rem_euclid(12) as u8 + 1);
        Date {
            year,
            month,
            day: self.day.min(days_in_month(year, month)),
        }
    }

    pub fn days_until(self, other: Date) -> i64 {
        other.to_days() - self.to_days()
    }
}

/// The weeks a month view shows: whole weeks starting on `week_start` (Sunday is 0),
/// padded with the neighbouring months' days.
pub fn month_grid(year: i32, month: u8, week_start: u8) -> Vec<[Date; 7]> {
    let first = Date {
        year,
        month,
        day: 1,
    };
    let lead = (first.weekday() + 7 - week_start % 7) % 7;
    let mut day = first.add_days(-(lead as i64));
    let last = Date {
        year,
        month,
        day: days_in_month(year, month),
    };
    let mut weeks = Vec::new();
    while day <= last {
        weeks.push(std::array::from_fn(|offset| day.add_days(offset as i64)));
        day = day.add_days(7);
    }
    weeks
}

/// Where a key moves the focused day in a calendar. Arrows move a day or a week,
/// Ctrl with the vertical arrows moves a month, and Home and End go to the ends of the
/// week.
pub fn date_for_key(
    date: Date,
    key: Key,
    mods: Modifiers,
    dir: Dir,
    week_start: u8,
) -> Option<Date> {
    let forward = if dir == Dir::Rtl { -1 } else { 1 };
    let into_week = ((date.weekday() + 7 - week_start % 7) % 7) as i64;
    Some(match key {
        Key::ArrowLeft => date.add_days(-forward),
        Key::ArrowRight => date.add_days(forward),
        Key::ArrowUp if mods.ctrl => date.add_months(-1),
        Key::ArrowDown if mods.ctrl => date.add_months(1),
        Key::ArrowUp => date.add_days(-7),
        Key::ArrowDown => date.add_days(7),
        Key::Home => date.add_days(-into_week),
        Key::End => date.add_days(6 - into_week),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(text: &str) -> Date {
        Date::parse(text).unwrap()
    }

    #[test]
    fn dates_that_do_not_exist_are_rejected() {
        assert_eq!(Date::new(2023, 2, 29), None);
        assert!(Date::new(2024, 2, 29).is_some());
        assert_eq!(
            Date::new(1900, 2, 29),
            None,
            "century years are not leap years"
        );
        assert!(Date::new(2000, 2, 29).is_some(), "unless divisible by 400");
        assert_eq!(Date::parse("2026-13-01"), None);
        assert_eq!(Date::parse("yesterday"), None);
    }

    #[test]
    fn weekdays_match_the_real_calendar() {
        assert_eq!(date("1970-01-01").weekday(), 4);
        assert_eq!(date("2026-11-17").weekday(), 2);
        assert_eq!(date("2000-02-29").weekday(), 2);
    }

    #[test]
    fn adding_days_crosses_months_years_and_leap_days() {
        assert_eq!(date("2024-02-28").add_days(2), date("2024-03-01"));
        assert_eq!(date("2025-12-31").add_days(1), date("2026-01-01"));
        assert_eq!(date("2026-01-01").add_days(-366), date("2024-12-31"));
        assert_eq!(date("2024-01-01").days_until(date("2025-01-01")), 366);
    }

    #[test]
    fn adding_months_clamps_to_the_end_of_a_shorter_month() {
        assert_eq!(date("2026-01-31").add_months(1), date("2026-02-28"));
        assert_eq!(date("2026-11-30").add_months(3), date("2027-02-28"));
        assert_eq!(date("2026-03-15").add_months(-15), date("2024-12-15"));
    }

    #[test]
    fn a_month_view_shows_whole_weeks_padded_by_its_neighbours() {
        let november = month_grid(2026, 11, 0);
        assert_eq!(november.len(), 5);
        assert_eq!(
            november[0][0],
            date("2026-11-01"),
            "November 2026 starts on a Sunday"
        );
        assert_eq!(november[4][6], date("2026-12-05"));
        let monday_first = month_grid(2026, 11, 1);
        assert_eq!(monday_first[0][0], date("2026-10-26"));
        assert_eq!(monday_first.len(), 6);
    }

    #[test]
    fn keys_move_by_day_week_and_month() {
        let none = Modifiers::default();
        let ctrl = Modifiers { ctrl: true, ..none };
        let day = date("2026-11-17");
        let go = |key, mods, dir| date_for_key(day, key, mods, dir, 0).unwrap();
        assert_eq!(go(Key::ArrowRight, none, Dir::Ltr), date("2026-11-18"));
        assert_eq!(go(Key::ArrowRight, none, Dir::Rtl), date("2026-11-16"));
        assert_eq!(go(Key::ArrowDown, none, Dir::Ltr), date("2026-11-24"));
        assert_eq!(go(Key::ArrowDown, ctrl, Dir::Ltr), date("2026-12-17"));
        assert_eq!(go(Key::Home, none, Dir::Ltr), date("2026-11-15"));
        assert_eq!(go(Key::End, none, Dir::Ltr), date("2026-11-21"));
        assert_eq!(date_for_key(day, Key::Enter, none, Dir::Ltr, 0), None);
    }
}
