use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::drag::{Drag, DragEvent};
use crate::geometry::Rect;
use crate::items::Items;
use crate::keys::{Key, Orientation};
use crate::primitive::{
    Effect, Focusable, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive, Reaction,
    Role, Semantics,
};
use crate::reorder::{Reorder, ReorderSession, ReorderStep, move_item, reorder_message};
use crate::roving;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Root,
    Item,
}

const ACTIONS: &[&str] = &["move_up", "move_down", "move_to_start", "move_to_end"];

/// A list reordered by dragging, or by the keyboard: Space picks an item up, arrows
/// move it, Space drops it and Escape puts it back. The value is the keys in order.
#[derive(Clone, Debug, Default)]
pub struct Sortable {
    orientation: Orientation,
    items: Items,
    focused: Option<String>,
    session: ReorderSession,
    drag: Drag,
    /// The item under the pointer since it went down.
    pressed: Option<String>,
}

impl Sortable {
    pub fn new(orientation: Orientation) -> Self {
        Sortable {
            orientation,
            ..Default::default()
        }
    }

    /// The move in progress, for showing where the item would land.
    pub fn moving(&self) -> Option<Reorder> {
        self.session.current()
    }

    fn keys(&self) -> Vec<String> {
        self.items.iter().map(|item| item.key.clone()).collect()
    }

    fn label_at(&self, index: usize) -> String {
        self.items
            .iter()
            .nth(index)
            .map(|item| item.label.clone())
            .unwrap_or_default()
    }

    /// Applies a move if the owner allows it, and says what happened.
    fn apply(
        &mut self,
        reorder: Reorder,
        source: Source,
        reason: Reason,
        gate: Gate,
    ) -> Option<String> {
        let mut order = self.keys();
        move_item(&mut order, reorder.from, reorder.to);
        let label = self.label_at(reorder.from);
        if !gate(&ValueChange {
            value: order.clone(),
            source,
            reason,
        }) {
            return None;
        }
        let rendered: Vec<(String, String)> =
            order.into_iter().map(|key| (key, String::new())).collect();
        self.items.set_order(&rendered);
        Some(reorder_message(
            ReorderStep::Dropped,
            &label,
            reorder,
            self.items.iter().count(),
        ))
    }

    fn on_key(&mut self, key: Key, gate: Gate) -> Reaction<Part> {
        let count = self.items.iter().count();
        let grabbed = self.session.current();
        match (key, grabbed) {
            (Key::Space | Key::Enter, None) => {
                let Some(at) = self
                    .focused
                    .as_deref()
                    .and_then(|key| self.items.index_of(key))
                else {
                    return Reaction::default();
                };
                self.session.grab(at);
                let message = reorder_message(
                    ReorderStep::Grabbed,
                    &self.label_at(at),
                    Reorder { from: at, to: at },
                    count,
                );
                Reaction::handled().announcing(message)
            }
            (Key::Space | Key::Enter, Some(_)) => match self.session.drop() {
                Some(reorder) => match self.apply(reorder, Source::User, Reason::Keyboard, gate) {
                    Some(message) => Reaction::handled().announcing(message),
                    None => Reaction::handled(),
                },
                None => Reaction::handled(),
            },
            (Key::Escape, Some(reorder)) => {
                self.session.cancel();
                Reaction::handled().announcing(reorder_message(
                    ReorderStep::Cancelled,
                    &self.label_at(reorder.from),
                    reorder,
                    count,
                ))
            }
            (key, Some(reorder)) => {
                let Some(nav) = roving::nav_for_key(key, self.orientation, Default::default())
                else {
                    return Reaction::default();
                };
                let steps = match nav {
                    roving::Nav::Next => 1,
                    roving::Nav::Prev => -1,
                    roving::Nav::First => -(count as isize),
                    roving::Nav::Last => count as isize,
                };
                self.session.step(steps, count);
                let now = self.session.current().unwrap_or(reorder);
                Reaction::handled().announcing(reorder_message(
                    ReorderStep::Moved,
                    &self.label_at(now.from),
                    now,
                    count,
                ))
            }
            (key, None) => {
                let Some(nav) = roving::nav_for_key(key, self.orientation, Default::default())
                else {
                    return Reaction::default();
                };
                let target = self.items.step(self.focused.as_deref(), nav, false);
                if let Some(key) = &target {
                    self.focused = Some(key.clone());
                }
                Reaction::item(Part::Item, true, target)
            }
        }
    }
}

impl PartOf for Part {
    type Primitive = Sortable;
}

