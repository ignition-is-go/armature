use armature_core::convert::Cast;
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
        Self {
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

/// Puts focus on one part of a machine, as Tab or a press would.
#[expect(
    clippy::expect_used,
    reason = "a helper for tests fails as a test does"
)]
fn focus<P: armature_core::primitive::Primitive>(
    machine: &Entity<Machine<P>>,
    cx: &mut VisualTestContext,
    part: P::Part,
    item: Option<&str>,
) {
    machine.update_in(cx, |m, window, cx| {
        m.focus_handle(part, item)
            .expect("the test names a part that takes focus")
            .focus(window, cx);
    });
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
    focus(&quantity, cx, number_input::Part::Input, None);

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
    focus(&commands, cx, command::Part::Input, None);

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
            "Dropped milk at position 3 of 4."
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
    focus(&wifi, cx, toggle::Part::Root, None);

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
    focus(&confirm, cx, dialog::Part::Trigger, None);

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

#[gpui::test]
fn a_calendar_keeps_focus_when_the_keyboard_pages_to_another_month(cx: &mut TestAppContext) {
    use armature_core::calendar::{self, Calendar};
    use armature_core::date::Date;

    struct Page(Entity<Machine<Calendar>>);
    impl Render for Page {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let days: Vec<String> = self
                .0
                .read(cx)
                .primitive()
                .weeks()
                .iter()
                .flatten()
                .map(|day| day.format())
                .collect();
            self.0.update(cx, |calendar, cx| {
                calendar.track(calendar::Part::Day, days.clone(), cx);
            });
            let cells = days
                .iter()
                .map(|day| part(&self.0, calendar::Part::Day, Some(day), cx).size(px(ROW)));
            part(&self.0, calendar::Part::Grid, None, cx)
                .flex()
                .flex_wrap()
                .w(px(7. * ROW))
                .children(cells)
        }
    }
    let start = Date::parse("2026-11-30").unwrap();
    let (page, cx) = cx.add_window_view(|_, cx| {
        Page(cx.new(|cx| {
            Machine::new(
                Calendar::new(armature_core::calendar::Options::default(), start),
                "Date",
                [],
                cx,
            )
        }))
    });
    let calendar = page.update(cx, |page, _| page.0.clone());
    focus(&calendar, cx, calendar::Part::Day, Some("2026-11-30"));

    cx.simulate_keystrokes("down down enter");

    assert_eq!(
        calendar.update(cx, |c, _| c.primitive().month()),
        (2026, 12)
    );
    assert_eq!(
        calendar.update(cx, |c, _| c.value()),
        ["2026-12-14"],
        "Enter reached the day focus moved to"
    );
    let focused = calendar.update_in(cx, |c, window, _| {
        c.focus_handle(calendar::Part::Day, Some("2026-12-14"))
            .unwrap()
            .is_focused(window)
    });
    assert!(focused);
}

#[gpui::test]
fn dragging_a_divider_resizes_the_panels(cx: &mut TestAppContext) {
    use armature_core::resizable::{self, Resizable};

    struct Page(Entity<Machine<Resizable>>);
    impl Render for Page {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let share = self.0.read(cx).primitive().percent().cast::<f32>();
            part(&self.0, resizable::Part::Root, None, cx)
                .flex()
                .w(px(400.))
                .h(px(100.))
                // 4px for each percent of the 400px group, less half the 10px handle.
                .child(div().w(px(4.0f32.mul_add(share, -5.))))
                .child(
                    part(&self.0, resizable::Part::Handle, None, cx)
                        .w(px(10.))
                        .h_full(),
                )
        }
    }
    let (page, cx) = cx.add_window_view(|_, cx| {
        Page(cx.new(|cx| Machine::new(Resizable::default(), "Panels", [], cx)))
    });
    let panels = page.update(cx, |page, _| page.0.clone());

    down(cx, 200., 50.);
    drag_to(cx, 100., 50.);
    up(cx, 100., 50.);
    assert_eq!(panels.update(cx, |p, _| p.primitive().percent()), 25.0);
    cx.simulate_keystrokes("right end");
    assert_eq!(
        panels.update(cx, |p, _| p.primitive().percent()),
        90.0,
        "the drag focused the divider"
    );
}
