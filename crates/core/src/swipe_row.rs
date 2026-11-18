use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::convert::Cast;
use crate::geometry::Point;
use crate::gesture::VelocityTracker;
use crate::items::Items;
use crate::keys::Key;
use crate::primitive::{
    Effect, Focusable, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive, Reaction,
    Role, Semantics,
};
use crate::swipe::{Swipe, SwipeActions, is_swipe};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// The row that slides aside. Move it by `offset`.
    Row,
    /// A button behind the row.
    Action,
}

const ACTIONS: &[&str] = &["reveal", "close", "press"];

/// A row with actions behind it, revealed by swiping it aside. A long enough swipe
/// runs the first action outright. Pressing an action offers its key to the owner.
#[derive(Clone, Debug)]
pub struct SwipeRow {
    shape: SwipeActions,
    actions: Items,
    revealed: bool,
    /// Where the pointer went down, while it is down.
    start: Option<Point>,
    swiping: bool,
    dragged: f64,
    speed: VelocityTracker,
    focused: Option<String>,
}

impl SwipeRow {
    #[must_use]
    pub fn new(shape: SwipeActions) -> Self {
        Self {
            shape,
            actions: Items::default(),
            revealed: false,
            start: None,
            swiping: false,
            dragged: 0.0,
            speed: VelocityTracker::default(),
            focused: None,
        }
    }

    #[must_use]
    pub const fn is_revealed(&self) -> bool {
        self.revealed
    }

    /// How far aside to draw the row.
    #[must_use]
    pub fn offset(&self) -> f64 {
        if self.swiping {
            self.shape.offset(self.dragged)
        } else {
            self.shape.rest(if self.revealed {
                Swipe::Revealed
            } else {
                Swipe::Closed
            })
        }
    }

    fn run(&mut self, key: &str, source: Source, reason: Reason, gate: Gate) -> bool {
        let acted = gate(&ValueChange {
            value: vec![key.to_string()],
            source,
            reason,
        });
        if acted {
            self.revealed = false;
        }
        acted
    }
}

impl Default for SwipeRow {
    fn default() -> Self {
        Self::new(SwipeActions::with_full_swipe(80.0, 320.0))
    }
}

impl PartOf for Part {
    type Primitive = SwipeRow;
}

