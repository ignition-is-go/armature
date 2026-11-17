//! Behaviors that UI SDK's component tests name (`packages/core/test`), restated
//! against armature's primitives. Each test is titled after the upstream file, and
//! each assertion carries the upstream test's own name where one maps across.

use armature_core::accordion::{self, Accordion};
use armature_core::keys::{Dir, Key, Modifiers, Orientation, apple_key};
use armature_core::listbox::{self, Listbox};
use armature_core::primitive::{Input, Item, Primitive};
use armature_core::range_slider::{self, RangeSlider};
use armature_core::rating::{self, Rating};
use armature_core::tabs::{self, Tabs};

fn item(key: &str, disabled: bool) -> Item {
    Item {
        key: key.into(),
        label: key.into(),
        disabled,
    }
}

fn key<P: Primitive>(primitive: &mut P, key: Key) -> Option<String> {
    let reaction = primitive.send(
        Input::Key {
            key,
            mods: Modifiers::default(),
            now_ms: 0,
        },
        &mut |_| true,
    );
    reaction.focus_item().map(str::to_string)
}

fn accordion(opts: accordion::Options) -> Accordion {
    let mut accordion = Accordion::new(opts);
    for (name, disabled) in [("a", false), ("b", false), ("c", true)] {
        accordion.upsert_item(item(name, disabled));
    }
    accordion
}

fn press(accordion: &mut Accordion, key: &str, allow: bool) {
    let input = Input::Press {
        part: accordion::Part::Trigger,
        item: Some(key.into()),
        mods: Modifiers::default(),
    };
    accordion.send(input, &mut |_| allow);
}

// accordion.test.ts
#[test]
fn accordion_behaviors() {
    // "single mode keeps one item open"
    let mut single = accordion(accordion::Options {
        collapsible: true,
        ..Default::default()
    });
    press(&mut single, "a", true);
    press(&mut single, "b", true);
    assert_eq!(single.value(), ["b"]);
    // "open items render expanded, others collapsed and hidden"
    assert_eq!(
        single
            .semantics(accordion::Part::Trigger, Some("b"))
            .expanded,
        Some(true)
    );
    assert!(single.semantics(accordion::Part::Panel, Some("a")).hidden);
    // "a disabled item disables its trigger"
    assert!(
        single
            .semantics(accordion::Part::Trigger, Some("c"))
            .disabled
    );
    // "a canceled change keeps the value"
    press(&mut single, "a", false);
    assert_eq!(single.value(), ["b"]);
    // "setting the value in single mode keeps the first item"
    single.set_value(vec!["a".into(), "b".into()]);
    assert_eq!(single.value(), ["a"]);

    // "multiple mode adds and removes items in order"
    let mut multiple = accordion(accordion::Options {
        multiple: true,
        collapsible: true,
        ..Default::default()
    });
    press(&mut multiple, "b", true);
    press(&mut multiple, "a", true);
    assert_eq!(multiple.value(), ["a", "b"]);
    press(&mut multiple, "a", true);
    assert_eq!(multiple.value(), ["b"]);
    // "openAll opens every enabled item, and closeAll closes them"
    multiple.act("open_all", None, &mut |_| true).unwrap();
    assert_eq!(multiple.value(), ["a", "b"]);
    multiple.act("close_all", None, &mut |_| true).unwrap();
    assert!(multiple.value().is_empty());

    // "without collapsible, the open item stays open and says so"
    let mut fixed = accordion(accordion::Options::default());
    press(&mut fixed, "a", true);
    press(&mut fixed, "a", true);
    assert_eq!(fixed.value(), ["a"]);
    // "closeAll leaves the one item that must stay open"
    fixed.act("close_all", None, &mut |_| true).unwrap();
    assert_eq!(fixed.value(), ["a"]);
    // "closed panels kept mounted are hidden, or hidden until found"
    let findable = accordion(accordion::Options {
        findable: true,
        ..Default::default()
    });
    let panel = findable.semantics(accordion::Part::Panel, Some("a"));
    assert!(panel.hidden && panel.findable);
}

fn tabs(opts: tabs::Options) -> Tabs {
    let mut tabs = Tabs::new(opts);
    for name in ["one", "two", "three"] {
        tabs.upsert_item(item(name, false));
    }
    tabs
}

// tabs.test.ts
#[test]
fn tabs_behaviors() {
    // "horizontal lists use left and right, mirrored in right-to-left layouts"
    let mut ltr = tabs(tabs::Options::default());
    assert_eq!(key(&mut ltr, Key::ArrowRight).as_deref(), Some("two"));
    assert_eq!(key(&mut ltr, Key::ArrowDown), None);
    let mut rtl = tabs(tabs::Options {
        dir: Dir::Rtl,
        ..Default::default()
    });
    assert_eq!(key(&mut rtl, Key::ArrowLeft).as_deref(), Some("two"));
    // "vertical lists use up and down, whatever the direction"
    let mut vertical = tabs(tabs::Options {
        orientation: Orientation::Vertical,
        dir: Dir::Rtl,
        ..Default::default()
    });
    assert_eq!(key(&mut vertical, Key::ArrowDown).as_deref(), Some("two"));
    assert_eq!(key(&mut vertical, Key::ArrowLeft), None);
    // "Home and End go to the ends"
    assert_eq!(key(&mut ltr, Key::End).as_deref(), Some("three"));
    assert_eq!(key(&mut ltr, Key::Home).as_deref(), Some("one"));
    // "moves wrap or stop at the ends"
    assert_eq!(key(&mut ltr, Key::ArrowLeft).as_deref(), Some("three"));
    let mut stopping = tabs(tabs::Options {
        wrap: false,
        ..Default::default()
    });
    assert_eq!(key(&mut stopping, Key::ArrowLeft), None);
    // "only the selected panel is present" and "pairs tabs and panels by value"
    assert!(!ltr.semantics(tabs::Part::Panel, Some("three")).hidden);
    assert!(ltr.semantics(tabs::Part::Panel, Some("one")).hidden);
    // "a cancelled change keeps the selection"
    ltr.send(
        Input::Press {
            part: tabs::Part::Tab,
            item: Some("one".into()),
            mods: Modifiers::default(),
        },
        &mut |_| false,
    );
    assert_eq!(ltr.value(), ["three"]);
    // "vertical lists say so"
    assert_eq!(
        vertical.semantics(tabs::Part::List, None).orientation,
        Some(Orientation::Vertical)
    );
}

