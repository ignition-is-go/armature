//! The contract features GPUI gets no help from the platform for: a text caret, Tab
//! trapping, hover timing, scrolling and the clipboard.

use std::time::Duration;

use armature_core::command::{self, Command};
use armature_core::data_grid::{self, DataGrid};
use armature_core::dialog::{self, Dialog};
use armature_core::editable::{self, Editable};
use armature_core::keys::Dir;
use armature_core::picker::{self, Picker};
use armature_core::primitive::{Item, Primitive};
use armature_core::toggle::{self, Toggle};
use armature_core::tooltip::{self, Tooltip};
use armature_core::virtual_list::{self, VirtualList};
use armature_gpui::{Machine, part};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClipboardItem, Context, Entity, Modifiers, TestAppContext, VisualTestContext,
    Window, div, point, px,
};

const ROW: f32 = 20.;

type Draw<P> = Box<dyn Fn(&Entity<Machine<P>>, &mut App) -> AnyElement>;

struct Page<P: Primitive> {
    machine: Entity<Machine<P>>,
    draw: Draw<P>,
}

impl<P: Primitive> Render for Page<P> {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        (self.draw)(&self.machine, cx)
    }
}

fn open<P: Primitive, E: IntoElement>(
    cx: &mut TestAppContext,
    primitive: P,
    draw: impl Fn(&Entity<Machine<P>>, &mut App) -> E + 'static,
) -> (Entity<Machine<P>>, &mut VisualTestContext) {
    let (page, cx) = cx.add_window_view(|_, cx| Page {
        machine: cx.new(|cx| Machine::new(primitive, "Test", Vec::<Item>::new(), cx)),
        draw: Box::new(move |machine, cx| draw(machine, cx).into_any_element()),
    });
    let machine = page.update(cx, |page, _| page.machine.clone());
    (machine, cx)
}

fn focus<P: Primitive>(
    machine: &Entity<Machine<P>>,
    cx: &mut VisualTestContext,
    part: P::Part,
    item: Option<&str>,
) {
    machine.update_in(cx, |m, window, cx| {
        m.focus_handle(part, item).unwrap().focus(window, cx)
    });
}

fn is_focused<P: Primitive>(
    machine: &Entity<Machine<P>>,
    cx: &mut VisualTestContext,
    part: P::Part,
) -> bool {
    machine.update_in(cx, |m, window, _| {
        m.focus_handle(part, None).unwrap().is_focused(window)
    })
}

#[gpui::test]
fn text_is_edited_at_a_caret_that_arrows_move(cx: &mut TestAppContext) {
    let (commands, cx) = open(
        cx,
        Command::default(),
        |commands: &Entity<Machine<Command>>, cx: &mut App| {
            part(commands, command::Part::Input, None, cx).size(px(ROW))
        },
    );
    focus(&commands, cx, command::Part::Input, None);

    cx.simulate_keystrokes("a c left b");
    let state = |cx: &mut VisualTestContext| {
        commands.update(cx, |c, _| {
            (
                c.text(command::Part::Input, None).unwrap(),
                c.caret(command::Part::Input, None),
            )
        })
    };
    assert_eq!(state(cx), ("abc".to_string(), 2), "typed in the middle");
    cx.simulate_keystrokes("delete home x backspace end d");
    assert_eq!(state(cx), ("abd".to_string(), 3));
}

#[gpui::test]
fn tab_stays_inside_an_open_dialog(cx: &mut TestAppContext) {
    let (confirm, cx) = open(
        cx,
        Dialog::default(),
        |confirm: &Entity<Machine<Dialog>>, cx: &mut App| {
            div()
                .child(part(confirm, dialog::Part::Trigger, None, cx).size(px(ROW)))
                .child(
                    part(confirm, dialog::Part::Content, None, cx)
                        .child(part(confirm, dialog::Part::Close, None, cx).size(px(ROW))),
                )
        },
    );
    focus(&confirm, cx, dialog::Part::Trigger, None);
    cx.simulate_keystrokes("enter");
    assert!(is_focused(&confirm, cx, dialog::Part::Content));

    cx.simulate_keystrokes("tab");
    assert!(is_focused(&confirm, cx, dialog::Part::Close));
    cx.simulate_keystrokes("tab");
    assert!(
        is_focused(&confirm, cx, dialog::Part::Close),
        "nowhere else to go"
    );
    cx.simulate_keystrokes("shift-tab");
    assert!(
        !is_focused(&confirm, cx, dialog::Part::Trigger),
        "the trigger is outside the dialog"
    );
}

