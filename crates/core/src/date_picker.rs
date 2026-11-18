use crate::agent::{ActError, Snapshot};
use crate::calendar::{self, Calendar};
use crate::change::Gate;
use crate::date::Date;
use crate::date_input::{self, DateInput};
use crate::keys::Key;
use crate::popup::{Align, Placement, Side};
use crate::primitive::{
    Focusable, Input, Item, PartOf, Primitive, Reaction, Relations, Role, Semantics,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// One typed piece of the date, keyed `year`, `month` or `day`.
    Segment,
    /// Opens the calendar.
    Trigger,
    /// The popup holding the calendar.
    Content,
    Grid,
    /// One day. Render one per date in `calendar().weeks()`.
    Day,
    PreviousMonth,
    NextMonth,
}

const ACTIONS: &[&str] = &["set", "clear", "open", "close"];

/// A date that can be typed or picked from a calendar. The two stay in step. The
/// value is the date as `YYYY-MM-DD`.
#[derive(Clone, Debug)]
pub struct DatePicker {
    input: DateInput,
    calendar: Calendar,
    open: bool,
    on_trigger: bool,
}

impl Default for DatePicker {
    fn default() -> Self {
        Self::new(
            calendar::Options::default(),
            Date::new(2026, 1, 1).unwrap_or(Date::FIRST),
        )
    }
}

const fn to_calendar(part: Part) -> Option<calendar::Part> {
    Some(match part {
        Part::Grid => calendar::Part::Grid,
        Part::Day => calendar::Part::Day,
        Part::PreviousMonth => calendar::Part::PreviousMonth,
        Part::NextMonth => calendar::Part::NextMonth,
        _ => return None,
    })
}

const fn from_calendar(part: calendar::Part) -> Part {
    match part {
        calendar::Part::Grid => Part::Grid,
        calendar::Part::Day => Part::Day,
        calendar::Part::PreviousMonth => Part::PreviousMonth,
        calendar::Part::NextMonth => Part::NextMonth,
    }
}

fn to_input(part: Part) -> Option<date_input::Part> {
    (part == Part::Segment).then_some(date_input::Part::Segment)
}

impl DatePicker {
    /// `showing` is the month the calendar opens on while no date is chosen.
    #[must_use]
    pub const fn new(opts: calendar::Options, showing: Date) -> Self {
        Self {
            input: DateInput::new(false),
            calendar: Calendar::new(opts, showing),
            open: false,
            on_trigger: false,
        }
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// The calendar, for its weeks and the month on show.
    #[must_use]
    pub const fn calendar(&self) -> &Calendar {
        &self.calendar
    }

    /// The typed pieces, for their order.
    #[must_use]
    pub const fn input(&self) -> &DateInput {
        &self.input
    }
}

impl PartOf for Part {
    type Primitive = DatePicker;
}

impl Primitive for DatePicker {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[
            Part::Segment,
            Part::Trigger,
            Part::Content,
            Part::Grid,
            Part::Day,
            Part::PreviousMonth,
            Part::NextMonth,
        ]
    }