// slider.test.ts
#[test]
fn slider_behaviors() {
    use armature_core::geometry::{Point, Rect};
    use armature_core::primitive::{Pointer, PointerPhase};
    let track = |_: range_slider::Part, _: Option<&str>| Some(Rect::new(0.0, 0.0, 100.0, 10.0));
    let press_at = |slider: &mut RangeSlider, x: f64| {
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            let pointer = Pointer {
                phase,
                part: range_slider::Part::Track,
                item: None,
                at: Point::new(x, 5.0),
                mods: Modifiers::default(),
                now_ms: 0,
            };
            slider.pointer(pointer, &track, &mut |_| true);
        }
    };
    // "keeps thumbs between their neighbours, apart by the minimum distance"
    let opts = range_slider::Options {
        min_steps_between: 10.0,
        ..Default::default()
    };
    let mut slider = RangeSlider::new(opts, &[20.0, 60.0]);
    slider.act("set", Some("0=90"), &mut |_| true).unwrap();
    assert_eq!(slider.value(), ["50", "60"]);
    // "a press picks the nearest thumb, and the side of a stack"
    let mut stacked = RangeSlider::new(range_slider::Options::default(), &[50.0, 50.0]);
    press_at(&mut stacked, 80.0);
    assert_eq!(
        stacked.value(),
        ["50", "80"],
        "a press beyond the stack takes the upper grip"
    );
    let mut stacked = RangeSlider::new(range_slider::Options::default(), &[50.0, 50.0]);
    press_at(&mut stacked, 20.0);
    assert_eq!(stacked.value(), ["20", "50"]);
    // "positions are percentages of the range"
    assert_eq!(stacked.fractions(), [0.2, 0.5]);
    // "maps keys the APG way, with Left and Right flipped right to left"
    let mut rtl = RangeSlider::new(
        range_slider::Options {
            dir: Dir::Rtl,
            ..Default::default()
        },
        &[20.0, 60.0],
    );
    rtl.send(
        Input::Key {
            key: Key::ArrowLeft,
            mods: Modifiers::default(),
            now_ms: 0,
        },
        &mut |_| true,
    );
    rtl.send(
        Input::Key {
            key: Key::ArrowUp,
            mods: Modifiers::default(),
            now_ms: 0,
        },
        &mut |_| true,
    );
    assert_eq!(rtl.value(), ["22", "60"]);
}

// listbox-keyboard.test.ts
#[test]
fn listbox_keyboard_behaviors() {
    let mut list = Listbox::new(listbox::Options {
        wrap: false,
        ..Default::default()
    });
    for index in 0..30 {
        list.upsert_item(item(&index.to_string(), false));
    }
    // "arrows step by one item"
    assert_eq!(key(&mut list, Key::ArrowDown).as_deref(), Some("0"));
    assert_eq!(key(&mut list, Key::ArrowDown).as_deref(), Some("1"));
    // "PageDown and PageUp step by a page"
    assert_eq!(key(&mut list, Key::PageDown).as_deref(), Some("11"));
    assert_eq!(key(&mut list, Key::PageUp).as_deref(), Some("1"));
    // "Cmd with an arrow goes to the last or first item"
    assert_eq!(
        apple_key(Key::ArrowDown, false, true, false),
        Some(Key::End)
    );
    assert_eq!(apple_key(Key::ArrowUp, false, true, false), Some(Key::Home));
    assert_eq!(key(&mut list, Key::End).as_deref(), Some("29"));
    // "Ctrl+N and Ctrl+P step by one item on Apple platforms only"
    assert_eq!(
        apple_key(Key::Char('n'), true, false, false),
        Some(Key::ArrowDown)
    );
    assert_eq!(
        apple_key(Key::Char('p'), true, false, false),
        Some(Key::ArrowUp)
    );
    // "Ctrl+N with another modifier, and N alone, are no step"
    assert_eq!(apple_key(Key::Char('n'), true, false, true), None);
    assert_eq!(apple_key(Key::Char('n'), false, false, false), None);
    // "typing still searches by first letters"
    assert_eq!(key(&mut list, Key::Char('7')).as_deref(), Some("7"));
}

// rating.test.ts
#[test]
fn rating_behaviors() {
    use armature_core::primitive::Role;
    // "stars fill up to the score, with a partial star for a fraction"
    let mut rating = Rating::new(rating::Options {
        halves: true,
        ..Default::default()
    });
    rating.set_value(vec!["2.5".into()]);
    assert_eq!(
        [
            rating.fill(1),
            rating.fill(2),
            rating.fill(3),
            rating.fill(4)
        ],
        [1.0, 1.0, 0.5, 0.0]
    );
    // "the controller reads and writes numbers"
    rating.act("set", Some("4"), &mut |_| true).unwrap();
    assert_eq!(rating.value(), ["4"]);
    // "a read-only rating is an image named by its score"
    let shown = Rating::new(rating::Options {
        read_only: true,
        ..Default::default()
    });
    assert_eq!(
        shown.semantics(rating::Part::Root, None).role,
        Some(Role::Image)
    );
}

use armature_core::calendar::{self, Calendar, Selecting};
use armature_core::collapsible::{self, Collapsible};
use armature_core::date::Date;
use armature_core::otp_input::{self, Accept, OtpInput};
use armature_core::primitive::Focusable;
use armature_core::timing::Hold;
use armature_core::toggle_group::{self, ToggleGroup};

fn focused_code(length: usize) -> OtpInput {
    let mut code = OtpInput::new(length);
    code.send(
        Input::Focus {
            part: otp_input::Part::Input,
            item: None,
        },
        &mut |_| true,
    );
    code
}

