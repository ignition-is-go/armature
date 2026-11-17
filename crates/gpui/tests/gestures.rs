use std::time::Duration;

use armature_core::command::{self, Command};
use armature_core::number_input::{self, NumberInput};
use armature_core::primitive::Item;
use armature_core::slider::{self, Slider};
use armature_core::sortable::{self, Sortable};
use armature_core::toggle::{self, Toggle};
use armature_gpui::{Machine, part};
use gpui::prelude::*;
use gpui::{
    Context, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext, Window, div, point,
    px,
};

fn item(key: &str) -> Item {
    Item {
        key: key.into(),
        label: key.into(),
        disabled: false,
    }
}

/// Every primitive stacked in a column of fixed-height rows, so tests can aim at them.
struct Demo {
    volume: Entity<Machine<Slider>>,
    quantity: Entity<Machine<NumberInput>>,
    commands: Entity<Machine<Command>>,
    groceries: Entity<Machine<Sortable>>,
    wifi: Entity<Machine<Toggle>>,
}

const ROW: f32 = 20.;
const SLIDER_ROW: f32 = 0.;
const STEPPER_ROW: f32 = 20.;
const GROCERY_ROW: f32 = 120.;

impl Demo {
    fn new(cx: &mut Context<Self>) -> Self {
        let commands = [item("open file"), item("save file"), item("close window")];
        let groceries = [item("milk"), item("eggs"), item("bread"), item("jam")];
        Demo {
            volume: cx.new(|cx| Machine::new(Slider::default(), "Volume", [], cx)),
            quantity: cx.new(|cx| Machine::new(NumberInput::default(), "Quantity", [], cx)),
            commands: cx.new(|cx| Machine::new(Command::default(), "Commands", commands, cx)),
            groceries: cx.new(|cx| Machine::new(Sortable::default(), "Groceries", groceries, cx)),
            wifi: cx.new(|cx| Machine::new(Toggle::default(), "Wi-Fi", [], cx)),
        }
    }
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let row = || div().h(px(ROW)).w(px(200.));
        let option = |key| part(&self.commands, command::Part::Option, Some(key), cx).child(key);
        let grocery = |key| {
            part(&self.groceries, sortable::Part::Item, Some(key), cx)
                .h(px(ROW))
                .w(px(200.))
                .child(key)
        };
        div()
            .size_full()
            .child(
                part(&self.volume, slider::Part::Track, None, cx)
                    .h(px(ROW))
                    .w(px(200.))
                    .child(part(&self.volume, slider::Part::Thumb, None, cx).size(px(10.))),
            )
            .child(
                row()
                    .flex()
                    .child(
                        part(&self.quantity, number_input::Part::Decrement, None, cx).size(px(ROW)),
                    )
                    .child(
                        part(&self.quantity, number_input::Part::Input, None, cx)
                            .w(px(100.))
                            .h(px(ROW)),
                    )
                    .child(
                        part(&self.quantity, number_input::Part::Increment, None, cx).size(px(ROW)),
                    ),
            )
            .child(
                part(&self.commands, command::Part::Input, None, cx)
                    .h(px(ROW))
                    .w(px(200.)),
            )
            .child(
                part(&self.commands, command::Part::List, None, cx)
                    .h(px(3. * ROW))
                    .children(["open file", "save file", "close window"].map(option)),
            )
            .child(
                part(&self.groceries, sortable::Part::Root, None, cx)
                    .children(["milk", "eggs", "bread", "jam"].map(grocery)),
            )
            .child(part(&self.wifi, toggle::Part::Root, None, cx).size(px(ROW)))
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<Demo>, &mut VisualTestContext) {
    cx.add_window_view(|_, cx| Demo::new(cx))
}

fn down(cx: &mut VisualTestContext, x: f32, y: f32) {
    cx.simulate_mouse_down(point(px(x), px(y)), MouseButton::Left, Modifiers::default());
}

fn drag_to(cx: &mut VisualTestContext, x: f32, y: f32) {
    cx.simulate_mouse_move(point(px(x), px(y)), MouseButton::Left, Modifiers::default());
}

fn up(cx: &mut VisualTestContext, x: f32, y: f32) {
    cx.simulate_mouse_up(point(px(x), px(y)), MouseButton::Left, Modifiers::default());
}

#[gpui::test]
fn a_slider_follows_the_pointer_even_off_its_track(cx: &mut TestAppContext) {
    let (demo, cx) = open(cx);
    let volume = |cx: &mut VisualTestContext| {
        demo.update(cx, |d, cx| d.volume.read(cx).primitive().number())
    };

    down(cx, 100., SLIDER_ROW + 10.);
    assert_eq!(volume(cx), 50.0, "pressing the track moves the thumb there");
    drag_to(cx, 160., 400.);
    assert_eq!(volume(cx), 80.0, "the drag continues outside the track");
    up(cx, 160., 400.);
    drag_to(cx, 20., SLIDER_ROW + 10.);
    assert_eq!(volume(cx), 80.0, "movement after release is not a drag");

    cx.simulate_keystrokes("left pageup home");
    assert_eq!(
        volume(cx),
        0.0,
        "the press focused the thumb, so keys reach it"
    );
}

#[gpui::test]
fn a_held_stepper_button_repeats_until_released(cx: &mut TestAppContext) {
    let (demo, cx) = open(cx);
    let quantity = |cx: &mut VisualTestContext| {
        demo.update(cx, |d, cx| d.quantity.read(cx).primitive().number())
    };

    down(cx, 130., STEPPER_ROW + 10.);
    assert_eq!(quantity(cx), 1.0, "the press itself is one step");
    cx.executor().advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    assert_eq!(quantity(cx), 2.0, "the first repeat comes after the delay");
    cx.executor().advance_clock(Duration::from_millis(60));
    cx.run_until_parked();
    assert_eq!(quantity(cx), 3.0);

    up(cx, 130., STEPPER_ROW + 10.);
    cx.executor().advance_clock(Duration::from_millis(1000));
    cx.run_until_parked();
    assert_eq!(quantity(cx), 3.0, "released");
}