    fn is_item(_: Part) -> bool {
        false
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

    fn named_part(&self) -> Option<Part> {
        Some(Part::Trigger)
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match part {
            Part::Segment => self.input.semantics(date_input::Part::Segment, item),
            Part::Trigger => Semantics {
                role: Some(Role::Button),
                expanded: Some(self.open),
                focusable: Focusable::TabStop,
                ..Default::default()
            },
            Part::Content => Semantics {
                role: Some(Role::Dialog),
                hidden: !self.open,
                ..Default::default()
            },
            other => {
                let inner = to_calendar(other)
                    .map(|part| self.calendar.semantics(part, item))
                    .unwrap_or_default();
                // Nothing in a closed calendar can be reached.
                if self.open {
                    inner
                } else {
                    Semantics {
                        hidden: true,
                        focusable: Focusable::No,
                        ..inner
                    }
                }
            }
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        if let Input::Focus { part, .. } = &input {
            self.on_trigger = *part == Part::Trigger;
        }
        match (&input, self.open) {
            (
                Input::Press {
                    part: Part::Trigger,
                    ..
                },
                _,
            ) => {
                self.open = !self.open;
                return if self.open {
                    self.calendar
                        .send(
                            Input::Key {
                                key: Key::Home,
                                mods: crate::keys::Modifiers::default(),
                                now_ms: 0,
                            },
                            &mut |_| true,
                        )
                        .for_parts(from_calendar)
                } else {
                    Reaction::handled()
                };
            }
            (
                Input::Key {
                    key: Key::Enter | Key::Space | Key::ArrowDown,
                    mods,
                    ..
                },
                false,
            ) if self.on_trigger => {
                let mods = *mods;
                return self.send(
                    Input::Press {
                        part: Part::Trigger,
                        item: None,
                        mods,
                    },
                    gate,
                );
            }
            (
                Input::Key {
                    key: Key::Escape, ..
                },
                true,
            ) => {
                self.open = false;
                return Reaction::focusing(Part::Trigger);
            }
            (Input::PressOutside | Input::WindowBlur | Input::Blur { .. }, true) => {
                self.open = false;
                return Reaction::default();
            }
            _ => {}
        }
        // Keys go to the calendar while it is open, and to the typed pieces otherwise.
        let for_calendar = match &input {
            Input::Key { .. } => self.open,
            Input::Press { part, .. } | Input::Focus { part, .. } => to_calendar(*part).is_some(),
            _ => false,
        };
        if for_calendar {
            let Some(inner) = input.for_parts(to_calendar) else {
                return Reaction::default();
            };
            let before = self.calendar.value();
            let reaction = self.calendar.send(inner, gate).for_parts(from_calendar);
            // A day was chosen: the typed pieces follow, and the calendar is done.
            if self.calendar.value() != before {
                self.input.set_value(self.calendar.value());
                self.open = false;
                return Reaction::focusing(Part::Trigger);
            }
            return reaction;
        }
        let Some(inner) = input.for_parts(to_input) else {
            return Reaction::default();
        };
        let reaction = self.input.send(inner, gate).for_parts(|_| Part::Segment);
        if !self.input.value().is_empty() {
            self.calendar.set_value(self.input.value());
        }
        reaction
    }

    fn text(&self, part: Part, item: Option<&str>) -> Option<String> {
        (part == Part::Segment)
            .then(|| self.input.text(date_input::Part::Segment, item))
            .flatten()
    }

    fn value(&self) -> Vec<String> {
        self.input.value()
    }

    fn set_value(&mut self, value: Vec<String>) {
        self.input.set_value(value.clone());
        self.calendar.set_value(value);
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "date-picker",
            label: label.to_string(),
            value: self.value(),
            expanded: Some(self.open),
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        match action {
            "open" | "close" => {
                self.open = action == "open";
                Ok(())
            }
            "set" | "clear" => {
                self.input.act(action, item, gate)?;
                if !self.input.value().is_empty() {
                    self.calendar.set_value(self.input.value());
                }
                Ok(())
            }
            other => Err(ActError::UnknownAction {
                action: other.to_string(),
                valid: ACTIONS,
            }),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::too_many_lines,
    reason = "a test fails by panicking, and its arithmetic is on values it wrote itself"
)]
mod tests {
    use super::*;
    use crate::keys::Modifiers;

    fn picker() -> DatePicker {
        DatePicker::new(Default::default(), Date::new(2026, 11, 17).unwrap())
    }

    fn key(picker: &mut DatePicker, key: Key) -> Reaction<Part> {
        picker.send(
            Input::Key {
                key,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        )
    }

    #[test]
    fn a_day_picked_from_the_calendar_fills_the_typed_pieces_and_closes_it() {
        let mut picker = picker();
        picker.send(
            Input::Press {
                part: Part::Trigger,
                item: None,
                mods: Modifiers::default(),
            },
            &mut |_| true,
        );
        assert!(picker.is_open());
        assert!(!picker.semantics(Part::Day, Some("2026-11-20")).hidden);
        let chosen = picker.send(
            Input::Press {
                part: Part::Day,
                item: Some("2026-11-20".into()),
                mods: Modifiers::default(),
            },
            &mut |_| true,
        );
        assert_eq!(picker.value(), ["2026-11-20"]);
        assert_eq!(
            picker.text(Part::Segment, Some("day")).as_deref(),
            Some("20")
        );
        assert!(!picker.is_open());
        assert_eq!(chosen.focus.unwrap().part, Part::Trigger);
    }

    #[test]
    fn a_typed_date_moves_the_calendar_to_its_month() {
        let mut picker = picker();
        for digit in "20270305".chars() {
            key(&mut picker, Key::Char(digit));
        }
        assert_eq!(picker.value(), ["2027-03-05"]);
        assert_eq!(picker.calendar().month(), (2027, 3));
    }

    #[test]
    fn the_keyboard_opens_the_calendar_walks_it_and_escape_closes_it() {
        let mut picker = picker();
        picker.send(
            Input::Focus {
                part: Part::Trigger,
                item: None,
            },
            &mut |_| true,
        );
        key(&mut picker, Key::Enter);
        assert!(picker.is_open());
        key(&mut picker, Key::ArrowRight);
        let closed = key(&mut picker, Key::Escape);
        assert!(!picker.is_open() && picker.value().is_empty());
        assert_eq!(closed.focus.unwrap().part, Part::Trigger);
        assert!(picker.semantics(Part::Day, Some("2026-11-20")).hidden);
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(picker, &[]);
    }
}