fn type_keys(code: &mut OtpInput, keys: &str) {
    for ch in keys.chars() {
        key(code, Key::Char(ch));
    }
}

// otp-input.test.ts
#[test]
fn otp_input_behaviors() {
    // "accepts characters by preset"
    let mut digits = focused_code(4);
    type_keys(&mut digits, "1a2");
    assert_eq!(digits.value(), ["12"]);
    let mut letters = OtpInput::new(4).accepting(Accept::Alphanumeric);
    letters.send(
        Input::Focus {
            part: otp_input::Part::Input,
            item: None,
        },
        &mut |_| true,
    );
    type_keys(&mut letters, "a1-");
    assert_eq!(letters.value(), ["a1"]);
    // "tests each character against a custom expression"
    let mut vowels = OtpInput::new(4).accepting(Accept::Custom(|ch| "aeiou".contains(ch)));
    vowels.set_value(vec!["hello".into()]);
    assert_eq!(vowels.value(), ["eo"]);
    // "drops spaces and dashes from a paste, and ignores one with anything else"
    let mut pasted = focused_code(6);
    pasted.send(
        Input::Paste {
            text: "123 4-56".into(),
        },
        &mut |_| true,
    );
    assert_eq!(pasted.value(), ["123456"]);
    pasted.send(
        Input::Paste {
            text: "12x4".into(),
        },
        &mut |_| true,
    );
    assert_eq!(pasted.value(), ["123456"]);
    // "a whole code replaces the value, wherever the caret is"
    key(&mut pasted, Key::Home);
    pasted.send(
        Input::Paste {
            text: "654321".into(),
        },
        &mut |_| true,
    );
    assert_eq!(pasted.value(), ["654321"]);
    // "a caret at the end of a full code selects its last character"
    assert_eq!(pasted.active_cell(), Some(5));
    // "focus goes after the code, or onto its last character when full"
    let mut partial = focused_code(4);
    type_keys(&mut partial, "12");
    assert_eq!(partial.active_cell(), Some(2));
    // "ArrowLeft from the empty slot at the end selects the last character"
    key(&mut partial, Key::ArrowLeft);
    assert_eq!(partial.active_cell(), Some(1));
    // "inserts over the selection": typing replaces the selected character
    type_keys(&mut partial, "9");
    assert_eq!(partial.value(), ["19"]);
    // "arrows move a slot at a time, and past the code to the empty slot after it"
    key(&mut partial, Key::ArrowRight);
    key(&mut partial, Key::ArrowRight);
    assert_eq!(
        partial.active_cell(),
        Some(2),
        "no further than the empty slot after the code"
    );
    // "Home selects the first character and End the slot after the code"
    key(&mut partial, Key::Home);
    assert_eq!(partial.active_cell(), Some(0));
    key(&mut partial, Key::End);
    assert_eq!(partial.active_cell(), Some(2));
    // "right to left, the arrows swap"
    let mut rtl = OtpInput::new(4).direction(Dir::Rtl);
    rtl.set_value(vec!["12".into()]);
    rtl.send(
        Input::Focus {
            part: otp_input::Part::Input,
            item: None,
        },
        &mut |_| true,
    );
    key(&mut rtl, Key::ArrowRight);
    assert_eq!(rtl.active_cell(), Some(1));
    // "a press selects that slot's character, or the slot after the code"
    rtl.send(
        Input::Press {
            part: otp_input::Part::Cell,
            item: Some("3".into()),
            mods: Modifiers::default(),
        },
        &mut |_| true,
    );
    assert_eq!(rtl.active_cell(), Some(2));
    // "nothing is active without focus"
    assert_eq!(OtpInput::new(4).active_cell(), None);
    // "a canceled change keeps the code"
    let mut refused = focused_code(4);
    refused.send(
        Input::Key {
            key: Key::Char('1'),
            mods: Modifiers::default(),
            now_ms: 0,
        },
        &mut |_| false,
    );
    assert_eq!(refused.value(), [""]);
    // "other keys are left to the browser"
    let tab = refused.send(
        Input::Key {
            key: Key::Tab,
            mods: Modifiers::default(),
            now_ms: 0,
        },
        &mut |_| true,
    );
    assert!(!tab.handled);
}

fn toggles(opts: toggle_group::Options) -> ToggleGroup {
    let mut group = ToggleGroup::new(opts);
    for (name, disabled) in [("bold", false), ("italic", true), ("under", false)] {
        group.upsert_item(item(name, disabled));
    }
    group
}

fn press_toggle(group: &mut ToggleGroup, key: &str, allow: bool) {
    let input = Input::Press {
        part: toggle_group::Part::Toggle,
        item: Some(key.into()),
        mods: Modifiers::default(),
    };
    group.send(input, &mut |_| allow);
}

// toggle.test.ts
#[test]
fn toggle_behaviors() {
    // "single mode presses one toggle at a time"
    let mut single = toggles(toggle_group::Options::default());
    press_toggle(&mut single, "bold", true);
    press_toggle(&mut single, "under", true);
    assert_eq!(single.value(), ["under"]);
    // "a canceled change keeps it as it was"
    press_toggle(&mut single, "bold", false);
    assert_eq!(single.value(), ["under"]);
    // "a disabled toggle ignores presses"
    press_toggle(&mut single, "italic", true);
    assert_eq!(single.value(), ["under"]);
    // "deselectable={false} keeps the pressed toggle pressed"
    let mut kept = toggles(toggle_group::Options {
        deselectable: false,
        ..Default::default()
    });
    press_toggle(&mut kept, "bold", true);
    press_toggle(&mut kept, "bold", true);
    assert_eq!(kept.value(), ["bold"]);
    // "values keep document order with multiple"
    let mut several = toggles(toggle_group::Options {
        multiple: true,
        ..Default::default()
    });
    press_toggle(&mut several, "under", true);
    press_toggle(&mut several, "bold", true);
    assert_eq!(several.value(), ["bold", "under"]);
    // "the tab stop starts on the first pressed enabled toggle, else the first enabled one"
    let fresh = toggles(toggle_group::Options::default());
    assert_eq!(
        fresh
            .semantics(toggle_group::Part::Toggle, Some("bold"))
            .focusable,
        Focusable::TabStop
    );
    let mut pressed = toggles(toggle_group::Options::default());
    pressed.set_value(vec!["under".into()]);
    assert_eq!(
        pressed
            .semantics(toggle_group::Part::Toggle, Some("under"))
            .focusable,
        Focusable::TabStop
    );
    assert_eq!(
        pressed
            .semantics(toggle_group::Part::Toggle, Some("bold"))
            .focusable,
        Focusable::Programmatic
    );
    // "disabled toggles in a group stay focusable with aria-disabled"
    let italic = fresh.semantics(toggle_group::Part::Toggle, Some("italic"));
    assert!(italic.disabled && italic.focusable != Focusable::No);
}