impl Primitive for Sortable {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Root, Part::Item]
    }

    fn is_item(part: Part) -> bool {
        part == Part::Item
    }

    fn tracks_pointer(part: Part) -> bool {
        part == Part::Item
    }

    fn upsert_item(&mut self, item: Item) {
        self.items.upsert(item);
    }

    fn remove_item(&mut self, key: &str) {
        let neighbour = self.items.remove(key);
        if self.focused.as_deref() == Some(key) {
            self.focused = neighbour;
        }
    }

    /// This list owns its order, so the rendered text only fills in labels.
    fn set_order(&mut self, rendered: &[(String, String)]) {
        let labels: Vec<(String, String)> = self
            .keys()
            .into_iter()
            .map(|key| {
                let text = rendered
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, text)| text.clone())
                    .unwrap_or_default();
                (key, text)
            })
            .collect();
        self.items.set_order(&labels);
    }

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        let Some(key) = item.filter(|_| part == Part::Item) else {
            return Semantics {
                role: Some(Role::List),
                orientation: Some(self.orientation),
                ..Default::default()
            };
        };
        let at = self.items.index_of(key);
        let stop = self
            .focused
            .as_deref()
            .filter(|f| self.items.get(f).is_some())
            .or_else(|| self.items.first_enabled());
        Semantics {
            role: Some(Role::ListItem),
            selected: Some(
                self.session
                    .current()
                    .is_some_and(|reorder| Some(reorder.from) == at),
            ),
            focusable: if stop == Some(key) {
                Focusable::TabStop
            } else {
                Focusable::Programmatic
            },
            active: self.focused.as_deref() == Some(key),
            ..Default::default()
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Key { key, .. } => self.on_key(key, gate),
            Input::Focus {
                item: Some(key), ..
            } => {
                self.focused = Some(key);
                Reaction::default()
            }
            _ => Reaction::default(),
        }
    }

    fn pointer(
        &mut self,
        pointer: Pointer<Part>,
        measure: Measure<Part>,
        gate: Gate,
    ) -> Reaction<Part> {
        match pointer.phase {
            PointerPhase::Down => {
                self.drag.down(pointer.at);
                self.pressed = pointer.item.clone();
                self.focused = pointer.item.clone();
                Reaction::item(Part::Item, true, pointer.item).with(Effect::CapturePointer)
            }
            PointerPhase::Move => {
                let Some(event) = self.drag.moved(pointer.at) else {
                    return Reaction::default();
                };
                if matches!(event, DragEvent::Start { .. })
                    && let Some(at) = self
                        .pressed
                        .as_deref()
                        .and_then(|key| self.items.index_of(key))
                {
                    self.session.grab(at);
                }
                let rects: Vec<Rect> = self
                    .keys()
                    .iter()
                    .map(|key| measure(Part::Item, Some(key)).unwrap_or_default())
                    .collect();
                self.session.over(&rects, pointer.at, self.orientation);
                Reaction::handled()
            }
            PointerPhase::Up => {
                self.pressed = None;
                let ended = self.drag.up(pointer.at);
                let reorder = self
                    .session
                    .drop()
                    .filter(|_| matches!(ended, Some(DragEvent::End { .. })));
                match reorder
                    .and_then(|reorder| self.apply(reorder, Source::User, Reason::Drag, gate))
                {
                    Some(message) => Reaction::handled().announcing(message),
                    None => Reaction::handled(),
                }
            }
            PointerPhase::Cancel => {
                self.pressed = None;
                self.drag.cancel();
                self.session.cancel();
                Reaction::handled()
            }
        }
    }

    fn value(&self) -> Vec<String> {
        self.keys()
    }

    fn set_value(&mut self, value: Vec<String>) {
        let rendered: Vec<(String, String)> =
            value.into_iter().map(|key| (key, String::new())).collect();
        self.items.set_order(&rendered);
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .items
            .iter()
            .map(|item| SnapshotItem {
                key: item.key.clone(),
                label: item.label.clone(),
                disabled: false,
                selected: false,
            })
            .collect();
        Snapshot {
            name: "sortable",
            label: label.to_string(),
            value: self.keys(),
            expanded: None,
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let action: &'static str = match ACTIONS.iter().find(|valid| **valid == action) {
            Some(valid) => valid,
            None => {
                return Err(ActError::UnknownAction {
                    action: action.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        let key = item.ok_or(ActError::MissingItem { action })?;
        let from = self
            .items
            .index_of(key)
            .ok_or_else(|| ActError::UnknownItem(key.to_string()))?;
        let last = self.items.iter().count() - 1;
        let to = match action {
            "move_up" => from.saturating_sub(1),
            "move_down" => (from + 1).min(last),
            "move_to_start" => 0,
            _ => last,
        };
        if from == to
            || self
                .apply(Reorder { from, to }, Source::Agent, Reason::Action, gate)
                .is_some()
        {
            Ok(())
        } else {
            Err(ActError::Refused)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;
    use crate::keys::Modifiers;

    fn list() -> Sortable {
        let mut list = Sortable::default();
        for key in ["milk", "eggs", "bread", "jam"] {
            list.upsert_item(Item {
                key: key.into(),
                label: key.into(),
                disabled: false,
            });
        }
        list
    }

    fn rows(part: Part, item: Option<&str>) -> Option<Rect> {
        let at = ["milk", "eggs", "bread", "jam"]
            .iter()
            .position(|key| Some(*key) == item)?;
        (part == Part::Item).then(|| Rect::new(0.0, at as f64 * 20.0, 100.0, 20.0))
    }

    fn pointer(
        list: &mut Sortable,
        phase: PointerPhase,
        item: &str,
        y: f64,
        gate: Gate,
    ) -> Reaction<Part> {
        let pointer = Pointer {
            phase,
            part: Part::Item,
            item: Some(item.into()),
            at: Point::new(50.0, y),
            mods: Modifiers::default(),
            now_ms: 0,
        };
        list.pointer(pointer, &rows, gate)
    }

    fn key(list: &mut Sortable, key: Key) -> Reaction<Part> {
        list.send(
            Input::Key {
                key,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        )
    }

    fn spoken(reaction: &Reaction<Part>) -> &str {
        match reaction.effects.first() {
            Some(Effect::Announce { message, .. }) => message,
            _ => "",
        }
    }

    #[test]
    fn dragging_an_item_past_others_reorders_the_list() {
        let mut list = list();
        pointer(&mut list, PointerPhase::Down, "milk", 10.0, &mut |_| true);
        pointer(&mut list, PointerPhase::Move, "milk", 55.0, &mut |_| true);
        assert_eq!(list.moving(), Some(Reorder { from: 0, to: 2 }));
        let dropped = pointer(&mut list, PointerPhase::Up, "milk", 55.0, &mut |_| true);
        assert_eq!(list.value(), ["eggs", "bread", "milk", "jam"]);
        assert_eq!(spoken(&dropped), "milk dropped at position 3 of 4.");
    }

    #[test]
    fn a_press_without_movement_only_focuses_the_item() {
        let mut list = list();
        let pressed = pointer(&mut list, PointerPhase::Down, "eggs", 30.0, &mut |_| true);
        pointer(&mut list, PointerPhase::Up, "eggs", 31.0, &mut |_| true);
        assert_eq!(pressed.focus_item(), Some("eggs"));
        assert_eq!(list.value(), ["milk", "eggs", "bread", "jam"]);
    }

    #[test]
    fn the_keyboard_picks_up_moves_and_drops_with_announcements() {
        let mut list = list();
        list.send(
            Input::Focus {
                part: Part::Item,
                item: Some("jam".into()),
            },
            &mut |_| true,
        );
        assert!(spoken(&key(&mut list, Key::Space)).starts_with("jam grabbed, position 4 of 4."));
        assert_eq!(
            spoken(&key(&mut list, Key::ArrowUp)),
            "jam moved to position 3 of 4."
        );
        key(&mut list, Key::Home);
        assert_eq!(
            spoken(&key(&mut list, Key::Space)),
            "jam dropped at position 1 of 4."
        );
        assert_eq!(list.value(), ["jam", "milk", "eggs", "bread"]);
    }

    #[test]
    fn escape_puts_the_item_back() {
        let mut list = list();
        list.send(
            Input::Focus {
                part: Part::Item,
                item: Some("milk".into()),
            },
            &mut |_| true,
        );
        key(&mut list, Key::Space);
        key(&mut list, Key::ArrowDown);
        assert!(spoken(&key(&mut list, Key::Escape)).starts_with("Reorder cancelled."));
        assert_eq!(list.value(), ["milk", "eggs", "bread", "jam"]);
    }

    #[test]
    fn arrows_move_focus_when_nothing_is_picked_up() {
        let mut list = list();
        list.send(
            Input::Focus {
                part: Part::Item,
                item: Some("milk".into()),
            },
            &mut |_| true,
        );
        assert_eq!(key(&mut list, Key::ArrowDown).focus_item(), Some("eggs"));
        assert_eq!(list.value(), ["milk", "eggs", "bread", "jam"]);
    }

    #[test]
    fn the_owner_can_refuse_a_reorder_and_agents_move_by_name() {
        let mut list = list();
        pointer(&mut list, PointerPhase::Down, "milk", 10.0, &mut |_| false);
        pointer(&mut list, PointerPhase::Move, "milk", 55.0, &mut |_| false);
        pointer(&mut list, PointerPhase::Up, "milk", 55.0, &mut |_| false);
        assert_eq!(list.value(), ["milk", "eggs", "bread", "jam"]);
        list.act("move_to_end", Some("milk"), &mut |_| true)
            .unwrap();
        list.act("move_up", Some("jam"), &mut |_| true).unwrap();
        assert_eq!(list.value(), ["eggs", "jam", "bread", "milk"]);
        assert_eq!(
            list.act("move_up", Some("eggs"), &mut |_| false),
            Ok(()),
            "already first"
        );
    }
}
