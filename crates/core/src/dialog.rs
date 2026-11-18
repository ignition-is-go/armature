use crate::agent::{ActError, Snapshot};
use crate::change::{Gate, Reason, Source, ValueChange};
use crate::keys::Key;
use crate::primitive::{
    Focusable, Input, Item, Measure, PartOf, Pointer, PointerPhase, Primitive, Reaction, Relations,
    Role, Semantics,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Trigger,
    /// Covers the page behind the dialog. Pressing it dismisses the dialog.
    Backdrop,
    /// The dialog itself. Tab stays inside it while it is open.
    Content,
    Close,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub dismiss_on_backdrop: bool,
    pub dismiss_on_escape: bool,
    /// Whether it interrupts with something that must be answered, such as
    /// confirming a deletion. Assistive technology then announces it at once.
    pub alert: bool,
    /// Whether Escape and a press outside may close it. When they may not, each
    /// try is counted in `attempts`, so the owner can say why it stays.
    pub dismissible: bool,
    /// Whether the page behind is out of reach while it is open. A dialog that is
    /// not modal leaves the page usable, so a press beside it is the page's.
    pub modal: bool,
}

/// What asked a dialog to close.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dismiss {
    Escape,
    OutsidePress,
    /// The platform's back gesture or button.
    Back,
    ClosePress,
    TriggerPress,
    /// The owner or an agent.
    Controller,
}

/// Why a dialog stayed open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Someone tried to dismiss it on purpose. Worth telling the owner.
    Attempt,
    /// A press that was meant for the page beside it. Nothing to report.
    Silent,
}

impl Options {
    const fn is_modal(self) -> bool {
        self.modal || self.alert
    }

    /// Whether a way of closing is refused, and whether that counts as a try.
    /// Close buttons, triggers and the controller always close.
    #[must_use]
    pub const fn refusal(&self, dismiss: Dismiss) -> Option<Refusal> {
        match dismiss {
            Dismiss::ClosePress | Dismiss::TriggerPress | Dismiss::Controller => None,
            Dismiss::OutsidePress
                if !(self.is_modal() || self.dismiss_on_backdrop && self.dismissible) =>
            {
                Some(Refusal::Silent)
            }
            Dismiss::OutsidePress
                if self.alert || !self.dismiss_on_backdrop || !self.dismissible =>
            {
                Some(Refusal::Attempt)
            }
            Dismiss::Escape if !self.dismiss_on_escape || !self.dismissible => {
                Some(Refusal::Attempt)
            }
            Dismiss::Back if !self.dismissible => Some(Refusal::Attempt),
            _ => None,
        }
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            dismiss_on_backdrop: true,
            dismiss_on_escape: true,
            alert: false,
            dismissible: true,
            modal: true,
        }
    }
}

const ACTIONS: &[&str] = &["open", "close"];
const OPEN: &str = "open";

/// A modal dialog. Its value is `["open"]` while it is open.
#[derive(Clone, Debug, Default)]
pub struct Dialog {
    opts: Options,
    open: bool,
    focused: Option<Part>,
    attempts: u32,
}

impl Dialog {
    #[must_use]
    pub const fn new(opts: Options) -> Self {
        Self {
            opts,
            open: false,
            focused: None,
            attempts: 0,
        }
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// How many times someone has tried to dismiss it and been refused by its
    /// options. Watch it to nudge the dialog or explain why it stays.
    #[must_use]
    pub const fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Closes for `dismiss` if the options allow it, and counts the try if not.
    fn try_dismiss(&mut self, dismiss: Dismiss, reason: Reason, gate: Gate) -> Reaction<Part> {
        match self.opts.refusal(dismiss) {
            None => self.dismiss(reason, gate),
            Some(Refusal::Attempt) => {
                self.attempts = self.attempts.saturating_add(1);
                Reaction::handled()
            }
            Some(Refusal::Silent) => Reaction::default(),
        }
    }

    fn set(&mut self, open: bool, source: Source, reason: Reason, gate: Gate) -> bool {
        if open == self.open {
            return true;
        }
        let value = if open {
            vec![OPEN.to_string()]
        } else {
            Vec::new()
        };
        let allowed = gate(&ValueChange {
            value,
            source,
            reason,
        });
        if allowed {
            self.open = open;
        }
        allowed
    }

    /// Closes and hands focus back to the trigger, unless the owner refuses.
    fn dismiss(&mut self, reason: Reason, gate: Gate) -> Reaction<Part> {
        if self.set(false, Source::User, reason, gate) {
            Reaction::focusing(Part::Trigger)
        } else {
            Reaction::handled()
        }
    }
}

impl PartOf for Part {
    type Primitive = Dialog;
}

impl Primitive for Dialog {
    type Part = Part;