#[gpui::test]
fn a_tooltip_shows_after_the_pointer_rests_and_hides_when_it_leaves(cx: &mut TestAppContext) {
    let (help, cx) = open(
        cx,
        Tooltip::default(),
        |help: &Entity<Machine<Tooltip>>, cx: &mut App| {
            div()
                .size_full()
                .child(
                    part(help, tooltip::Part::Trigger, None, cx)
                        .w(px(100.))
                        .h(px(ROW)),
                )
                .child(part(help, tooltip::Part::Content, None, cx).child("Saves your work"))
        },
    );
    let shown = |cx: &mut VisualTestContext| help.update(cx, |h, _| h.primitive().is_open());

    cx.simulate_mouse_move(point(px(10.), px(10.)), None, Modifiers::default());
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    assert!(!shown(cx), "not yet");
    cx.executor().advance_clock(Duration::from_millis(250));
    cx.run_until_parked();
    assert!(shown(cx));

    cx.simulate_mouse_move(point(px(300.), px(300.)), None, Modifiers::default());
    cx.run_until_parked();
    assert!(!shown(cx));
}

#[gpui::test]
fn a_long_list_renders_only_what_is_in_view_and_keys_scroll_it(cx: &mut TestAppContext) {
    let (rows, cx) = open(
        cx,
        VirtualList::new(10_000, ROW as f64),
        |rows: &Entity<Machine<VirtualList>>, cx: &mut App| {
            let visible = rows.read(cx).primitive().rows();
            let keys: Vec<String> = visible.iter().map(|row| row.key.clone()).collect();
            rows.update(cx, |m, cx| m.track(virtual_list::Part::Row, keys, cx));
            let total = rows.read(cx).primitive().total_size() as f32;
            let drawn = visible.into_iter().map(|row| {
                part(rows, virtual_list::Part::Row, Some(&row.key), cx)
                    .absolute()
                    .top(px(row.offset as f32))
                    .h(px(ROW))
                    .w_full()
            });
            part(rows, virtual_list::Part::Viewport, None, cx)
                .h(px(100.))
                .w(px(200.))
                .child(
                    div()
                        .relative()
                        .h(px(total))
                        .children(drawn.collect::<Vec<_>>()),
                )
        },
    );
    cx.run_until_parked();
    let rendered = |cx: &mut VisualTestContext| {
        rows.update(cx, |m, _| {
            m.primitive()
                .rows()
                .iter()
                .map(|row| row.index)
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(
        rendered(cx),
        (0..9).collect::<Vec<_>>(),
        "five in view and four spare"
    );

    focus(&rows, cx, virtual_list::Part::Row, Some("0"));
    cx.simulate_keystrokes("end");
    cx.run_until_parked();
    assert_eq!(rendered(cx).last(), Some(&9999));
    assert!(rendered(cx).len() < 12);
    cx.simulate_keystrokes("enter");
    assert_eq!(rows.update(cx, |m, _| m.value()), ["9999"]);

    let reference = cx.update(|_, cx| armature_gpui::agent::snapshot(cx)[0].0.clone());
    cx.update(|_, cx| {
        armature_gpui::agent::act(cx, &reference, "scroll_to", Some("5000")).unwrap()
    });
    cx.run_until_parked();
    assert!(rendered(cx).contains(&5000));
    let scrolled = rows.update(cx, |m, _| m.primitive().wanted_scroll().unwrap());
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    assert!(
        rendered(cx).contains(&5001),
        "the box really scrolled, so the next row is in view"
    );
    assert_eq!(
        rows.update(cx, |m, _| m.primitive().wanted_scroll().unwrap()),
        scrolled
    );
}

#[gpui::test]
fn a_grid_copies_its_selection_and_pastes_from_the_clipboard(cx: &mut TestAppContext) {
    let sheet = vec![
        vec!["a1".to_string(), "b1".into()],
        vec!["a2".into(), "b2".into()],
    ];
    let (grid, cx) = open(
        cx,
        DataGrid::new(sheet, Dir::Ltr),
        |grid: &Entity<Machine<DataGrid>>, cx: &mut App| {
            let keys = ["0,0", "0,1", "1,0", "1,1"];
            grid.update(cx, |m, cx| {
                m.track(data_grid::Part::Cell, keys.map(String::from), cx)
            });
            let cells =
                keys.map(|key| part(grid, data_grid::Part::Cell, Some(key), cx).size(px(ROW)));
            part(grid, data_grid::Part::Root, None, cx).children(cells)
        },
    );
    cx.run_until_parked();
    focus(&grid, cx, data_grid::Part::Cell, Some("0,0"));

    cx.simulate_keystrokes("shift-down ctrl-c");
    let copied = cx.update(|_, cx| {
        cx.read_from_clipboard()
            .and_then(|clipboard| clipboard.text())
    });
    assert_eq!(copied.as_deref(), Some("a1\na2"));

    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("X\tY".into())));
    cx.simulate_keystrokes("ctrl-v");
    let row = grid.update(cx, |m, _| {
        let cell = |col| {
            m.primitive()
                .text_of(armature_core::grid::Cell { row: 0, col })
                .to_string()
        };
        (cell(0), cell(1))
    });
    assert_eq!(
        row,
        ("X".to_string(), "Y".to_string()),
        "pasted from the selection's corner"
    );
}

#[gpui::test]
fn text_is_edited_in_place_and_saved_when_focus_leaves(cx: &mut TestAppContext) {
    struct Both {
        title: Entity<Machine<Editable>>,
        other: Entity<Machine<Toggle>>,
    }
    impl Render for Both {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .child(part(&self.title, editable::Part::Preview, None, cx).size(px(ROW)))
                .child(part(&self.title, editable::Part::Input, None, cx).size(px(ROW)))
                .child(part(&self.other, toggle::Part::Root, None, cx).size(px(ROW)))
        }
    }
    let (page, cx) = cx.add_window_view(|_, cx| Both {
        title: cx
            .new(|cx| Machine::new(Editable::new("Untitled"), "Title", Vec::<Item>::new(), cx)),
        other: cx.new(|cx| Machine::new(Toggle::default(), "Other", Vec::<Item>::new(), cx)),
    });
    let (title, other) = page.update(cx, |page, _| (page.title.clone(), page.other.clone()));

    // Focus changes are only reported in the active window.
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    focus(&title, cx, editable::Part::Preview, None);
    cx.simulate_keystrokes("f2");
    assert!(
        is_focused(&title, cx, editable::Part::Input),
        "F2 opens the field"
    );
    cx.simulate_keystrokes("! escape");
    assert_eq!(
        title.read_with(cx, |m, _| m.value()),
        ["Untitled"],
        "Escape puts the text back"
    );

    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("! !");
    assert_eq!(
        title.read_with(cx, |m, _| m.value()),
        ["Untitled"],
        "not saved while typing"
    );
    focus(&other, cx, toggle::Part::Root, None);
    // The window reports focus changes when it next draws.
    cx.simulate_keystrokes("space");
    assert_eq!(
        title.read_with(cx, |m, _| m.value()),
        ["Untitled!!"],
        "leaving the field saves"
    );
}