// collapsible.test.ts
#[test]
fn collapsible_behaviors() {
    let press = |section: &mut Collapsible, allow: bool| {
        section.send(
            Input::Press {
                part: collapsible::Part::Trigger,
                item: None,
                mods: Modifiers::default(),
            },
            &mut |_| allow,
        );
    };
    // "renders closed by default, with the panel out of the page"
    let mut section = Collapsible::default();
    assert!(section.semantics(collapsible::Part::Panel, None).hidden);
    // "open renders the panel and the open attributes"
    press(&mut section, true);
    assert_eq!(
        section.semantics(collapsible::Part::Trigger, None).expanded,
        Some(true)
    );
    assert!(!section.semantics(collapsible::Part::Panel, None).hidden);
    // "toggling calls back with the reason, and can be canceled"
    press(&mut section, false);
    assert!(section.is_open());
    // "a kept or findable panel stays in the page, hidden"
    let findable = Collapsible::default().findable();
    let panel = findable.semantics(collapsible::Part::Panel, None);
    assert!(panel.hidden && panel.findable);
    // "disabled marks every part and keeps the trigger focusable"
    let disabled = Collapsible::default().disabled();
    let trigger = disabled.semantics(collapsible::Part::Trigger, None);
    assert!(trigger.disabled && trigger.focusable != Focusable::No);
}

// hold.test.ts
#[test]
fn hold_behaviors() {
    // "holds once a press lasts past its delay"
    let mut hold = Hold::new(500);
    assert_eq!(hold.start(1000), 1500);
    assert!(!hold.tick(1400));
    assert!(hold.tick(1500));
    assert!(!hold.tick(2000), "once per press");
    // "a short press isn't held"
    let mut short = Hold::new(500);
    short.start(0);
    short.release();
    assert!(!short.tick(600));
    // "a pointer that moves past the tolerance before it's held cancels the press"
    let mut moved = Hold::new(500);
    moved.start(0);
    moved.moved(40.0);
    assert!(!moved.tick(600));
    // A small drift within the tolerance does not.
    let mut steady = Hold::new(500);
    steady.start(0);
    steady.moved(4.0);
    assert!(steady.tick(600));
}

// calendar.test.ts
#[test]
fn calendar_behaviors() {
    let start = Date::new(2026, 10, 16).unwrap();
    let on = |calendar: &Calendar| calendar.semantics(calendar::Part::Day, None);
    let _ = on;
    let focus_of = |calendar: &mut Calendar, pressed: Key, mods: Modifiers| {
        calendar
            .send(
                Input::Key {
                    key: pressed,
                    mods,
                    now_ms: 0,
                },
                &mut |_| true,
            )
            .focus_item()
            .map(str::to_string)
    };
    let plain = Modifiers::default();
    // "arrow keys move by day and week, into the next month as it renders"
    let mut calendar = Calendar::new(calendar::Options::default(), start);
    assert_eq!(
        focus_of(&mut calendar, Key::ArrowRight, plain).as_deref(),
        Some("2026-10-17")
    );
    assert_eq!(
        focus_of(&mut calendar, Key::ArrowDown, plain).as_deref(),
        Some("2026-10-24")
    );
    assert_eq!(
        focus_of(&mut calendar, Key::ArrowDown, plain).as_deref(),
        Some("2026-10-31")
    );
    assert_eq!(
        focus_of(&mut calendar, Key::ArrowDown, plain).as_deref(),
        Some("2026-11-07")
    );
    assert_eq!(calendar.month(), (2026, 11));
    // "Page Up and Down turn months, with Shift years, and Home and End go to the week's ends"
    assert_eq!(
        focus_of(&mut calendar, Key::PageUp, plain).as_deref(),
        Some("2026-10-07")
    );
    assert_eq!(
        focus_of(
            &mut calendar,
            Key::PageDown,
            Modifiers {
                shift: true,
                ..plain
            }
        )
        .as_deref(),
        Some("2027-10-07")
    );
    assert_eq!(
        focus_of(&mut calendar, Key::Home, plain).as_deref(),
        Some("2027-10-03")
    );
    assert_eq!(
        focus_of(&mut calendar, Key::End, plain).as_deref(),
        Some("2027-10-09")
    );
    // "Left and Right swap in a right-to-left page"
    let mut rtl = Calendar::new(
        calendar::Options {
            dir: Dir::Rtl,
            ..Default::default()
        },
        start,
    );
    assert_eq!(
        focus_of(&mut rtl, Key::ArrowLeft, plain).as_deref(),
        Some("2026-10-17")
    );
    // "keys stop at min and max"
    let limits = calendar::Options {
        min: Date::new(2026, 10, 15),
        max: Date::new(2026, 10, 17),
        ..Default::default()
    };
    let mut bounded = Calendar::new(limits, start);
    focus_of(&mut bounded, Key::ArrowRight, plain);
    assert_eq!(
        focus_of(&mut bounded, Key::ArrowRight, plain),
        None,
        "no further than the last day allowed"
    );
    // "a press or Enter selects one date"
    let mut picking = Calendar::new(calendar::Options::default(), start);
    focus_of(&mut picking, Key::Enter, plain);
    assert_eq!(picking.value(), ["2026-10-16"]);
    // "multiple mode toggles dates and keeps them sorted"
    let mut several = Calendar::new(
        calendar::Options {
            mode: Selecting::Multiple,
            ..Default::default()
        },
        start,
    );
    for day in ["2026-10-20", "2026-10-12", "2026-10-20", "2026-10-14"] {
        several.act("select", Some(day), &mut |_| true).unwrap();
    }
    assert_eq!(several.value(), ["2026-10-12", "2026-10-14"]);
    // "a range ends before its start, which then becomes its end"
    let mut range = Calendar::new(
        calendar::Options {
            mode: Selecting::Range,
            ..Default::default()
        },
        start,
    );
    range
        .act("select", Some("2026-10-20"), &mut |_| true)
        .unwrap();
    range
        .act("select", Some("2026-10-12"), &mut |_| true)
        .unwrap();
    assert_eq!(range.value(), ["2026-10-12", "2026-10-20"]);
    // "the tab stop starts on the selection"
    assert_eq!(
        range
            .semantics(calendar::Part::Day, Some("2026-10-12"))
            .focusable,
        Focusable::TabStop
    );
}