    fn parts() -> &'static [Part] {
        &[Part::Trigger, Part::Backdrop, Part::Content, Part::Close]
    }

    fn is_item(_: Part) -> bool {
        false
    }

    fn tracks_pointer(part: Part) -> bool {
        part == Part::Backdrop
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

    fn upsert_item(&mut self, _: Item) {}
    fn remove_item(&mut self, _: &str) {}
    fn set_order(&mut self, _: &[(String, String)]) {}

    fn semantics(&self, part: Part, _: Option<&str>) -> Semantics {
        let hidden = !self.open;
        match part {
            Part::Trigger => Semantics {
                role: Some(Role::Button),
                // Only a dialog that leaves the page usable reads as expanded.
                expanded: (!self.opts.is_modal()).then_some(self.open),
                focusable: Focusable::TabStop,
                ..Default::default()
            },
            Part::Backdrop => Semantics {
                hidden,
                ..Default::default()
            },
            Part::Content => Semantics {
                role: Some(if self.opts.alert {
                    Role::AlertDialog
                } else {
                    Role::Dialog
                }),
                hidden,
                modal: self.opts.is_modal(),
                focusable: Focusable::Programmatic,
                ..Default::default()
            },
            Part::Close => Semantics {
                role: Some(Role::Button),
                hidden,
                focusable: if hidden {
                    Focusable::No
                } else {
                    Focusable::TabStop
                },
                ..Default::default()
            },
        }
    }

    fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part> {
        match input {
            Input::Focus { part, .. } => {
                self.focused = Some(part);
                Reaction::default()
            }
            // The trigger and the close button are buttons, so Enter and Space press them.
            Input::Key {
                key: Key::Enter | Key::Space,
                mods,
                ..
            } => match self.focused {
                Some(part @ (Part::Trigger | Part::Close)) => self.send(
                    Input::Press {
                        part,
                        item: None,
                        mods,
                    },
                    gate,
                ),
                _ => Reaction::default(),
            },
            Input::Press {
                part: Part::Trigger,
                ..
            } if !self.open => {
                if self.set(true, Source::User, Reason::Press, gate) {
                    Reaction::focusing(Part::Content)
                } else {
                    Reaction::handled()
                }
            }
            Input::Press {
                part: Part::Trigger | Part::Close,
                ..
            } => self.dismiss(Reason::Press, gate),
            Input::Key {
                key: Key::Escape, ..
            } if self.open => self.try_dismiss(Dismiss::Escape, Reason::Keyboard, gate),
            Input::PressOutside if self.open => {
                self.try_dismiss(Dismiss::OutsidePress, Reason::Press, gate)
            }
            _ => Reaction::default(),
        }
    }

    fn pointer(&mut self, pointer: Pointer<Part>, _: Measure<Part>, gate: Gate) -> Reaction<Part> {
        match pointer.phase {
            PointerPhase::Down if self.open => {
                self.try_dismiss(Dismiss::OutsidePress, Reason::Press, gate)
            }
            _ => Reaction::default(),
        }
    }

    fn value(&self) -> Vec<String> {
        if self.open {
            vec![OPEN.to_string()]
        } else {
            Vec::new()
        }
    }

    fn set_value(&mut self, value: Vec<String>) {
        self.open = value.iter().any(|v| v == OPEN);
    }

    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            name: "dialog",
            label: label.to_string(),
            value: self.value(),
            expanded: Some(self.open),
            actions: ACTIONS,
            items: Vec::new(),
        }
    }

    fn act(&mut self, action: &str, _: Option<&str>, gate: Gate) -> Result<(), ActError> {
        let open = match action {
            "open" => true,
            "close" => false,
            other => {
                return Err(ActError::UnknownAction {
                    action: other.to_string(),
                    valid: ACTIONS,
                });
            }
        };
        if self.set(open, Source::Agent, Reason::Action, gate) {
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
    use crate::primitive::FocusTarget;

    fn press(dialog: &mut Dialog, part: Part) -> Reaction<Part> {
        dialog.send(
            Input::Press {
                part,
                item: None,
                mods: Modifiers::default(),
            },
            &mut |_| true,
        )
    }

    fn opened(opts: Options) -> Dialog {
        let mut dialog = Dialog::new(opts);
        press(&mut dialog, Part::Trigger);
        dialog
    }

    fn backdrop(dialog: &mut Dialog) -> Reaction<Part> {
        let pointer = Pointer {
            phase: PointerPhase::Down,
            part: Part::Backdrop,
            item: None,
            at: Point::default(),
            mods: Modifiers::default(),
            now_ms: 0,
        };
        dialog.pointer(pointer, &|_, _| None, &mut |_| true)
    }

    const TO_TRIGGER: Option<FocusTarget<Part>> = Some(FocusTarget {
        part: Part::Trigger,
        item: None,
    });

    #[test]
    fn opening_shows_the_dialog_and_moves_focus_into_it() {
        let mut dialog = Dialog::default();
        assert!(dialog.semantics(Part::Content, None).hidden);
        let opened = press(&mut dialog, Part::Trigger);
        assert_eq!(opened.focus.unwrap().part, Part::Content);
        let content = dialog.semantics(Part::Content, None);
        assert!(!content.hidden && content.modal);
        assert_eq!(dialog.semantics(Part::Trigger, None).expanded, None);
    }

    #[test]
    fn escape_the_close_button_and_the_backdrop_all_return_focus_to_the_trigger() {
        let escape = Input::Key {
            key: Key::Escape,
            mods: Modifiers::default(),
            now_ms: 0,
        };
        let mut dialog = opened(Options::default());
        assert_eq!(dialog.send(escape, &mut |_| true).focus, TO_TRIGGER);
        assert!(!dialog.is_open());

        let mut dialog = opened(Options::default());
        assert_eq!(press(&mut dialog, Part::Close).focus, TO_TRIGGER);

        let mut dialog = opened(Options::default());
        assert_eq!(backdrop(&mut dialog).focus, TO_TRIGGER);
        assert!(!dialog.is_open());
    }

    #[test]
    fn a_dialog_that_must_be_answered_ignores_escape_and_the_backdrop() {
        let mut dialog = opened(Options {
            dismiss_on_backdrop: false,
            dismiss_on_escape: false,
            alert: true,
            ..Default::default()
        });
        assert_eq!(
            dialog.semantics(Part::Content, None).role,
            Some(Role::AlertDialog)
        );
        dialog.send(
            Input::Key {
                key: Key::Escape,
                mods: Modifiers::default(),
                now_ms: 0,
            },
            &mut |_| true,
        );
        backdrop(&mut dialog);
        assert!(dialog.is_open());
        press(&mut dialog, Part::Close);
        assert!(!dialog.is_open());
    }

    #[test]
    fn the_owner_can_keep_it_open() {
        let mut dialog = opened(Options::default());
        let kept = dialog.send(
            Input::Press {
                part: Part::Close,
                item: None,
                mods: Modifiers::default(),
            },
            &mut |_| false,
        );
        assert!(dialog.is_open());
        assert_eq!(kept.focus, None);
    }

    #[test]
    fn enter_on_the_focused_trigger_or_close_button_presses_it() {
        let enter = || Input::Key {
            key: Key::Enter,
            mods: Modifiers::default(),
            now_ms: 0,
        };
        let mut dialog = Dialog::default();
        dialog.send(
            Input::Focus {
                part: Part::Trigger,
                item: None,
            },
            &mut |_| true,
        );
        dialog.send(enter(), &mut |_| true);
        assert!(dialog.is_open());
        dialog.send(
            Input::Focus {
                part: Part::Content,
                item: None,
            },
            &mut |_| true,
        );
        dialog.send(enter(), &mut |_| true);
        assert!(
            dialog.is_open(),
            "Enter inside the dialog is the content's business"
        );
        dialog.send(
            Input::Focus {
                part: Part::Close,
                item: None,
            },
            &mut |_| true,
        );
        dialog.send(enter(), &mut |_| true);
        assert!(!dialog.is_open());
    }

    #[test]
    fn it_conforms() {
        crate::testing::assert_conforms(Dialog::default, &[]);
    }
}
