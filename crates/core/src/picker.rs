use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::convert::Cast;
use crate::items::Items;
use crate::keys::Key;
use crate::primitive::{
    Effect, Focusable, Input, Item, PartOf, Primitive, Reaction, Role, Semantics,
};
use crate::typeahead::Typeahead;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The scrolling column. Make it a few options tall, and pad its content above
    /// and below by half its height less half an option, so the first and last
    /// options can reach the middle.
    Viewport,
    /// One option, `option_size` tall.
    Option,
}

const ACTIONS: &[&str] = &["select"];
/// How long scrolling must pause before the column settles on an option.
const REST_MS: u64 = 120;

/// A column of options that is scrolled to choose one, as a time or date wheel is.
///
/// The option in the middle is the value. It follows the scroll as it moves, and the
/// column settles on it once scrolling rests.
#[derive(Clone, Debug)]
pub struct Picker {
    items: Items,
    option_size: f64,
    selected: Option<String>,
    viewport: f64,
    /// Where the column was when it last reported.
    scrolled_from: Option<f64>,
    settle_at: Option<u64>,
    typeahead: Typeahead,
}

impl Default for Picker {
    fn default() -> Self {
        Self::new(32.0)
    }
}

impl Picker {
    #[must_use]
    pub fn new(option_size: f64) -> Self {
        Self {
            items: Items::default(),
            option_size,
            selected: None,
            viewport: 0.0,
            scrolled_from: None,
            settle_at: None,
            typeahead: Typeahead::default(),
        }
    }

    /// Where the column scrolls to for the chosen option to sit in the middle.
    #[must_use]
    pub fn offset(&self) -> f64 {
        let index = self
            .selected
            .as_deref()
            .and_then(|key| self.items.index_of(key))
            .unwrap_or(0);
        index.cast::<f64>() * self.option_size
    }

    /// The enabled option nearest an index, looking outwards from it.
    fn nearest_enabled(&self, index: usize) -> Option<String> {
        let enabled = self.items.enabled();
        let near = (0..enabled.len())
            .filter(|at| enabled.get(*at).copied().unwrap_or_default())
            .min_by_key(|at| at.abs_diff(index))?;
        Some(self.items.key_at(near))
    }

    fn choose(&mut self, key: Option<String>, source: Source, reason: Reason, gate: Gate) -> bool {
        let Some(key) = key.filter(|key| self.items.is_enabled(key)) else {
            return false;
        };
        if self.selected.as_deref() == Some(key.as_str()) {
            return true;
        }
        if !gate(&ValueChange {
            value: vec![key.clone()],
            source,
            reason,
        }) {
            return false;
        }
        self.selected = Some(key);
        true
    }

    /// Chooses the option `by` steps away, skipping disabled ones, and scrolls to it.
    fn step(&mut self, by: isize, gate: Gate) -> Reaction<Part> {
        let enabled: Vec<usize> = self
            .items
            .enabled()
            .iter()
            .enumerate()
            .filter(|(_, on)| **on)
            .map(|(at, _)| at)
            .collect();
        let Some(last) = enabled.len().checked_sub(1) else {
            return Reaction::default();
        };
        let current = self
            .selected
            .as_deref()
            .and_then(|key| self.items.index_of(key));
        let place = current.and_then(|at| enabled.iter().position(|index| *index == at));
        let next = place.map_or(0, |place| place.saturating_add_signed(by).min(last));
        self.choose(
            Some(
                self.items
                    .key_at(enabled.get(next).copied().unwrap_or_default()),
            ),
            Source::User,
            Reason::Keyboard,
            gate,
        );
        Reaction::handled().with(Effect::ScrollTo {
            offset: self.offset().cast::<u32>(),
        })
    }
}

impl PartOf for Part {
    type Primitive = Picker;
}