use armature_core::dialog::{self, Dialog, Dismiss, Refusal};

// dialog.test.ts
#[test]
fn dialog_behaviors() {
    let open = dialog::Options::default();
    let press = |dialog: &mut Dialog, part: dialog::Part| {
        dialog.send(
            Input::Press {
                part,
                item: None,
                mods: Modifiers::default(),
            },
            &mut |_| true,
        );
    };
    // "dismissals close unless dismissible is false, which makes them attempts"
    for dismiss in [Dismiss::Escape, Dismiss::OutsidePress, Dismiss::Back] {
        assert_eq!(open.refusal(dismiss), None);
        assert_eq!(
            dialog::Options {
                dismissible: false,
                ..open
            }
            .refusal(dismiss),
            Some(Refusal::Attempt)
        );
    }
    // "Close parts, triggers and the controller close a non-dismissible dialog"
    for dismiss in [
        Dismiss::ClosePress,
        Dismiss::TriggerPress,
        Dismiss::Controller,
    ] {
        assert_eq!(
            dialog::Options {
                dismissible: false,
                ..open
            }
            .refusal(dismiss),
            None
        );
    }
    // "an outside press is an attempt when outside presses don't close"
    let kept = dialog::Options {
        dismiss_on_backdrop: false,
        ..open
    };
    assert_eq!(kept.refusal(Dismiss::OutsidePress), Some(Refusal::Attempt));
    assert_eq!(kept.refusal(Dismiss::Escape), None);
    // "a refused press beside a non-modal dialog is the page's, not an attempt"
    let beside = dialog::Options {
        dismiss_on_backdrop: false,
        modal: false,
        ..open
    };
    assert_eq!(beside.refusal(Dismiss::OutsidePress), Some(Refusal::Silent));
    assert_eq!(
        dialog::Options {
            dismissible: false,
            ..beside
        }
        .refusal(Dismiss::Escape),
        Some(Refusal::Attempt)
    );
    // "an outside press never closes an alert dialog" and "an alert dialog is always modal"
    let mut alert = Dialog::new(dialog::Options {
        alert: true,
        modal: false,
        ..open
    });
    press(&mut alert, dialog::Part::Trigger);
    alert.send(Input::PressOutside, &mut |_| true);
    assert!(alert.is_open() && alert.semantics(dialog::Part::Content, None).modal);
    // "a dialog is modal by default, and marked modal"
    let mut modal = Dialog::default();
    press(&mut modal, dialog::Part::Trigger);
    assert!(modal.semantics(dialog::Part::Content, None).modal);
    // "a modal dialog's trigger has no aria-expanded, and a non-modal one's does"
    assert_eq!(modal.semantics(dialog::Part::Trigger, None).expanded, None);
    let plain = Dialog::new(dialog::Options {
        modal: false,
        ..open
    });
    assert_eq!(
        plain.semantics(dialog::Part::Trigger, None).expanded,
        Some(false)
    );
    assert!(!plain.semantics(dialog::Part::Content, None).modal);
    // "Escape on a non-dismissible dialog reports the attempt and keeps it open"
    let mut fixed = Dialog::new(dialog::Options {
        dismissible: false,
        ..open
    });
    press(&mut fixed, dialog::Part::Trigger);
    fixed.send(
        Input::Key {
            key: Key::Escape,
            mods: Modifiers::default(),
            now_ms: 0,
        },
        &mut |_| true,
    );
    assert!(fixed.is_open());
    assert_eq!(fixed.attempts(), 1);
    // "canceling the close keeps it open"
    modal.send(
        Input::Key {
            key: Key::Escape,
            mods: Modifiers::default(),
            now_ms: 0,
        },
        &mut |_| false,
    );
    assert!(modal.is_open());
    assert_eq!(
        modal.attempts(),
        0,
        "the owner's own refusal is not counted again"
    );
}

use armature_core::tooltip::{self, Tooltip, Warmth};

