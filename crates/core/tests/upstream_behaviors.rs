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