impl Primitive for SwipeRow {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Row, Part::Action]
    }

    fn is_item(part: Part) -> bool {
        part == Part::Action
    }

    fn tracks_pointer(part: Part) -> bool {
        part == Part::Row
    }

    fn upsert_item(&mut self, item: Item) {
        self.actions.upsert(item);
    }

    fn remove_item(&mut self, key: &str) {
        self.actions.remove(key);
    }

    fn set_order(&mut self, rendered: &[(String, String)]) {
        self.actions.set_order(rendered);
    }

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        match part {
            Part::Row => Semantics {
                focusable: Focusable::TabStop,
                active: self.swiping,
                ..Default::default()
            },
            Part::Action => {
                let shown = self.revealed || self.swiping;
                let enabled = item.is_some_and(|key| self.actions.is_enabled(key));
                Semantics {
                    role: Some(Role::Button),
                    hidden: !shown,
                    disabled: !enabled,
                    focusable: if self.revealed && enabled {
                        Focusable::TabStop
                    } else {
                        Focusable::No
                    },
                    ..Default::default()
                }
            }
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Focus {
                part: Part::Action,
                item,
            } => {
                self.focused = item;
                Reaction::default()
            }
            Input::Focus {
                part: Part::Row, ..
            } => {
                self.focused = None;
                Reaction::default()
            }
            Input::Press {
                part: Part::Action,
                item: Some(key),
                ..
            } if self.revealed && self.actions.is_enabled(&key) => {
                self.run(&key, Source::User, Reason::Press, gate);
                Reaction::focusing(Part::Row)
            }
            Input::Key {
                key: Key::Enter | Key::Space,
                ..
            } => match self.focused.clone() {
                Some(key) if self.revealed && self.actions.is_enabled(&key) => {
                    self.run(&key, Source::User, Reason::Keyboard, gate);
                    Reaction::focusing(Part::Row)
                }
                _ => Reaction::default(),
            },
            // The keyboard's swipe: the arrow toward the actions reveals them.
            Input::Key {
                key: Key::ArrowLeft,
                ..
            } if !self.revealed => {
                self.revealed = true;
                let first = self.actions.first_enabled().map(str::to_string);
                Reaction::item(Part::Action, true, first)
            }
            Input::Key {
                key: Key::Escape | Key::ArrowRight,
                ..
            } if self.revealed => {
                self.revealed = false;
                Reaction::focusing(Part::Row)
            }
            _ => Reaction::default(),
        }
    }

    fn pointer(&mut self, pointer: Pointer<Part>, _: Measure<Part>, gate: Gate) -> Reaction<Part> {
        match pointer.phase {
            PointerPhase::Down => {
                self.start = Some(pointer.at);
                self.swiping = false;
                self.speed = VelocityTracker::default();
                Reaction::handled().with(Effect::CapturePointer)
            }
            PointerPhase::Move => {
                let Some(start) = self.start else {
                    return Reaction::default();
                };
                // Swiping left, toward the trailing edge, uncovers the actions.
                let (dx, dy) = (start.x - pointer.at.x, pointer.at.y - start.y);
                self.swiping = self.swiping || is_swipe(dx, dy);
                if !self.swiping {
                    return Reaction::default();
                }
                let base = if self.revealed {
                    self.shape.reveal
                } else {
                    0.0
                };
                self.dragged = (base + dx).max(0.0);
                self.speed.push(pointer.now_ms.cast::<f64>(), self.dragged);
                Reaction::handled()
            }
            PointerPhase::Up => {
                let was_swiping = std::mem::take(&mut self.swiping);
                if self.start.take().is_none() {
                    return Reaction::default();
                }
                if !was_swiping {
                    // A tap on a row that is open closes it.
                    self.revealed = false;
                    return Reaction::handled();
                }
                match self.shape.settle(
                    self.dragged,
                    self.speed.velocity_at(pointer.now_ms.cast::<f64>()),
                ) {
                    Swipe::Full => {
                        self.revealed = false;
                        if let Some(first) = self.actions.first_enabled().map(str::to_string) {
                            self.run(&first, Source::User, Reason::Drag, gate);
                        }
                    }
                    Swipe::Revealed => self.revealed = true,
                    Swipe::Closed => self.revealed = false,
                }
                Reaction::handled()
            }
            PointerPhase::Cancel => {
                self.start = None;
                self.swiping = false;
                Reaction::handled()
            }
        }
    }

    fn value(&self) -> Vec<String> {
        Vec::new()
    }

    fn set_value(&mut self, _: Vec<String>) {}

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .actions
            .iter()
            .map(|item| SnapshotItem {
                key: item.key.clone(),
                label: item.label.clone(),
                disabled: item.disabled,
                selected: false,
            })
            .collect();
        Snapshot {
            name: "swipe-row",
            label: label.to_string(),
            value: Vec::new(),
            expanded: Some(self.revealed),
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        match action {
            "reveal" => self.revealed = true,
            "close" => self.revealed = false,
            "press" => {
                let key = item.ok_or(ActError::MissingItem { action: "press" })?;
                let found = self
                    .actions
                    .get(key)
                    .ok_or_else(|| ActError::UnknownItem(key.to_string()))?;
                if found.disabled {
                    return Err(ActError::Disabled(key.to_string()));
                }
                if !self.run(key, Source::Agent, Reason::Action, gate) {
                    return Err(ActError::Refused);
                }
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

    fn row() -> SwipeRow {
        let mut row = SwipeRow::default();
        for key in ["archive", "delete"] {
            row.upsert_item(Item {
                key: key.into(),
                label: key.into(),
                disabled: false,
            });
        }
        row
    }

    fn pointer(
        row: &mut SwipeRow,
        phase: PointerPhase,
        x: f64,
        now_ms: u64,
        ran: &mut Vec<String>,
    ) {
        let pointer = Pointer {
            phase,
            part: Part::Row,
            item: None,
            at: Point::new(x, 10.0),
            mods: Modifiers::default(),
            now_ms,
        };
        row.pointer(pointer, &|_, _| None, &mut |change| {
            ran.extend(change.value.iter().cloned());
            true
        });
    }

    /// Drags left by `distance` over a second, slowly enough not to be a flick.
    fn swipe(row: &mut SwipeRow, distance: f64) -> Vec<String> {
        let mut ran = Vec::new();
        pointer(row, PointerPhase::Down, 300.0, 0, &mut ran);
        pointer(
            row,
            PointerPhase::Move,
            300.0 - distance / 2.0,
            900,
            &mut ran,
        );
        pointer(row, PointerPhase::Move, 300.0 - distance, 1000, &mut ran);
        pointer(row, PointerPhase::Up, 300.0 - distance, 1000, &mut ran);
        ran
    }

    #[test]
    fn a_swipe_past_halfway_leaves_the_actions_showing() {
        let mut row = row();
        assert_eq!(swipe(&mut row, 50.0), Vec::<String>::new());
        assert!(row.is_revealed());
        assert_eq!(row.offset(), 80.0);
        assert!(!row.semantics(Part::Action, Some("delete")).hidden);
    }

    #[test]
    fn a_short_swipe_springs_back() {
        let mut row = row();
        swipe(&mut row, 20.0);
        assert!(!row.is_revealed());
        assert_eq!(row.offset(), 0.0);
    }

    #[test]
    fn a_full_swipe_runs_the_first_action_and_closes() {
        let mut row = row();
        assert_eq!(swipe(&mut row, 200.0), ["archive"]);
        assert!(!row.is_revealed());
    }

    #[test]
    fn the_row_follows_the_finger_while_it_is_down() {
        let mut row = row();
        let mut ran = Vec::new();
        pointer(&mut row, PointerPhase::Down, 300.0, 0, &mut ran);
        pointer(&mut row, PointerPhase::Move, 296.0, 10, &mut ran);
        assert_eq!(row.offset(), 0.0, "not a swipe yet");
        pointer(&mut row, PointerPhase::Move, 260.0, 20, &mut ran);
        assert_eq!(row.offset(), 40.0);
    }

    #[test]
    fn the_keyboard_reveals_presses_and_closes() {
        let mut row = row();
        let key = |row: &mut SwipeRow, key, ran: &mut Vec<String>| {
            row.send(
                Input::Key {
                    key,
                    mods: Modifiers::default(),
                    now_ms: 0,
                },
                &mut |change| {
                    ran.extend(change.value.iter().cloned());
                    true
                },
            )
        };
        let mut ran = Vec::new();
        assert_eq!(
            key(&mut row, Key::ArrowLeft, &mut ran).focus_item(),
            Some("archive")
        );
        row.send(
            Input::Focus {
                part: Part::Action,
                item: Some("delete".into()),
            },
            &mut |_| true,
        );
        key(&mut row, Key::Enter, &mut ran);
        assert_eq!(ran, ["delete"]);
        assert!(!row.is_revealed());
    }

    #[test]
    fn it_conforms() {
        let actions: Vec<Item> = ["archive", "delete"]
            .iter()
            .map(|key| Item {
                key: key.to_string(),
                label: key.to_string(),
                disabled: false,
            })
            .collect();
        crate::testing::assert_conforms(SwipeRow::default, &actions);
    }
}
