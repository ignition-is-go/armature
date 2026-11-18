use crate::agent::{ActError, Snapshot, SnapshotItem};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::geometry::Point;
use crate::keys::Key;
use crate::primitive::{
    Effect, Focusable, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive, Reaction,
    Role, Semantics,
};

/// What a notification is about, which decides how it is announced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Default,
    Success,
    /// Announced at once, interrupting what is being read.
    Error,
    /// Stays until it is replaced or dismissed.
    Loading,
}

/// One notification.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Toast {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub description: String,
    /// The label of a button that does something, such as Undo.
    pub action: String,
    /// How long it shows. `None` uses the toaster's time, and zero keeps it until
    /// dismissed.
    pub duration_ms: Option<u64>,
}

const SEPARATOR: char = '\u{1f}';

impl Toast {
    #[must_use]
    pub fn new(id: &str, title: &str) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            ..Default::default()
        }
    }

    #[must_use]
    pub const fn of(mut self, kind: Kind) -> Self {
        self.kind = kind;
        self
    }

    #[must_use]
    pub fn action(mut self, label: &str) -> Self {
        self.action = label.into();
        self
    }

    #[must_use]
    pub const fn lasting(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    /// The toast as one entry of a toaster's value.
    #[must_use]
    pub fn encode(&self) -> String {
        let kind = match self.kind {
            Kind::Default => "default",
            Kind::Success => "success",
            Kind::Error => "error",
            Kind::Loading => "loading",
        };
        let duration = self
            .duration_ms
            .map(|ms| ms.to_string())
            .unwrap_or_default();
        [
            self.id.as_str(),
            kind,
            &duration,
            &self.title,
            &self.description,
            &self.action,
        ]
        .join(&SEPARATOR.to_string())
    }

    #[must_use]
    pub fn parse(entry: &str) -> Option<Self> {
        let mut fields = entry.split(SEPARATOR);
        let id = fields.next()?.to_string();
        let kind = match fields.next()? {
            "success" => Kind::Success,
            "error" => Kind::Error,
            "loading" => Kind::Loading,
            _ => Kind::Default,
        };
        let duration_ms = fields.next()?.parse().ok();
        let mut text = || fields.next().unwrap_or_default().to_string();
        Some(Self {
            id,
            kind,
            duration_ms,
            title: text(),
            description: text(),
            action: text(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    /// Where the notifications stack. The pointer or focus inside it stops the clocks.
    Region,
    /// One notification, keyed by its id. Dragging it sideways dismisses it.
    Toast,
    Title,
    Description,
    Action,
    Close,
}

const ACTIONS: &[&str] = &["dismiss", "action", "dismiss_all"];
/// How far sideways a notification is dragged before letting go dismisses it.
const SWIPE: f64 = 60.0;

/// A notification being swiped away.
#[derive(Clone, Debug)]
struct Swipe {
    id: String,
    /// Where the pointer went down.
    from: Point,
    /// How far sideways it has gone since.
    moved: f64,
}

/// A stack of notifications.
///
/// The value is the notifications, each written with `Toast::encode`, oldest first: the
/// owner adds to it, and the toaster offers the list without a notification when its
/// time is up or it is dismissed. Pressing an action offers `action:<id>` first.
#[derive(Clone, Debug)]
pub struct Toaster {
    toasts: Vec<Toast>,
    /// How many show at once. The rest wait, with their clocks stopped.
    limit: usize,
    duration_ms: u64,
    /// When each showing notification's time is up.
    deadlines: Vec<(String, u64)>,
    /// What each had left when the clocks were stopped.
    paused: Option<Vec<(String, u64)>>,
    hovered: bool,
    focused: Option<(Part, String)>,
    /// The notification being dragged, where the drag began and how far it has gone.
    drag: Option<Swipe>,
}

impl Default for Toaster {
    fn default() -> Self {
        Self::new(3, 5000)
    }
}

impl Toaster {
    #[must_use]
    pub const fn new(limit: usize, duration_ms: u64) -> Self {
        Self {
            toasts: Vec::new(),
            limit,
            duration_ms,
            deadlines: Vec::new(),
            paused: None,
            hovered: false,
            focused: None,
            drag: None,
        }
    }

    /// The notifications on show, oldest first.
    #[must_use]
    pub fn visible(&self) -> &[Toast] {
        self.toasts
            .get(..self.toasts.len().min(self.limit))
            .unwrap_or_default()
    }

    /// How far a notification being dragged has moved sideways.
    #[must_use]
    pub fn offset(&self, id: &str) -> f64 {
        self.drag
            .as_ref()
            .filter(|swipe| swipe.id == id)
            .map_or(0.0, |swipe| swipe.moved)
    }

    fn lasts(&self, toast: &Toast) -> Option<u64> {
        let duration = toast.duration_ms.unwrap_or(if toast.kind == Kind::Loading {
            0
        } else {
            self.duration_ms
        });
        (duration > 0).then_some(duration)
    }

    fn unstarted(&self) -> Vec<&Toast> {
        let timed = |toast: &&Toast| {
            self.lasts(toast).is_some() && !self.deadlines.iter().any(|(id, _)| *id == toast.id)
        };
        self.visible().iter().filter(timed).collect()
    }

    fn remove(&mut self, ids: &[String], source: Source, reason: Reason, gate: Gate) -> bool {
        let next: Vec<Toast> = self
            .toasts
            .iter()
            .filter(|toast| !ids.contains(&toast.id))
            .cloned()
            .collect();
        if next.len() == self.toasts.len() {
            return true;
        }
        let allowed = gate(&ValueChange {
            value: next.iter().map(Toast::encode).collect(),
            source,
            reason,
        });
        if allowed {
            self.toasts = next;
            self.deadlines.retain(|(id, _)| !ids.contains(id));
        }
        allowed
    }

    /// Stops the clocks while someone is reading, and starts them again after.
    fn hold(&mut self, hold: bool, now_ms: u64) -> Reaction<Part> {
        match (hold, self.paused.take()) {
            (true, None) => {
                self.paused = Some(
                    self.deadlines
                        .drain(..)
                        .map(|(id, at)| (id, at.saturating_sub(now_ms)))
                        .collect(),
                );
                Reaction::default()
            }
            (true, already) => {
                self.paused = already;
                Reaction::default()
            }
            (false, Some(left)) => {
                self.deadlines = left
                    .into_iter()
                    .map(|(id, left)| (id, now_ms.saturating_add(left)))
                    .collect();
                self.wake()
            }
            (false, None) => Reaction::default(),
        }
    }

    fn wake(&self) -> Reaction<Part> {
        self.deadlines
            .iter()
            .map(|(_, at)| *at)
            .min()
            .map_or_else(Reaction::default, |at_ms| {
                Reaction::default().with(Effect::Wake { at_ms })
            })
    }
}

impl Toaster {
    /// Time passing: clocks are held or resumed, and notifications whose time is up leave.
    fn on_tick(&mut self, now_ms: u64, gate: Gate) -> Reaction<Part> {
        // Reading with the pointer and with the keyboard hold the clocks
        // separately: they run again only once neither does.
        let held = self.hovered || self.focused.is_some();
        if held != self.paused.is_some() {
            let resumed = self.hold(held, now_ms);
            if held {
                return resumed;
            }
        }
        if self.paused.is_some() {
            return Reaction::default();
        }
        let due: Vec<String> = self
            .deadlines
            .iter()
            .filter(|(_, at)| now_ms >= *at)
            .map(|(id, _)| id.clone())
            .collect();
        if !self.remove(&due, Source::User, Reason::Timer, gate) {
            // The owner kept them, so they get their time again.
            self.deadlines.retain(|(id, _)| !due.contains(id));
        }
        // Whatever has just come into view starts its clock and is read out.
        let mut reaction = Reaction::default();
        let started: Vec<(String, u64, String, bool)> = self
            .unstarted()
            .into_iter()
            .map(|toast| {
                (
                    toast.id.clone(),
                    self.lasts(toast).unwrap_or(0),
                    toast.title.clone(),
                    toast.kind == Kind::Error,
                )
            })
            .collect();
        for (id, lasts, title, assertive) in started {
            self.deadlines.push((id, now_ms.saturating_add(lasts)));
            reaction = reaction.with(Effect::Announce {
                message: title,
                assertive,
            });
        }
        let wake = self.wake();
        reaction.effects.extend(wake.effects);
        reaction
    }
}

impl PartOf for Part {
    type Primitive = Toaster;
}

impl Primitive for Toaster {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[
            Part::Region,
            Part::Toast,
            Part::Title,
            Part::Description,
            Part::Action,
            Part::Close,
        ]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn hovers(part: Part) -> bool {
        part == Part::Region
    }

    fn tracks_pointer(part: Part) -> bool {
        part == Part::Toast
    }

    fn named_part(&self) -> Option<Part> {
        Some(Part::Region)
    }

    /// F6 moves focus into the stack and back, so the keyboard can reach a
    /// notification without tabbing through the page.
    fn hotkeys(&self) -> Vec<crate::shortcut::Shortcut> {
        crate::shortcut::Shortcut::parse("f6").into_iter().collect()
    }

    fn wake_after(&self) -> Option<u64> {
        let held = self.hovered || self.focused.is_some();
        (held != self.paused.is_some() || (self.paused.is_none() && !self.unstarted().is_empty()))
            .then_some(0)
    }

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, item: Option<&str>) -> Semantics {
        let toast = item.and_then(|id| self.visible().iter().find(|toast| toast.id == id));
        let button = |shown: bool| Semantics {
            role: Some(Role::Button),
            hidden: !shown,
            focusable: if shown {
                Focusable::TabStop
            } else {
                Focusable::No
            },
            ..Default::default()
        };
        match (part, toast) {
            (Part::Region, _) => Semantics {
                role: Some(Role::Region),
                ..Default::default()
            },
            (Part::Toast, Some(toast)) => Semantics {
                role: Some(if toast.kind == Kind::Error {
                    Role::Alert
                } else {
                    Role::Status
                }),
                active: self.drag.as_ref().is_some_and(|swipe| swipe.id == toast.id),
                ..Default::default()
            },
            (Part::Title, Some(_)) => Semantics::default(),
            (Part::Description, Some(toast)) => Semantics {
                hidden: toast.description.is_empty(),
                ..Default::default()
            },
            (Part::Action, Some(toast)) => button(!toast.action.is_empty()),
            (Part::Close, Some(_)) => button(true),
            // A notification waiting its turn, or one that is gone.
            (_, None) => Semantics {
                hidden: true,
                ..Default::default()
            },
        }
    }
    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Tick { now_ms } => self.on_tick(now_ms, gate),
            Input::Hover {
                part: Part::Region,
                inside,
                now_ms,
                ..
            } => {
                self.hovered = inside;
                self.hold(inside || self.focused.is_some(), now_ms)
            }
            Input::Focus {
                part,
                item: Some(id),
            } => {
                self.focused = Some((part, id));
                Reaction::default()
            }
            Input::Blur { .. } => {
                self.focused = None;
                Reaction::default()
            }
            Input::Hotkey { .. } => match (&self.focused, self.visible().last()) {
                (Some(_), _) => {
                    self.focused = None;
                    Reaction::handled().with(Effect::RestoreFocus)
                }
                // The newest notification is the one most likely wanted.
                (None, Some(toast)) => Reaction::item(Part::Close, true, Some(toast.id.clone())),
                (None, None) => Reaction::default(),
            },
            Input::Press {
                part: Part::Close,
                item: Some(id),
                ..
            } => {
                self.remove(&[id], Source::User, Reason::Press, gate);
                Reaction::handled()
            }
            Input::Press {
                part: Part::Action,
                item: Some(id),
                ..
            } => {
                if gate(&ValueChange {
                    value: vec![format!("action:{id}")],
                    source: Source::User,
                    reason: Reason::Press,
                }) {
                    self.remove(&[id], Source::User, Reason::Press, gate);
                }
                Reaction::handled()
            }
            Input::Key {
                key: Key::Enter | Key::Space,
                mods,
                ..
            } => match self.focused.clone() {
                Some((part @ (Part::Action | Part::Close), id)) => self.send(
                    Input::Press {
                        part,
                        item: Some(id),
                        mods,
                    },
                    gate,
                ),
                _ => Reaction::default(),
            },
            // Escape dismisses the notification that holds focus.
            Input::Key {
                key: Key::Escape, ..
            } => match self.focused.take() {
                Some((_, id)) => {
                    self.remove(&[id], Source::User, Reason::Keyboard, gate);
                    Reaction::handled()
                }
                None => Reaction::default(),
            },
            _ => Reaction::default(),
        }
    }

    fn pointer(&mut self, pointer: Pointer<Part>, _: Measure<Part>, gate: Gate) -> Reaction<Part> {
        match (pointer.phase, pointer.item) {
            (PointerPhase::Down, Some(id)) => {
                self.drag = Some(Swipe {
                    id,
                    from: pointer.at,
                    moved: 0.0,
                });
                Reaction::default().with(Effect::CapturePointer)
            }
            (PointerPhase::Move, _) => {
                if let Some(swipe) = &mut self.drag {
                    swipe.moved = pointer.at.x - swipe.from.x;
                }
                Reaction::default()
            }
            (PointerPhase::Up, _) => {
                if let Some(swipe) = self.drag.take()
                    && swipe.moved.abs() >= SWIPE
                {
                    self.remove(&[swipe.id], Source::User, Reason::Drag, gate);
                }
                Reaction::default()
            }
            _ => {
                self.drag = None;
                Reaction::default()
            }
        }
    }

    fn value(&self) -> Vec<String> {
        self.toasts.iter().map(Toast::encode).collect()
    }

    fn set_value(&mut self, value: Vec<String>) {
        self.toasts = value
            .iter()
            .filter_map(|entry| Toast::parse(entry))
            .collect();
        let ids: Vec<&String> = self.toasts.iter().map(|toast| &toast.id).collect();
        self.deadlines.retain(|(id, _)| ids.contains(&id));
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        let items = self
            .toasts
            .iter()
            .map(|toast| SnapshotItem {
                key: toast.id.clone(),
                label: toast.title.clone(),
                disabled: false,
                selected: false,
            })
            .collect();
        Snapshot {
            name: "toaster",
            label: label.to_string(),
            value: self.value(),
            expanded: None,
            actions: ACTIONS,
            items,
        }
    }

    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let action: &'static str = match action {
            "dismiss_all" => {
                let all: Vec<String> = self.toasts.iter().map(|toast| toast.id.clone()).collect();
                return self
                    .remove(&all, Source::Agent, Reason::Action, gate)
                    .then_some(())
                    .ok_or(ActError::Refused);
            }
            "dismiss" => "dismiss",
            "action" => "action",
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        let id = item.ok_or(ActError::MissingItem { action })?;
        let toast = self
            .toasts
            .iter()
            .find(|toast| toast.id == id)
            .ok_or_else(|| ActError::UnknownItem(id.to_string()))?;
        if action == "action" {
            let offered = ValueChange {
                value: vec![format!("action:{id}")],
                source: Source::Agent,
                reason: Reason::Action,
            };
            if toast.action.is_empty() || !gate(&offered) {
                return Err(ActError::Refused);
            }
        }
        if self.remove(&[id.to_string()], Source::Agent, Reason::Action, gate) {
            Ok(())
        } else {
            Err(ActError::Refused)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Modifiers;

    fn toaster(toasts: &[Toast]) -> Toaster {
        let mut toaster = Toaster::default();
        toaster.set_value(toasts.iter().map(Toast::encode).collect());
        toaster
    }

    fn tick(toaster: &mut Toaster, now_ms: u64) -> Reaction<Part> {
        toaster.send(Input::Tick { now_ms }, &mut |_| true)
    }

    fn ids(toaster: &Toaster) -> Vec<&str> {
        toaster
            .toasts
            .iter()
            .map(|toast| toast.id.as_str())
            .collect()
    }

    #[test]
    fn a_toast_is_read_out_when_it_shows_and_leaves_when_its_time_is_up() {
        let mut toaster = toaster(&[
            Toast::new("a", "Saved"),
            Toast::new("b", "Upload failed")
                .of(Kind::Error)
                .lasting(8000),
        ]);
        assert_eq!(
            toaster.wake_after(),
            Some(0),
            "set from outside, so the clocks have yet to start"
        );
        let shown = tick(&mut toaster, 1000);
        assert_eq!(
            shown.effects,
            [
                Effect::Announce {
                    message: "Saved".into(),
                    assertive: false
                },
                Effect::Announce {
                    message: "Upload failed".into(),
                    assertive: true
                },
                Effect::Wake { at_ms: 6000 },
            ]
        );
        assert_eq!(toaster.wake_after(), None);
        tick(&mut toaster, 6000);
        assert_eq!(ids(&toaster), ["b"]);
        tick(&mut toaster, 9000);
        assert_eq!(ids(&toaster), Vec::<&str>::new());
    }

    #[test]
    fn only_so_many_show_and_the_rest_wait_with_their_clocks_stopped() {
        let many: Vec<Toast> = ["a", "b", "c", "d"]
            .iter()
            .map(|id| Toast::new(id, id))
            .collect();
        let mut toaster = toaster(&many);
        tick(&mut toaster, 0);
        assert_eq!(toaster.visible().len(), 3);
        assert!(toaster.semantics(Part::Toast, Some("d")).hidden);
        tick(&mut toaster, 5000);
        assert_eq!(ids(&toaster), ["d"]);
        tick(&mut toaster, 9000);
        assert_eq!(
            ids(&toaster),
            ["d"],
            "its five seconds began when it came into view"
        );
        tick(&mut toaster, 10_000);
        assert_eq!(ids(&toaster), Vec::<&str>::new());
    }

    #[test]
    fn the_pointer_resting_on_the_stack_stops_the_clocks() {
        let mut toaster = toaster(&[Toast::new("a", "Saved")]);
        tick(&mut toaster, 0);
        toaster.send(
            Input::Hover {
                part: Part::Region,
                item: None,
                inside: true,
                now_ms: 4000,
            },
            &mut |_| true,
        );
        tick(&mut toaster, 20_000);
        assert_eq!(ids(&toaster), ["a"]);
        let resumed = toaster.send(
            Input::Hover {
                part: Part::Region,
                item: None,
                inside: false,
                now_ms: 20_000,
            },
            &mut |_| true,
        );
        assert_eq!(
            resumed.effects,
            [Effect::Wake { at_ms: 21_000 }],
            "the second it had left"
        );
    }

    #[test]
    fn an_action_is_offered_before_the_toast_goes_and_a_loading_toast_stays() {
        let mut toaster = toaster(&[
            Toast::new("a", "Deleted").action("Undo"),
            Toast::new("b", "Uploading").of(Kind::Loading),
        ]);
        tick(&mut toaster, 0);
        let mut offered = Vec::new();
        toaster.send(
            Input::Press {
                part: Part::Action,
                item: Some("a".into()),
                mods: Modifiers::default(),
            },
            &mut |change| {
                offered.push(change.value.first().cloned().unwrap_or_default());
                true
            },
        );
        assert_eq!(offered[0], "action:a");
        tick(&mut toaster, 60_000);
        assert_eq!(ids(&toaster), ["b"]);
    }

    #[test]
    fn dragging_a_toast_far_enough_dismisses_it() {
        let mut toaster = toaster(&[Toast::new("a", "Saved")]);
        let drag = |toaster: &mut Toaster, phase, x: f64| {
            let pointer = Pointer {
                phase,
                part: Part::Toast,
                item: Some("a".into()),
                at: Point::new(x, 0.0),
                mods: Modifiers::default(),
                now_ms: 0,
            };
            toaster.pointer(pointer, &|_, _| None, &mut |_| true);
        };
        drag(&mut toaster, PointerPhase::Down, 0.0);
        drag(&mut toaster, PointerPhase::Move, 30.0);
        assert_eq!(toaster.offset("a"), 30.0);
        drag(&mut toaster, PointerPhase::Up, 30.0);
        assert_eq!(ids(&toaster), ["a"], "not far enough");
        drag(&mut toaster, PointerPhase::Down, 0.0);
        drag(&mut toaster, PointerPhase::Move, 90.0);
        drag(&mut toaster, PointerPhase::Up, 90.0);
        assert_eq!(ids(&toaster), Vec::<&str>::new());
    }

    #[test]
    fn the_hotkey_moves_focus_into_the_stack_and_back() {
        let mut toaster = toaster(&[Toast::new("a", "Saved"), Toast::new("b", "Sent")]);
        assert_eq!(toaster.hotkeys().first().map(|key| key.key), Some(Key::F6));
        let entered = toaster.send(
            Input::Hotkey {
                shortcut: toaster.hotkeys()[0],
            },
            &mut |_| true,
        );
        assert_eq!(
            (
                entered.focus.as_ref().map(|target| target.part),
                entered.focus_item()
            ),
            (Some(Part::Close), Some("b"))
        );
        toaster.send(
            Input::Focus {
                part: Part::Close,
                item: Some("b".into()),
            },
            &mut |_| true,
        );
        let left = toaster.send(
            Input::Hotkey {
                shortcut: toaster.hotkeys()[0],
            },
            &mut |_| true,
        );
        assert_eq!(left.effects, [Effect::RestoreFocus]);
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(
            || {
                toaster(&[
                    Toast::new("a", "Saved").action("Undo"),
                    Toast::new("b", "Sent"),
                ])
            },
            &[],
        );
    }
}