#[gpui::test]
fn a_wheel_picker_scrolls_to_what_the_keys_choose_and_follows_the_scroll(cx: &mut TestAppContext) {
    let hours: Vec<Item> = (1..=12)
        .map(|hour| Item {
            key: hour.to_string(),
            label: hour.to_string(),
            disabled: false,
        })
        .collect();
    let (page, cx) = cx.add_window_view(|_, cx| Page {
        machine: cx.new(|cx| Machine::new(Picker::new(ROW as f64), "Hour", hours, cx)),
        draw: Box::new(|picker: &Entity<Machine<Picker>>, cx: &mut App| {
            let options = (1..=12).map(|hour| {
                part(picker, picker::Part::Option, Some(&hour.to_string()), cx)
                    .h(px(ROW))
                    .w_full()
            });
            part(picker, picker::Part::Viewport, None, cx)
                .h(px(100.))
                .w(px(80.))
                .child(div().py(px(40.)).children(options.collect::<Vec<_>>()))
                .into_any_element()
        }),
    });
    let picker = page.update(cx, |page, _| page.machine.clone());
    cx.run_until_parked();

    assert!(
        picker.read_with(cx, |m, _| m.value()).is_empty(),
        "nothing is chosen until someone chooses"
    );
    focus(&picker, cx, picker::Part::Viewport, None);
    cx.simulate_keystrokes("down down down");
    cx.run_until_parked();
    assert_eq!(picker.read_with(cx, |m, _| m.value()), ["3"]);
    assert_eq!(
        picker.read_with(cx, |m, _| m.primitive().wanted_scroll()),
        Some(2. * ROW as f64)
    );

    // The column really moved, and once it rests the choice it reports is unchanged.
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    assert_eq!(picker.read_with(cx, |m, _| m.value()), ["3"]);

    cx.simulate_keystrokes("end");
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    assert_eq!(picker.read_with(cx, |m, _| m.value()), ["12"]);
}