// tooltip.test.ts
#[test]
fn tooltip_behaviors() {
    let hover = |tip: &mut Tooltip, inside: bool, now_ms: u64, allow: bool| {
        tip.send(
            Input::Hover {
                part: tooltip::Part::Trigger,
                item: None,
                inside,
                now_ms,
            },
            &mut |_| allow,
        );
    };
    let tick = |tip: &mut Tooltip, now_ms: u64, allow: bool| {
        tip.send(Input::Tick { now_ms }, &mut |_| allow);
    };
    // "opens after the delay, and at once for 0"
    let mut tip = Tooltip::default();
    hover(&mut tip, true, 0, true);
    assert!(!tip.is_open());
    tick(&mut tip, 500, true);
    assert!(tip.is_open());
    let mut instant = Tooltip::new(tooltip::Options {
        open_delay_ms: 0,
        ..Default::default()
    });
    hover(&mut instant, true, 0, true);
    assert!(instant.is_open());
    // "leaving before the delay drops the open"
    let mut left = Tooltip::default();
    hover(&mut left, true, 0, true);
    hover(&mut left, false, 200, true);
    tick(&mut left, 500, true);
    assert!(!left.is_open());
    // "the content is always present, hidden while closed, so the reference resolves"
    assert!(left.semantics(tooltip::Part::Content, None).hidden);
    // "a canceled change keeps the card closed"
    let mut refused = Tooltip::default();
    hover(&mut refused, true, 0, true);
    tick(&mut refused, 500, false);
    assert!(!refused.is_open());
    // "a disabled tooltip refuses hover and focus but obeys its controller"
    let mut disabled = Tooltip::new(tooltip::Options {
        open_delay_ms: 0,
        disabled: true,
        ..Default::default()
    });
    hover(&mut disabled, true, 0, true);
    disabled.send(
        Input::Focus {
            part: tooltip::Part::Trigger,
            item: None,
        },
        &mut |_| true,
    );
    assert!(!disabled.is_open());
    disabled.act("show", None, &mut |_| true).unwrap();
    assert!(disabled.is_open());
    // "a tooltip that opens while another is warm opens instantly"
    let warmth = Warmth::default();
    let mut first = Tooltip::default().sharing(warmth.clone());
    let mut second = Tooltip::default().sharing(warmth);
    hover(&mut first, true, 0, true);
    tick(&mut first, 500, true);
    hover(&mut first, false, 900, true);
    hover(&mut second, true, 1000, true);
    assert!(
        !first.is_open() && second.is_open(),
        "no second wait while the group is warm"
    );
    // Once it has cooled, the next waits again.
    hover(&mut second, false, 1100, true);
    hover(&mut first, true, 5000, true);
    assert!(!first.is_open());
}

use armature_core::number::NumberRange;
use armature_core::number_input::NumberInput;
use armature_core::select::Select;

// select.test.ts
#[test]
fn select_behaviors() {
    let filled = |select: Select| {
        let mut select = select;
        for name in ["small", "large"] {
            select.upsert_item(item(name, false));
        }
        select.set_value(vec!["large".into()]);
        select
    };
    // "clearing a read-only or disabled select keeps its value"
    let mut locked = filled(Select::default().read_only());
    assert!(locked.act("clear", None, &mut |_| true).is_err());
    assert_eq!(locked.value(), ["large"]);
    // "a canceled clear keeps the value"
    let mut select = filled(Select::default());
    let _ = select.act("clear", None, &mut |_| false);
    assert_eq!(select.value(), ["large"]);
    select.act("clear", None, &mut |_| true).unwrap();
    assert!(select.value().is_empty());
}

// number-input.test.ts
#[test]
fn number_input_behaviors() {
    use armature_core::number_input::Part;
    // "End and the increment button stop at the highest value on the step grid"
    let mut input = NumberInput::new(NumberRange::new(0.0, 10.0, 3.0), 0.0);
    key(&mut input, Key::End);
    assert_eq!(input.number(), 9.0);
    assert!(input.semantics(Part::Increment, None).disabled);
    input.act("increment", None, &mut |_| true).unwrap();
    assert_eq!(input.number(), 9.0);
    key(&mut input, Key::Home);
    assert_eq!(input.number(), 0.0);
    assert!(input.semantics(Part::Decrement, None).disabled);
    // "the default small step has no floating point noise"
    let mut small = NumberInput::new(NumberRange::new(0.0, 1.0, 0.1), 0.0);
    for _ in 0..3 {
        key(&mut small, Key::ArrowUp);
    }
    assert_eq!(small.value(), ["0.3"]);
    // "typing follows the value only while it's a number within the bounds"
    let mut typed = NumberInput::new(NumberRange::new(0.0, 10.0, 1.0), 5.0);
    typed.send(
        Input::Text {
            part: Part::Input,
            item: None,
            value: "abc".into(),
        },
        &mut |_| true,
    );
    key(&mut typed, Key::Enter);
    assert_eq!(typed.number(), 5.0);
    typed.send(
        Input::Text {
            part: Part::Input,
            item: None,
            value: "50".into(),
        },
        &mut |_| true,
    );
    key(&mut typed, Key::Enter);
    assert_eq!(typed.number(), 10.0, "Enter clamps what was typed");
}

use armature_core::combobox::{self, Combobox};

fn frameworks(opts: combobox::Options) -> Combobox {
    let mut combobox = Combobox::new(opts);
    for (key, label) in [("astro", "Astro"), ("next", "Next.js"), ("nuxt", "Nuxt")] {
        combobox.upsert_item(Item {
            key: key.into(),
            label: label.into(),
            disabled: false,
        });
    }
    combobox.set_value(vec!["astro".into()]);
    combobox
}

