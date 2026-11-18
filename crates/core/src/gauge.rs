//! Read-only readings: a meter, a progress bar and a separator. They take no input
//! from the person. The owner sets the value.

use crate::agent::{ActError, Snapshot};
use crate::change::Gate;
use crate::keys::Orientation;
use crate::number::{format_number, read_number};
use crate::primitive::{Input, Item, PartOf, Primitive, Reaction, Role, Semantics, ValueRange};

/// Where `value` sits between `min` and `max`, from 0 to 100. Readings outside the
/// limits are clamped.
#[must_use]
pub fn percent(value: f64, min: f64, max: f64) -> f64 {
    // A range with no room in it is full.
    if max <= min {
        return 100.0;
    }
    ((value - min) / (max - min) * 100.0).clamp(0.0, 100.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MeterPart {
    Root,
    Track,
    /// The filled length. Size it to `percent`.
    Indicator,
    Label,
    /// The reading as text. Fill it from `value_text`.
    Value,
}

/// A reading within a known range, such as storage used. The value is the number.
#[derive(Clone, Debug)]
pub struct Meter {
    value: f64,
    min: f64,
    max: f64,
}

impl Default for Meter {
    fn default() -> Self {
        Self::new(0.0, 0.0, 100.0)
    }
}

impl Meter {
    #[must_use]
    pub const fn new(value: f64, min: f64, max: f64) -> Self {
        Self { value, min, max }
    }

    /// The value as it reads: within the range, and the least value for something
    /// that is not a number.
    const fn reading(&self) -> f64 {
        if self.value.is_finite() {
            self.value.clamp(self.min, self.max)
        } else {
            self.min
        }
    }

    #[must_use]
    pub fn percent(&self) -> f64 {
        percent(self.reading(), self.min, self.max)
    }

    /// The reading as a percentage, such as `40%`.
    #[must_use]
    pub fn value_text(&self) -> String {
        format!("{}%", format_number(self.percent(), 0))
    }
}

impl PartOf for MeterPart {
    type Primitive = Meter;
}

impl Primitive for Meter {
    type Part = MeterPart;

    fn parts() -> &'static [MeterPart] {
        &[
            MeterPart::Root,
            MeterPart::Track,
            MeterPart::Indicator,
            MeterPart::Label,
            MeterPart::Value,
        ]
    }

    fn is_item(_: MeterPart) -> bool {
        false
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: MeterPart, _: Option<&str>) -> Semantics {
        match part {
            MeterPart::Root => Semantics {
                role: Some(Role::Meter),
                range: Some(ValueRange {
                    now: self.reading(),
                    min: self.min,
                    max: self.max,
                }),
                ..Default::default()
            },
            _ => Semantics::default(),
        }
    }

    fn send(&mut self, _: Input<MeterPart>, _: Gate) -> Reaction<MeterPart> {
        Reaction::default()
    }

    fn value(&self) -> Vec<String> {
        vec![self.value.to_string()]
    }

    fn set_value(&mut self, value: Vec<String>) {
        if let Some(number) = value.first().and_then(|text| text.parse().ok()) {
            self.value = number;
        }
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "meter",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: &[],
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, _: Option<&str>, _: Gate) -> Result<(), ActError> {
        Err(ActError::UnknownAction {
            action: action.to_string(),
            valid: &[],
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressState {
    /// Under way, but how far is not known.
    Indeterminate,
    Loading,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProgressPart {
    Root,
    Track,
    Indicator,
    Label,
    Value,
}

/// How far a task has come. The value is the number, or empty while that is unknown.
/// A value that is not a number counts as unknown.
#[derive(Clone, Debug)]
pub struct Progress {
    value: Option<f64>,
    min: f64,
    max: f64,
}

impl Default for Progress {
    fn default() -> Self {
        Self::new(None, 0.0, 100.0)
    }
}

impl Progress {
    #[must_use]
    pub fn new(value: Option<f64>, min: f64, max: f64) -> Self {
        Self {
            value: value.filter(|value| value.is_finite()),
            min,
            max,
        }
    }

    #[must_use]
    pub fn percent(&self) -> Option<f64> {
        self.value.map(|value| percent(value, self.min, self.max))
    }

    #[must_use]
    pub fn state(&self) -> ProgressState {
        match self.value {
            None => ProgressState::Indeterminate,
            Some(value) if value >= self.max => ProgressState::Complete,
            Some(_) => ProgressState::Loading,
        }
    }

    /// The progress as a percentage, or empty while it is unknown.
    #[must_use]
    pub fn value_text(&self) -> String {
        self.percent()
            .map(|percent| format!("{}%", format_number(percent, 0)))
            .unwrap_or_default()
    }
}

impl PartOf for ProgressPart {
    type Primitive = Progress;
}

impl Primitive for Progress {
    type Part = ProgressPart;

    fn parts() -> &'static [ProgressPart] {
        &[
            ProgressPart::Root,
            ProgressPart::Track,
            ProgressPart::Indicator,
            ProgressPart::Label,
            ProgressPart::Value,
        ]
    }

    fn is_item(_: ProgressPart) -> bool {
        false
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: ProgressPart, _: Option<&str>) -> Semantics {
        match part {
            ProgressPart::Root => Semantics {
                role: Some(Role::ProgressBar),
                range: self.value.map(|value| ValueRange {
                    now: value.clamp(self.min, self.max),
                    min: self.min,
                    max: self.max,
                }),
                ..Default::default()
            },
            _ => Semantics::default(),
        }
    }

    fn send(&mut self, _: Input<ProgressPart>, _: Gate) -> Reaction<ProgressPart> {
        Reaction::default()
    }

    fn value(&self) -> Vec<String> {
        self.value
            .iter()
            .map(|value| format_number(*value, 2))
            .collect()
    }

    fn set_value(&mut self, value: Vec<String>) {
        // Something that is not a number says nothing of how far along it is.
        self.value = value.first().and_then(|text| read_number(text));
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "progress",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: &[],
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, _: Option<&str>, _: Gate) -> Result<(), ActError> {
        Err(ActError::UnknownAction {
            action: action.to_string(),
            valid: &[],
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SeparatorPart {
    Root,
}

/// A line between groups of content. A decorative one is left out of what a screen
/// reader hears.
#[derive(Clone, Debug)]
pub struct Separator {
    orientation: Orientation,
    decorative: bool,
}

impl Default for Separator {
    fn default() -> Self {
        Self::new(Orientation::Horizontal)
    }
}

impl Separator {
    #[must_use]
    pub const fn new(orientation: Orientation) -> Self {
        Self {
            orientation,
            decorative: false,
        }
    }

    /// Only for looks: assistive technology passes over it.
    #[must_use]
    pub const fn decorative(mut self) -> Self {
        self.decorative = true;
        self
    }
}

impl PartOf for SeparatorPart {
    type Primitive = Separator;
}

impl Primitive for Separator {
    type Part = SeparatorPart;

    fn parts() -> &'static [SeparatorPart] {
        &[SeparatorPart::Root]
    }

    fn is_item(_: SeparatorPart) -> bool {
        false
    }

    /// A line names nothing.
    fn named_part(&self) -> Option<SeparatorPart> {
        None
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, _: SeparatorPart, _: Option<&str>) -> Semantics {
        Semantics {
            role: (!self.decorative).then_some(Role::Separator),
            orientation: Some(self.orientation),
            ..Default::default()
        }
    }

    fn send(&mut self, _: Input<SeparatorPart>, _: Gate) -> Reaction<SeparatorPart> {
        Reaction::default()
    }

    fn value(&self) -> Vec<String> {
        Vec::new()
    }

    fn set_value(&mut self, _: Vec<String>) {}

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "separator",
            label: label.to_string(),
            value: Vec::new(),
            expanded: None,
            actions: &[],
            items: Vec::new(),
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

    #[test]
    fn a_meter_clamps_what_it_shows_but_keeps_the_reading() {
        let mut meter = Meter::new(40.0, 0.0, 200.0);
        assert_eq!(meter.percent(), 20.0);
        assert_eq!(meter.value_text(), "20%");
        meter.set_value(vec!["250".into()]);
        assert_eq!(meter.percent(), 100.0);
        assert_eq!(
            meter.semantics(MeterPart::Root, None).range.unwrap().now,
            200.0
        );
        assert_eq!(meter.value(), ["250"]);
    }

    #[test]
    fn progress_is_unknown_under_way_or_done() {
        let mut progress = Progress::default();
        assert_eq!(progress.state(), ProgressState::Indeterminate);
        assert_eq!(progress.semantics(ProgressPart::Root, None).range, None);
        assert_eq!(progress.value_text(), "");
        progress.set_value(vec!["40".into()]);
        assert_eq!(
            (progress.state(), progress.value_text().as_str()),
            (ProgressState::Loading, "40%")
        );
        progress.set_value(vec!["100".into()]);
        assert_eq!(progress.state(), ProgressState::Complete);
    }

    #[test]
    fn a_decorative_separator_has_no_role() {
        assert_eq!(
            Separator::default()
                .semantics(SeparatorPart::Root, None)
                .role,
            Some(Role::Separator)
        );
        assert_eq!(
            Separator::new(Orientation::Vertical)
                .decorative()
                .semantics(SeparatorPart::Root, None)
                .role,
            None
        );
    }

    #[test]
    fn they_conform() {
        crate::testing::assert_conforms(Meter::default, &[]);
        crate::testing::assert_conforms(Progress::default, &[]);
        crate::testing::assert_conforms(Separator::default, &[]);
    }

    #[test]
    fn progress_that_is_not_a_number_is_unknown() {
        let mut progress = Progress::new(Some(f64::NAN), 0.0, 100.0);
        assert_eq!(progress.state(), ProgressState::Indeterminate);
        assert_eq!(progress.value_text(), "");
        assert_eq!(progress.semantics(ProgressPart::Root, None).range, None);
        progress.set_value(vec!["inf".to_string()]);
        assert_eq!(progress.state(), ProgressState::Indeterminate);
        assert_eq!(progress.value(), Vec::<String>::new());
    }
}
