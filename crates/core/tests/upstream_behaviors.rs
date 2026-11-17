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