// combobox.test.ts
#[test]
fn combobox_behaviors() {
    use armature_core::change::Reason;
    use combobox::Part;
    let typed = |combobox: &mut Combobox, text: &str, allow: bool| {
        combobox.send(
            Input::Text {
                part: Part::Input,
                item: None,
                value: text.into(),
            },
            &mut |_| allow,
        );
    };
    let query = |combobox: &Combobox| combobox.text(Part::Input, None).unwrap_or_default();
    // "clearing empties the value and the text"
    let mut cleared = frameworks(Default::default());
    cleared.act("clear", None, &mut |_| true).unwrap();
    assert!(cleared.value().is_empty() && query(&cleared).is_empty());
    // "a canceled clear keeps the value and its label in the input"
    let mut kept = frameworks(Default::default());
    assert!(kept.act("clear", None, &mut |_| false).is_err());
    assert_eq!(
        (kept.value(), query(&kept)),
        (vec!["astro".to_string()], "Astro".to_string())
    );
    // "Escape after deleting the text brings the value back with the reason escape-key"
    let mut combobox = frameworks(Default::default());
    let mut reasons = Vec::new();
    combobox.send(
        Input::Text {
            part: Part::Input,
            item: None,
            value: String::new(),
        },
        &mut |change| {
            reasons.push(change.reason);
            true
        },
    );
    assert!(combobox.value().is_empty());
    combobox.send(
        Input::Key {
            key: Key::Escape,
            mods: Modifiers::default(),
            now_ms: 0,
        },
        &mut |change| {
            reasons.push(change.reason);
            true
        },
    );
    assert_eq!(
        (combobox.value(), query(&combobox)),
        (vec!["astro".to_string()], "Astro".to_string())
    );
    assert_eq!(reasons, [Reason::Text, Reason::Keyboard]);
    // "Escape after clearing keeps the value cleared"
    let mut combobox = frameworks(Default::default());
    typed(&mut combobox, "", true);
    typed(&mut combobox, "N", true);
    combobox.act("clear", None, &mut |_| true).unwrap();
    typed(&mut combobox, "N", true);
    key(&mut combobox, Key::Escape);
    assert!(combobox.value().is_empty());
    // "Escape brings back the value deleted last, not an earlier one"
    let mut combobox = frameworks(Default::default());
    typed(&mut combobox, "", true);
    combobox.set_value(vec!["next".into()]);
    typed(&mut combobox, "N", true);
    typed(&mut combobox, "", true);
    key(&mut combobox, Key::Escape);
    assert_eq!(
        (combobox.value(), query(&combobox)),
        (vec!["next".to_string()], "Next.js".to_string())
    );
    // "clearing with nothing selected still empties the text"
    let mut empty = frameworks(Default::default());
    empty.set_value(Vec::new());
    typed(&mut empty, "ne", true);
    empty.act("clear", None, &mut |_| true).unwrap();
    assert!(query(&empty).is_empty());
    // "a read-only combobox ignores clear"
    let mut locked = frameworks(combobox::Options {
        read_only: true,
        ..Default::default()
    });
    let _ = locked.act("clear", None, &mut |_| true);
    assert_eq!(
        (locked.value(), query(&locked)),
        (vec!["astro".to_string()], "Astro".to_string())
    );
}

use armature_core::menu::{self, Entry, Kind, Menu};

// menu-headless.test.ts
#[test]
fn menu_behaviors() {
    use menu::Part;
    let entries = || {
        vec![
            Entry::action("rename", "Rename"),
            Entry::action("delete", "Delete").destructive(),
            Entry::action("more", "More").of(Kind::Submenu).disabled(),
            Entry::action("inner", "Inner").inside("more"),
            Entry::action("left", "Left").of(Kind::Radio("align".into())),
            Entry::action("right", "Right").of(Kind::Radio("align".into())),
        ]
    };
    let press = |menu: &mut Menu, part: Part, item: Option<&str>, allow: bool| {
        menu.send(
            Input::Press {
                part,
                item: item.map(str::to_string),
                mods: Modifiers::default(),
            },
            &mut |_| allow,
        );
    };
    // "opening, closing, toggling and running actions" and "pressing parts runs through the same paths"
    let mut menu = Menu::new(entries());
    menu.act("open", None, &mut |_| true).unwrap();
    assert!(menu.is_open());
    menu.act("close", None, &mut |_| true).unwrap();
    press(&mut menu, Part::Trigger, None, true);
    assert!(menu.is_open());
    // "destructive items say so"
    assert!(menu.is_destructive("delete") && !menu.is_destructive("rename"));
    // "onSelect ... can cancel, which keeps the menu open"
    press(&mut menu, Part::Item, Some("rename"), false);
    assert!(menu.is_open());
    // "a disabled submenu trigger never opens its submenu"
    press(&mut menu, Part::Item, Some("more"), true);
    assert!(menu.path().is_empty());
    // "choosing the selected radio item keeps a menu open by default"
    press(&mut menu, Part::Item, Some("left"), true);
    press(&mut menu, Part::Item, Some("left"), true);
    assert!(menu.is_open() && menu.is_on("left"));
    // "choosing the selected radio item still closes a menu that closes on select"
    let mut closing = Menu::new(entries()).closing_on_select();
    press(&mut closing, Part::Trigger, None, true);
    press(&mut closing, Part::Item, Some("left"), true);
    assert!(!closing.is_open());
    press(&mut closing, Part::Trigger, None, true);
    press(&mut closing, Part::Item, Some("left"), true);
    assert!(!closing.is_open() && closing.is_on("left"));
    // "a disabled root trigger is a disabled button that never opens the menu"
    let mut disabled = Menu::new(entries()).disabled();
    press(&mut disabled, Part::Trigger, None, true);
    key(&mut disabled, Key::ArrowDown);
    assert!(!disabled.is_open());
    assert!(disabled.semantics(Part::Trigger, None).disabled);
}

use armature_core::toast::{self, Toast, Toaster};