impl Primitive for Picker {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Viewport, Part::Option]
    }

    fn is_item(part: Part) -> bool {
        part == Part::Option
    }

    fn scrolls(part: Part) -> bool {
        part == Part::Viewport
    }

    fn wanted_scroll(&self) -> Option<f64> {
        Some(self.offset())
    }

    fn upsert_item(&mut self, item: Item) {
        self.items.upsert(item);
    }

    fn remove_item(&mut self, key: &str) {
        self.items.remove(key);
        if self.selected.as_deref() == Some(key) {
            self.selected = None;
        }
    }

    fn set_order(&mut self, rendered: &[(String, String)]) {
        self.items.set_order(rendered);
    }

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match part {
            Part::Viewport => Semantics {
                role: Some(Role::Listbox),
                focusable: Focusable::TabStop,
                ..Default::default()
            },
            Part::Option => Semantics {
                role: Some(Role::Option),
                selected: Some(item.is_some() && self.selected.as_deref() == item),
                disabled: item.is_some_and(|key| !self.items.is_enabled(key)),
                active: item.is_some() && self.selected.as_deref() == item,
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        // A page is the options on show, less one kept in view as context.
        let page = (((self.viewport / self.option_size.max(1.0)).cast::<isize>())
            .saturating_sub(1))
        .max(1);
        match input {
            Input::Scrolled {
                offset,
                viewport,
                now_ms,
                ..
            } => {
                self.viewport = viewport;
                // A report without movement only measures the column. Nobody chose.
                if !self
                    .scrolled_from
                    .replace(offset)
                    .is_some_and(|was| (was - offset).abs() > f64::EPSILON)
                {
                    return Reaction::default();
                }
                let middle = (offset / self.option_size.max(1.0))
                    .round()
                    .max(0.0)
                    .cast::<usize>();
                let nearest = self.nearest_enabled(middle);
                self.choose(nearest, Source::User, Reason::Drag, gate);
                // Each movement puts the settling off again.
                let at_ms = now_ms.saturating_add(REST_MS);
                self.settle_at = Some(at_ms);
                Reaction::default().with(Effect::Wake { at_ms })
            }
            Input::Tick { now_ms } => match self.settle_at {
                Some(at_ms) if now_ms >= at_ms => {
                    self.settle_at = None;
                    Reaction::default().with(Effect::ScrollTo {
                        offset: self.offset().cast::<u32>(),
                    })
                }
                _ => Reaction::default(),
            },
            Input::Press {
                part: Part::Option,
                item,
                ..
            } => {
                self.choose(item, Source::User, Reason::Press, gate);
                Reaction::handled().with(Effect::ScrollTo {
                    offset: self.offset().cast::<u32>(),
                })
            }
            Input::Key { key, mods, now_ms } if !mods.ctrl && !mods.alt => match key {
                Key::ArrowDown => self.step(1, gate),
                Key::ArrowUp => self.step(-1, gate),
                Key::PageDown => self.step(page, gate),
                Key::PageUp => self.step(page.saturating_neg(), gate),
                Key::Home => self.step(isize::MIN, gate),
                Key::End => self.step(isize::MAX, gate),
                Key::Char(ch) if ch != ' ' => {
                    let labels: Vec<(&str, bool)> = self
                        .items
                        .iter()
                        .map(|item| (item.label.as_str(), !item.disabled))
                        .collect();
                    let current = self
                        .selected
                        .as_deref()
                        .and_then(|key| self.items.index_of(key));
                    let Some(found) = self.typeahead.push(ch, now_ms, &labels, current) else {
                        return Reaction::default();
                    };
                    let key = self.items.key_at(found);
                    self.choose(Some(key), Source::User, Reason::Keyboard, gate);
                    Reaction::handled().with(Effect::ScrollTo {
                        offset: self.offset().cast::<u32>(),
                    })
                }
                _ => Reaction::default(),
            },
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        self.selected.iter().cloned().collect()
    }

    fn set_value(&mut self, value: Vec<String>) {
        self.selected = value
            .into_iter()
            .next()
            .filter(|key| self.items.get(key).is_some());
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .items
            .iter()
            .map(|item| SnapshotItem {
                key: item.key.clone(),
                label: item.label.clone(),
                disabled: item.disabled,
                selected: self.selected.as_deref() == Some(item.key.as_str()),
            })
            .collect();
        Snapshot {
            name: "picker",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        if action != "select" {
            return Err(ActError::UnknownAction {
                action: action.to_string(),
                valid: ACTIONS,
            });
        }
        let key = item.ok_or(ActError::MissingItem { action: "select" })?;
        match self.items.get(key).map(|item| item.disabled) {
            None => Err(ActError::UnknownItem(key.to_string())),
            Some(true) => Err(ActError::Disabled(key.to_string())),
            Some(false)
                if self.choose(Some(key.to_string()), Source::Agent, Reason::Action, gate) =>
            {
                Ok(())
            }
            Some(false) => Err(ActError::Refused),
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

    fn hours() -> Picker {
        let mut picker = Picker::new(30.0);
        for hour in 1..=12 {
            picker.upsert_item(Item {
                key: hour.to_string(),
                label: hour.to_string(),
                disabled: hour == 4,
            });
        }
        scrolled(&mut picker, 0.0, 0);
        scrolled(&mut picker, 0.0, 0);
        assert!(picker.value().is_empty(), "being measured chooses nothing");
        picker
    }

    fn scrolled(picker: &mut Picker, offset: f64, now_ms: u64) -> Reaction<Part> {
        picker.send(
            Input::Scrolled {
                part: Part::Viewport,
                offset,
                viewport: 150.0,
                content: 10_000.0,
                now_ms,
            },
            &mut |_| true,
        )
    }

    fn key(picker: &mut Picker, key: Key) -> Reaction<Part> {
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
    fn the_option_in_the_middle_is_chosen_as_the_column_scrolls() {
        let mut picker = hours();
        scrolled(&mut picker, 64.0, 0);
        assert_eq!(picker.value(), ["3"]);
        scrolled(&mut picker, 80.0, 16);
        assert_eq!(
            picker.value(),
            ["3"],
            "4 is disabled, so the nearest enabled option stands in"
        );
        scrolled(&mut picker, 140.0, 32);
        assert_eq!(picker.value(), ["6"]);
    }

    #[test]
    fn the_column_settles_on_the_choice_once_scrolling_rests() {
        let mut picker = hours();
        let moving = scrolled(&mut picker, 50.0, 0);
        assert_eq!(moving.effects, [Effect::Wake { at_ms: 120 }]);
        scrolled(&mut picker, 64.0, 100);
        let early = picker.send(Input::Tick { now_ms: 120 }, &mut |_| true);
        assert!(
            early.effects.is_empty(),
            "it moved again, so the rest starts over"
        );
        let rested = picker.send(Input::Tick { now_ms: 220 }, &mut |_| true);
        assert_eq!(rested.effects, [Effect::ScrollTo { offset: 60 }]);
    }

    #[test]
    fn keys_step_page_and_jump_past_disabled_options() {
        let mut picker = hours();
        scrolled(&mut picker, 60.0, 0);
        let down = key(&mut picker, Key::ArrowDown);
        assert_eq!(picker.value(), ["5"]);
        assert_eq!(down.effects, [Effect::ScrollTo { offset: 120 }]);
        key(&mut picker, Key::PageDown);
        assert_eq!(picker.value(), ["9"], "five fit, so a page is four");
        key(&mut picker, Key::End);
        assert_eq!(picker.value(), ["12"]);
        key(&mut picker, Key::Home);
        assert_eq!(picker.value(), ["1"]);
        key(&mut picker, Key::Char('7'));
        assert_eq!(picker.value(), ["7"]);
    }

    #[test]
    fn the_owner_can_refuse_a_choice_and_an_agent_cannot_pick_a_disabled_one() {
        let mut picker = hours();
        picker.send(
            Input::Scrolled {
                part: Part::Viewport,
                offset: 60.0,
                viewport: 150.0,
                content: 10_000.0,
                now_ms: 0,
            },
            &mut |_| false,
        );
        assert_eq!(picker.value(), [] as [std::string::String; 0]);
        assert_eq!(
            picker.act("select", Some("4"), &mut |_| true),
            Err(ActError::Disabled("4".into()))
        );
        picker.act("select", Some("8"), &mut |_| true).unwrap();
        assert_eq!(picker.wanted_scroll(), Some(210.0));
    }

    #[test]
    fn it_conforms() {
        let items: Vec<Item> = (1..=5)
            .map(|n| Item {
                key: n.to_string(),
                label: n.to_string(),
                disabled: n == 2,
            })
            .collect();
        crate::testing::assert_conforms(Picker::default, &items);
    }
}
