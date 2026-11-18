//! The examples in `skills/armature/references/core-contract.md`, kept here so
//! they stay true.

#![allow(
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

use armature_core::agent::ActError;
use armature_core::change::{Reason, Source, ValueChange};
use armature_core::combobox::{self, Combobox};
use armature_core::dialog::{self, Dialog};
use armature_core::keys::{Key, Modifiers};
use armature_core::primitive::{Input, Item, Primitive};
use armature_core::tabs::{self, Tabs};

#[test]
fn a_refused_change_keeps_the_tab() {
    let mut tabs = Tabs::default();
    for key in ["one", "two", "three"] {
        tabs.upsert_item(Item {
            key: key.into(),
            label: key.into(),
            disabled: false,
        });
    }
    assert_eq!(
        tabs.value(),
        ["one"],
        "a fresh Tabs has its first enabled tab selected"
    );
    let right = || Input::Key {
        key: Key::ArrowRight,
        mods: Modifiers::default(),
        now_ms: 0,
    };
    let moved = tabs.send(right(), &mut |_| true);
    assert_eq!(tabs.value(), ["two"]);
    assert_eq!(moved.focus_item(), Some("two"));
    tabs.send(right(), &mut |_| false);
    assert_eq!(tabs.value(), ["two"]);
    let manual = Tabs::new(tabs::Options {
        manual: true,
        ..Default::default()
    });
    assert_eq!(manual.value(), [] as [std::string::String; 0]);
}

#[test]
fn the_gate_sees_who_and_why() {
    let mut dialog = Dialog::default();
    let mut seen = Vec::new();
    dialog
        .act("open", None, &mut |change: &ValueChange| {
            seen.push((change.value.clone(), change.source, change.reason));
            true
        })
        .unwrap();
    assert_eq!(
        seen,
        [(vec!["open".to_string()], Source::Agent, Reason::Action)]
    );
    assert!(dialog.is_open());
    assert!(matches!(
        dialog.act("explode", None, &mut |_| true),
        Err(ActError::UnknownAction { .. })
    ));
    let closed = dialog.send(
        Input::Key {
            key: Key::Escape,
            mods: Modifiers::default(),
            now_ms: 0,
        },
        &mut |_| true,
    );
    assert_eq!(
        closed.focus.map(|target| target.part),
        Some(dialog::Part::Trigger)
    );
}

#[test]
fn typing_narrows_a_combobox() {
    let mut combobox = Combobox::default();
    for key in ["Apple", "Apricot", "Banana"] {
        combobox.upsert_item(Item {
            key: key.into(),
            label: key.into(),
            disabled: false,
        });
    }
    combobox.send(
        Input::Text {
            part: combobox::Part::Input,
            item: None,
            value: "ap".into(),
        },
        &mut |_| true,
    );
    assert_eq!(combobox.results(), ["Apple", "Apricot"]);
    assert!(combobox.is_open());
    assert_eq!(
        combobox.text(combobox::Part::Input, None).as_deref(),
        Some("ap")
    );
}

#[test]
fn the_details_the_skill_states() {
    use armature_core::accordion::Accordion;
    use armature_core::calendar::Selecting;
    use armature_core::date::Date;
    use armature_core::keys::{Dir, Orientation};
    use armature_core::listbox::{self, Listbox};
    use armature_core::number::NumberRange;
    use armature_core::number_input::NumberInput;
    use armature_core::selection::Mode;

    let _ = (
        Dir::Ltr,
        Dir::Rtl,
        Orientation::Horizontal,
        Orientation::Vertical,
    );
    let _ = (
        Mode::Single,
        Mode::Multiple,
        Selecting::Single,
        Selecting::Multiple,
        Selecting::Range,
    );
    assert!(Date::new(2026, 2, 30).is_none() && Date::new(2026, 2, 28).is_some());
    assert_eq!(
        NumberInput::new(NumberRange::new(0.0, 10.0, 0.5), 2.5).value(),
        ["2.5"]
    );

    // A fresh accordion has nothing open.
    let mut accordion = Accordion::default();
    accordion.upsert_item(Item {
        key: "a".into(),
        label: "a".into(),
        disabled: false,
    });
    assert_eq!(accordion.value(), [] as [std::string::String; 0]);

    // Lists: arrows skip disabled items and only move focus; Enter chooses.
    let mut list = Listbox::default();
    for (key, disabled) in [("a", false), ("b", true), ("c", false)] {
        list.upsert_item(Item {
            key: key.into(),
            label: key.into(),
            disabled,
        });
    }
    list.send(
        Input::Focus {
            part: listbox::Part::Option,
            item: Some("a".into()),
        },
        &mut |_| true,
    );
    let key = |key| Input::Key {
        key,
        mods: Modifiers::default(),
        now_ms: 0,
    };
    let moved = list.send(key(Key::ArrowDown), &mut |_| true);
    assert_eq!(moved.focus_item(), Some("c"));
    assert!(list.value().is_empty(), "arrows only move focus");
    list.send(
        Input::Focus {
            part: listbox::Part::Option,
            item: Some("c".into()),
        },
        &mut |_| true,
    );
    list.send(key(Key::Enter), &mut |_| true);
    assert_eq!(list.value(), ["c"]);
    assert_eq!(
        list.act("select", Some("b"), &mut |_| true),
        Err(ActError::Disabled("b".into()))
    );
}