// toast.test.ts
#[test]
fn toast_behaviors() {
    use toast::Part;
    let stack = |toasts: &[Toast]| {
        let mut toaster = Toaster::default();
        toaster.set_value(toasts.iter().map(Toast::encode).collect());
        toaster
    };
    let tick = |toaster: &mut Toaster, now_ms: u64, allow: bool| {
        toaster.send(Input::Tick { now_ms }, &mut |_| allow);
    };
    let hover = |toaster: &mut Toaster, inside: bool, now_ms: u64| {
        toaster.send(
            Input::Hover {
                part: Part::Region,
                item: None,
                inside,
                now_ms,
            },
            &mut |_| true,
        );
    };
    let count = |toaster: &Toaster| toaster.value().len();
    // "a shown toast expires after its duration"
    let mut toaster = stack(&[Toast::new("a", "Saved")]);
    tick(&mut toaster, 0, true);
    tick(&mut toaster, 4999, true);
    assert_eq!(count(&toaster), 1);
    tick(&mut toaster, 5000, true);
    assert_eq!(count(&toaster), 0);
    // "Infinity never expires": a loading toast stays
    let mut loading = stack(&[Toast::new("a", "Saving").of(toast::Kind::Loading)]);
    tick(&mut loading, 0, true);
    tick(&mut loading, 1_000_000, true);
    assert_eq!(count(&loading), 1);
    // "pausing keeps the time left, and resuming continues from it"
    let mut paused = stack(&[Toast::new("a", "Saved")]);
    tick(&mut paused, 0, true);
    hover(&mut paused, true, 4000);
    tick(&mut paused, 9000, true);
    assert_eq!(count(&paused), 1);
    hover(&mut paused, false, 10_000);
    tick(&mut paused, 10_999, true);
    assert_eq!(count(&paused), 1);
    tick(&mut paused, 11_000, true);
    assert_eq!(count(&paused), 0);
    // "pause reasons hold separately: one clearing never resumes while another holds"
    let mut both = stack(&[Toast::new("a", "Saved")]);
    tick(&mut both, 0, true);
    hover(&mut both, true, 1000);
    both.send(
        Input::Focus {
            part: Part::Close,
            item: Some("a".into()),
        },
        &mut |_| true,
    );
    hover(&mut both, false, 2000);
    tick(&mut both, 20_000, true);
    assert_eq!(count(&both), 1, "focus still holds it");
    both.send(
        Input::Blur {
            part: Part::Close,
            item: Some("a".into()),
        },
        &mut |_| true,
    );
    assert_eq!(
        both.wake_after(),
        Some(0),
        "asks to be woken so the clocks can run again"
    );
    tick(&mut both, 30_000, true);
    tick(&mut both, 33_999, true);
    assert_eq!(count(&both), 1);
    tick(&mut both, 34_000, true);
    assert_eq!(count(&both), 0);
    // Focus alone holds too.
    let mut focused = stack(&[Toast::new("a", "Saved")]);
    tick(&mut focused, 0, true);
    focused.send(
        Input::Focus {
            part: Part::Close,
            item: Some("a".into()),
        },
        &mut |_| true,
    );
    tick(&mut focused, 100, true);
    tick(&mut focused, 9000, true);
    assert_eq!(count(&focused), 1);
    // "onDismiss can keep a toast" and "a canceled timeout counts the full duration again"
    let mut kept = stack(&[Toast::new("a", "Saved")]);
    tick(&mut kept, 0, true);
    tick(&mut kept, 5000, false);
    assert_eq!(count(&kept), 1);
    tick(&mut kept, 9999, true);
    assert_eq!(count(&kept), 1);
    tick(&mut kept, 10_000, true);
    assert_eq!(count(&kept), 0);
    // "dismiss closes one toast by id, or all of them"
    let mut several = stack(&[
        Toast::new("a", "One"),
        Toast::new("b", "Two"),
        Toast::new("c", "Three"),
    ]);
    several.act("dismiss", Some("b"), &mut |_| true).unwrap();
    assert_eq!(count(&several), 2);
    several.act("dismiss_all", None, &mut |_| true).unwrap();
    assert_eq!(count(&several), 0);
}

use armature_core::popover::{self, Popover};

// popover.test.ts
#[test]
fn popover_behaviors() {
    use armature_core::geometry::{Point, Rect, Size};
    use armature_core::popup::{arrow_along, bridges};
    // "points at the anchor's center"
    let arrow = Size {
        width: 12.0,
        height: 6.0,
    };
    assert_eq!(
        arrow_along(
            Rect::new(100.0, 0.0, 60.0, 30.0),
            Rect::new(50.0, 40.0, 200.0, 100.0),
            arrow,
            false,
            6.0
        ),
        (74.0, false)
    );
    // "stops short of the corners and says so"
    assert_eq!(
        arrow_along(
            Rect::new(0.0, 0.0, 10.0, 30.0),
            Rect::new(40.0, 40.0, 200.0, 100.0),
            arrow,
            false,
            6.0
        ),
        (6.0, true)
    );
    // "runs along the vertical edge for side placements"
    let tall = Size {
        width: 6.0,
        height: 12.0,
    };
    assert_eq!(
        arrow_along(
            Rect::new(0.0, 100.0, 30.0, 40.0),
            Rect::new(40.0, 60.0, 200.0, 100.0),
            tall,
            true,
            6.0
        )
        .0,
        54.0
    );
    // hover bridge
    let trigger = Rect::new(100.0, 0.0, 60.0, 30.0);
    let below = Rect::new(50.0, 38.0, 200.0, 162.0);
    assert!(
        bridges(trigger, below, 4.0, Point::new(130.0, 34.0)),
        "covers the gap"
    );
    assert!(
        bridges(trigger, below, 4.0, Point::new(85.0, 25.0)),
        "covers a path that leaves sideways toward the popup"
    );
    assert!(!bridges(trigger, below, 4.0, Point::new(40.0, 15.0)));
    assert!(!bridges(trigger, below, 4.0, Point::new(130.0, -10.0)));
    let above = Rect::new(50.0, -200.0, 200.0, 192.0);
    assert!(bridges(trigger, above, 4.0, Point::new(130.0, -4.0)));
    assert!(!bridges(trigger, above, 4.0, Point::new(130.0, 40.0)));
    // "only modal popovers are marked modal"
    assert!(
        !Popover::default()
            .semantics(popover::Part::Content, None)
            .modal
    );
    assert!(
        Popover::default()
            .modal()
            .semantics(popover::Part::Content, None)
            .modal
    );
    // "a cancelled change keeps the popover closed"
    let mut refused = Popover::default();
    assert!(refused.act("open", None, &mut |_| false).is_err());
    refused.send(
        Input::Press {
            part: popover::Part::Trigger,
            item: None,
            mods: Modifiers::default(),
        },
        &mut |_| false,
    );
    assert!(!refused.is_open());
    // And an open one stays open when its close is refused.
    let mut open = Popover::default();
    open.act("open", None, &mut |_| true).unwrap();
    open.send(Input::PressOutside, &mut |_| false);
    assert!(open.is_open());
}