#[gpui::test]
fn typing_edits_a_number_field_and_enter_commits_it(cx: &mut TestAppContext) {
    let (demo, cx) = open(cx);
    let quantity = demo.update(cx, |d, _| d.quantity.clone());
    quantity.update_in(cx, |q, window, cx| {
        q.focus_handle(number_input::Part::Input, None)
            .unwrap()
            .focus(window, cx);
    });

    cx.simulate_keystrokes("backspace 4 2");
    let shown = quantity.update(cx, |q, _| q.text(number_input::Part::Input, None));
    assert_eq!(shown.as_deref(), Some("42"));
    assert_eq!(
        quantity.update(cx, |q, _| q.primitive().number()),
        0.0,
        "not committed yet"
    );
    cx.simulate_keystrokes("enter");
    assert_eq!(quantity.update(cx, |q, _| q.value()), ["42"]);
}

#[gpui::test]
fn typing_filters_a_command_palette_and_enter_runs_the_match(cx: &mut TestAppContext) {
    let (demo, cx) = open(cx);
    let commands = demo.update(cx, |d, _| d.commands.clone());
    commands.update_in(cx, |c, window, cx| {
        c.focus_handle(command::Part::Input, None)
            .unwrap()
            .focus(window, cx);
    });

    cx.simulate_keystrokes("s a v");
    commands.update(cx, |c, _| {
        assert_eq!(c.text(command::Part::Input, None).as_deref(), Some("sav"));
        assert!(c.semantics(command::Part::Option, Some("open file")).hidden);
        assert!(c.semantics(command::Part::Option, Some("save file")).active);
    });
    cx.simulate_keystrokes("enter");
    assert_eq!(commands.update(cx, |c, _| c.value()), ["save file"]);
}

#[gpui::test]
fn dragging_a_row_reorders_the_list_and_says_so(cx: &mut TestAppContext) {
    let (demo, cx) = open(cx);
    let groceries = demo.update(cx, |d, _| d.groceries.clone());

    down(cx, 50., GROCERY_ROW + 10.);
    drag_to(cx, 50., GROCERY_ROW + 20.);
    drag_to(cx, 50., GROCERY_ROW + 55.);
    up(cx, 50., GROCERY_ROW + 55.);

    groceries.update(cx, |g, _| {
        assert_eq!(g.value(), ["eggs", "bread", "milk", "jam"]);
        assert_eq!(
            g.announcements().last().unwrap().message,
            "milk dropped at position 3 of 4."
        );
    });
}

#[gpui::test]
fn the_owner_can_set_a_value_without_the_gate(cx: &mut TestAppContext) {
    let refused =
        cx.new(|cx| Machine::new(Toggle::default(), "Wi-Fi", [], cx).on_value_change(|_, _| false));
    refused.update(cx, |wifi, cx| wifi.set_value(vec!["on".into()], cx));
    assert!(refused.update(cx, |wifi, _| wifi.primitive().is_on()));
}

#[gpui::test]
fn enter_and_space_press_a_focused_switch_exactly_once(cx: &mut TestAppContext) {
    let (demo, cx) = open(cx);
    let wifi = demo.update(cx, |d, _| d.wifi.clone());
    wifi.update_in(cx, |w, window, cx| {
        w.focus_handle(toggle::Part::Root, None)
            .unwrap()
            .focus(window, cx);
    });

    cx.simulate_keystrokes("enter");
    assert!(wifi.update(cx, |w, _| w.primitive().is_on()));
    cx.simulate_keystrokes("space");
    assert!(!wifi.update(cx, |w, _| w.primitive().is_on()));
}

#[gpui::test]
fn a_dialog_opens_from_the_keyboard_and_escape_returns_focus(cx: &mut TestAppContext) {
    use armature_core::dialog::{self, Dialog};

    struct Page(Entity<Machine<Dialog>>);
    impl Render for Page {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .child(part(&self.0, dialog::Part::Trigger, None, cx).size(px(ROW)))
                .child(part(&self.0, dialog::Part::Backdrop, None, cx))
                .child(
                    part(&self.0, dialog::Part::Content, None, cx)
                        .child(part(&self.0, dialog::Part::Close, None, cx).size(px(ROW))),
                )
        }
    }
    let (page, cx) = cx.add_window_view(|_, cx| {
        Page(cx.new(|cx| Machine::new(Dialog::default(), "Confirm", [], cx)))
    });
    let confirm = page.update(cx, |page, _| page.0.clone());
    let focused = |cx: &mut VisualTestContext, part| {
        confirm.update_in(cx, |c, window, _| {
            c.focus_handle(part, None).unwrap().is_focused(window)
        })
    };
    confirm.update_in(cx, |c, window, cx| {
        c.focus_handle(dialog::Part::Trigger, None)
            .unwrap()
            .focus(window, cx);
    });

    cx.simulate_keystrokes("enter");
    assert!(confirm.update(cx, |c, _| c.primitive().is_open()));
    assert!(
        focused(cx, dialog::Part::Content),
        "focus moved into the dialog"
    );
    cx.simulate_keystrokes("escape");
    assert!(!confirm.update(cx, |c, _| c.primitive().is_open()));
    assert!(focused(cx, dialog::Part::Trigger));
}
