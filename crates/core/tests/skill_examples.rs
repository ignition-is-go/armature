//! The examples in `skills/armature/references/core-contract.md`, kept here so
//! they stay true.

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
    assert!(manual.value().is_empty());
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
