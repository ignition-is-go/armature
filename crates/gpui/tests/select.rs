use armature_core::primitive::Item;
use armature_core::select::{Part, Select};
use armature_gpui::{Machine, agent, part};
use gpui::prelude::*;
use gpui::{Context, Entity, Modifiers, TestAppContext, VisualTestContext, Window, div, point, px};

struct Demo {
    size: Entity<Machine<Select>>,
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let option = |key| {
            part(&self.size, Part::Option, Some(key), cx)
                .h(px(20.))
                .child(key)
        };
        let content = part(&self.size, Part::Content, None, cx)
            .w(px(100.))
            .children(["small", "medium", "large"].map(option));
        div()
            .size_full()
            .child(
                part(&self.size, Part::Trigger, None, cx)
                    .w(px(100.))
                    .h(px(30.))
                    .child("Size"),
            )
            .child(content)
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<Machine<Select>>, &mut VisualTestContext) {
    let items =
        [("small", false), ("medium", true), ("large", false)].map(|(key, disabled)| Item {
            key: key.into(),
            label: key.into(),
            disabled,
        });
    let (demo, cx) = cx.add_window_view(|_, cx| Demo {
        size: cx.new(|cx| Machine::new(Select::default(), "Size", items, cx)),
    });
    let size = demo.update(cx, |demo, _| demo.size.clone());
    size.update_in(cx, |size, window, cx| {
        size.focus_handle(Part::Trigger, None)
            .unwrap()
            .focus(window, cx);
    });
    (size, cx)
}

fn is_open(size: &Entity<Machine<Select>>, cx: &mut VisualTestContext) -> bool {
    size.update(cx, |size, _| {
        size.semantics(Part::Trigger, None).expanded == Some(true)
    })
}

fn trigger_has_focus(size: &Entity<Machine<Select>>, cx: &mut VisualTestContext) -> bool {
    size.update_in(cx, |size, window, _| {
        size.focus_handle(Part::Trigger, None)
            .unwrap()
            .is_focused(window)
    })
}

const IN_TRIGGER: (f32, f32) = (10., 10.);
/// Inside the first option when the popup sits 4px below the 30px trigger.
const IN_FIRST_OPTION: (f32, f32) = (10., 44.);
const FAR_AWAY: (f32, f32) = (400., 400.);

fn click(cx: &mut VisualTestContext, (x, y): (f32, f32)) {
    cx.simulate_click(point(px(x), px(y)), Modifiers::default());
}

#[gpui::test]
fn the_keyboard_opens_chooses_and_returns_focus_to_the_trigger(cx: &mut TestAppContext) {
    let (size, cx) = open(cx);

    cx.simulate_keystrokes("enter");
    assert!(is_open(&size, cx));
    assert!(!trigger_has_focus(&size, cx), "focus moved into the popup");

    cx.simulate_keystrokes("down enter");
    assert_eq!(
        size.update(cx, |size, _| size.value()),
        ["large"],
        "down skipped medium"
    );
    assert!(!is_open(&size, cx));
    assert!(trigger_has_focus(&size, cx));
}

#[gpui::test]
fn escape_closes_and_returns_focus_without_choosing(cx: &mut TestAppContext) {
    let (size, cx) = open(cx);
    cx.simulate_keystrokes("enter down escape");
    assert!(!is_open(&size, cx));
    assert!(trigger_has_focus(&size, cx));
    assert!(size.update(cx, |size, _| size.value()).is_empty());
}

#[gpui::test]
fn the_popup_is_painted_just_below_the_trigger(cx: &mut TestAppContext) {
    let (size, cx) = open(cx);
    click(cx, IN_FIRST_OPTION);
    assert!(
        size.update(cx, |size, _| size.value()).is_empty(),
        "nothing there while closed"
    );

    click(cx, IN_TRIGGER);
    assert!(is_open(&size, cx));
    click(cx, IN_FIRST_OPTION);
    assert_eq!(size.update(cx, |size, _| size.value()), ["small"]);
    assert!(!is_open(&size, cx));
}

#[gpui::test]
fn a_press_outside_closes_the_popup(cx: &mut TestAppContext) {
    let (size, cx) = open(cx);
    click(cx, IN_TRIGGER);
    click(cx, FAR_AWAY);
    assert!(!is_open(&size, cx));
}

#[gpui::test]
fn pressing_the_trigger_again_closes_instead_of_reopening(cx: &mut TestAppContext) {
    let (size, cx) = open(cx);
    click(cx, IN_TRIGGER);
    click(cx, IN_TRIGGER);
    assert!(!is_open(&size, cx));
}

#[gpui::test]
fn an_agent_sees_and_controls_the_popup(cx: &mut TestAppContext) {
    let (size, cx) = open(cx);
    cx.update(|_, cx| {
        let (reference, before) = agent::snapshot(cx).remove(0);
        assert_eq!(before.expanded, Some(false));
        agent::act(cx, &reference, "open", None).unwrap();
        assert_eq!(agent::snapshot(cx)[0].1.expanded, Some(true));
        assert_eq!(
            agent::act(cx, &reference, "select", Some("large")).unwrap(),
            ["large"]
        );
    });
    assert!(is_open(&size, cx));
}
