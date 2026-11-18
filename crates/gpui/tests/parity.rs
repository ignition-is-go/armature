//! The contract features GPUI gets no help from the platform for: a text caret, Tab
//! trapping, hover timing, scrolling and the clipboard.

use armature_core::convert::Cast;
use std::time::Duration;

use armature_core::command::{self, Command};
use armature_core::data_grid::{self, DataGrid};
use armature_core::dialog::{self, Dialog};
use armature_core::editable::{self, Editable};
use armature_core::file_input::{self, FileInput};
use armature_core::keys::Dir;
use armature_core::menu::{self, Entry, Kind as EntryKind, Menu};
use armature_core::navigation_menu::{self, NavigationMenu};
use armature_core::number::NumberRange;
use armature_core::number_input::{self, NumberInput};
use armature_core::picker::{self, Picker};
use armature_core::popover::{self, Popover};
use armature_core::primitive::{Item, Primitive};
use armature_core::text_field::{self, Kind as FieldKind, TextField};
use armature_core::toast::{self, Toast, Toaster};
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

#[expect(
    clippy::expect_used,
    reason = "a helper for tests fails as a test does"
)]
fn focus<P: Primitive>(
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

fn is_focused<P: Primitive>(
    machine: &Entity<Machine<P>>,
    cx: &mut VisualTestContext,
    part: P::Part,
) -> bool {
    machine.update_in(cx, |m, window, _| {
        m.focus_handle(part, None)
            .is_some_and(|handle| handle.is_focused(window))
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
fn a_tooltip_stays_while_the_pointer_crosses_onto_it(cx: &mut TestAppContext) {
    let (help, cx) = open(
        cx,
        Tooltip::default(),
        |help: &Entity<Machine<Tooltip>>, cx: &mut App| {
            // The tooltip opens above its trigger, so the trigger sits a little way down.
            div()
                .size_full()
                .pt(px(60.))
                .child(
                    part(help, tooltip::Part::Trigger, None, cx)
                        .w(px(100.))
                        .h(px(ROW)),
                )
                .child(
                    part(help, tooltip::Part::Content, None, cx)
                        .w(px(100.))
                        .h(px(ROW))
                        .child("Saves your work"),
                )
        },
    );
    let shown = |cx: &mut VisualTestContext| help.update(cx, |h, _| h.primitive().is_open());

    cx.simulate_mouse_move(point(px(10.), px(70.)), None, Modifiers::default());
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    assert!(shown(cx));

    // Off the trigger, over the gap, and onto the tooltip above it.
    cx.simulate_mouse_move(point(px(10.), px(57.)), None, Modifiers::default());
    cx.simulate_mouse_move(point(px(10.), px(44.)), None, Modifiers::default());
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    assert!(shown(cx), "arriving on the tooltip keeps it");

    cx.simulate_mouse_move(point(px(300.), px(300.)), None, Modifiers::default());
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    assert!(!shown(cx), "and leaving the tooltip closes it");
}

#[gpui::test]
fn a_tooltip_stays_when_the_pointer_lands_on_it_in_one_move(cx: &mut TestAppContext) {
    let (help, cx) = open(
        cx,
        Tooltip::default(),
        |help: &Entity<Machine<Tooltip>>, cx: &mut App| {
            // The tooltip opens above its trigger, so the trigger sits a little way down.
            div()
                .size_full()
                .pt(px(60.))
                .child(
                    part(help, tooltip::Part::Trigger, None, cx)
                        .w(px(100.))
                        .h(px(ROW)),
                )
                .child(
                    part(help, tooltip::Part::Content, None, cx)
                        .w(px(100.))
                        .h(px(ROW))
                        .child("Saves your work"),
                )
        },
    );
    let shown = |cx: &mut VisualTestContext| help.update(cx, |h, _| h.primitive().is_open());

    cx.simulate_mouse_move(point(px(10.), px(70.)), None, Modifiers::default());
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    assert!(shown(cx));

    // Straight from the trigger onto the tooltip, with no move in between.
    cx.simulate_mouse_move(point(px(10.), px(44.)), None, Modifiers::default());
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    assert!(shown(cx), "arriving on the tooltip keeps it");

    cx.simulate_mouse_move(point(px(300.), px(300.)), None, Modifiers::default());
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    assert!(!shown(cx), "and leaving the tooltip closes it");
}

#[gpui::test]
fn a_long_list_renders_only_what_is_in_view_and_keys_scroll_it(cx: &mut TestAppContext) {
    let (rows, cx) = open(
        cx,
        VirtualList::new(10_000, f64::from(ROW)),
        |rows: &Entity<Machine<VirtualList>>, cx: &mut App| {
            let visible = rows.read(cx).primitive().rows();
            let keys: Vec<String> = visible.iter().map(|row| row.key.clone()).collect();
            rows.update(cx, |m, cx| m.track(virtual_list::Part::Row, keys, cx));
            let total = rows.read(cx).primitive().total_size().cast::<f32>();
            let drawn = visible.into_iter().map(|row| {
                part(rows, virtual_list::Part::Row, Some(&row.key), cx)
                    .absolute()
                    .top(px(row.offset.cast::<f32>()))
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
                m.track(data_grid::Part::Cell, keys.map(String::from), cx);
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
        machine: cx.new(|cx| Machine::new(Picker::new(f64::from(ROW)), "Hour", hours, cx)),
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
        Some(2. * f64::from(ROW))
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

#[gpui::test]
fn a_popup_is_tied_to_its_trigger_and_title_in_the_accessibility_tree(cx: &mut TestAppContext) {
    let (popover, cx) = open(
        cx,
        Popover::default(),
        |popover: &Entity<Machine<Popover>>, cx: &mut App| {
            div()
                .child(part(popover, popover::Part::Trigger, None, cx).size(px(ROW)))
                .child(
                    part(popover, popover::Part::Content, None, cx)
                        .child(
                            part(popover, popover::Part::Title, None, cx).child("Share this page"),
                        )
                        .child(
                            part(popover, popover::Part::Description, None, cx)
                                .child("Anyone can view."),
                        ),
                )
        },
    );
    focus(&popover, cx, popover::Part::Trigger, None);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();

    let links = |kind: popover::Part| popover.read_with(cx, |m, _| m.links(kind, None));
    let node = |kind: popover::Part| {
        popover
            .read_with(cx, |m, _| m.node(kind, None))
            .expect("drawn")
    };
    assert_ne!(node(popover::Part::Title), node(popover::Part::Content));
    assert_eq!(
        links(popover::Part::Trigger)[1],
        Some(node(popover::Part::Content)),
        "controls"
    );
    assert_eq!(
        links(popover::Part::Content)[0],
        Some(node(popover::Part::Title)),
        "labelled by"
    );
    assert_eq!(
        links(popover::Part::Content)[2],
        Some(node(popover::Part::Description)),
        "described by"
    );
}

#[gpui::test]
fn a_press_while_a_dialog_is_up_does_not_close_the_popover_under_it(cx: &mut TestAppContext) {
    struct Both {
        share: Entity<Machine<Popover>>,
        confirm: Entity<Machine<Dialog>>,
    }
    impl Render for Both {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .child(part(&self.share, popover::Part::Trigger, None, cx).size(px(ROW)))
                .child(part(&self.share, popover::Part::Content, None, cx).size(px(ROW)))
                .child(part(&self.confirm, dialog::Part::Content, None, cx).size(px(ROW)))
        }
    }
    let (page, cx) = cx.add_window_view(|_, cx| Both {
        share: cx.new(|cx| Machine::new(Popover::default(), "Share", Vec::<Item>::new(), cx)),
        confirm: cx.new(|cx| Machine::new(Dialog::default(), "Confirm", Vec::<Item>::new(), cx)),
    });
    let share = page.update(cx, |page, _| page.share.clone());
    let act = |cx: &mut VisualTestContext, label: &str, action: &str| {
        cx.update(|_, cx| {
            let found = armature_gpui::agent::snapshot(cx)
                .into_iter()
                .find(|(_, s)| s.label == label)
                .unwrap();
            armature_gpui::agent::act(cx, &found.0, action, None).unwrap();
        });
        cx.run_until_parked();
    };
    let open = |cx: &mut VisualTestContext| {
        share.read_with(cx, |m, _| m.snapshot().expanded) == Some(true)
    };

    act(cx, "Share", "open");
    act(cx, "Confirm", "open");
    cx.simulate_click(point(px(300.), px(300.)), Modifiers::default());
    assert!(open(cx), "the dialog is in the way");

    act(cx, "Confirm", "close");
    cx.simulate_click(point(px(300.), px(300.)), Modifiers::default());
    assert!(
        !open(cx),
        "with the dialog gone, a press elsewhere closes it"
    );
}

#[gpui::test]
fn a_toast_set_from_outside_starts_its_own_clock_and_leaves(cx: &mut TestAppContext) {
    let (toaster, cx) = open(
        cx,
        Toaster::default(),
        |toaster: &Entity<Machine<Toaster>>, cx: &mut App| {
            let shown: Vec<String> = toaster
                .read(cx)
                .primitive()
                .visible()
                .iter()
                .map(|toast| toast.id.clone())
                .collect();
            part(toaster, toast::Part::Region, None, cx)
                .size(px(100.))
                .children(
                    shown
                        .iter()
                        .map(|id| part(toaster, toast::Part::Toast, Some(id), cx).h(px(ROW)))
                        .collect::<Vec<_>>(),
                )
        },
    );
    let said = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    cx.update(|_, cx| {
        let said = said.clone();
        cx.subscribe(
            &toaster,
            move |_, announcement: &armature_gpui::Announcement, _| {
                said.borrow_mut().push(announcement.message.clone());
            },
        )
        .detach();
    });
    toaster.update(cx, |m, cx| {
        m.set_value(vec![Toast::new("a", "Saved").encode()], cx);
    });
    cx.run_until_parked();
    // A test window redraws on an event, so nudge it.
    cx.simulate_mouse_move(point(px(300.), px(300.)), None, Modifiers::default());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    assert_eq!(*said.borrow(), ["Saved"], "read out once it shows");
    assert_eq!(toaster.read_with(cx, |m, _| m.value().len()), 1);

    cx.executor().advance_clock(Duration::from_millis(6000));
    cx.run_until_parked();
    assert!(
        toaster.read_with(cx, |m, _| m.value().is_empty()),
        "its five seconds are up"
    );
}

#[gpui::test]
fn a_menu_drills_into_a_submenu_and_back_without_losing_focus(cx: &mut TestAppContext) {
    let entries = vec![
        Entry::action("new", "New file"),
        Entry::action("share", "Share").of(EntryKind::Submenu),
        Entry::action("email", "Email").inside("share"),
    ];
    let ran = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let heard = ran.clone();
    let (page, cx) = cx.add_window_view(|_, cx| Page {
        machine: cx.new(|cx| {
            Machine::new(Menu::new(entries), "Actions", Vec::<Item>::new(), cx).on_value_change(
                move |change, _| {
                    heard.borrow_mut().extend(change.value.iter().cloned());
                    true
                },
            )
        }),
        draw: Box::new(|menu: &Entity<Machine<Menu>>, cx: &mut App| {
            let shown: Vec<String> = menu
                .read(cx)
                .primitive()
                .shown()
                .iter()
                .map(|entry| entry.key.clone())
                .collect();
            menu.update(cx, |m, cx| m.track(menu::Part::Item, shown.clone(), cx));
            div()
                .child(part(menu, menu::Part::Trigger, None, cx).size(px(ROW)))
                .child(
                    part(menu, menu::Part::Content, None, cx).children(
                        shown
                            .iter()
                            .map(|key| part(menu, menu::Part::Item, Some(key), cx).h(px(ROW)))
                            .collect::<Vec<_>>(),
                    ),
                )
                .into_any_element()
        }),
    });
    let menu = page.update(cx, |page, _| page.machine.clone());
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    let focused = |cx: &mut VisualTestContext, key: &str| {
        menu.update_in(cx, |m, window, _| {
            m.focus_handle(menu::Part::Item, Some(key))
                .is_some_and(|handle| handle.is_focused(window))
        })
    };

    focus(&menu, cx, menu::Part::Trigger, None);
    cx.simulate_keystrokes("down down right");
    cx.run_until_parked();
    assert!(focused(cx, "email"), "inside the submenu");
    assert_eq!(
        menu.read_with(cx, |m, _| m.modality()),
        armature_core::preference::Modality::Keyboard
    );
    assert!(menu.read_with(cx, |m, _| m.primitive().is_open()));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(focused(cx, "share"), "back on the entry that opened it");
    cx.simulate_keystrokes("up enter");
    cx.run_until_parked();
    assert_eq!(*ran.borrow(), ["run:new"]);
    assert!(is_focused(&menu, cx, menu::Part::Trigger));
}

#[gpui::test]
fn a_text_field_shows_its_problem_once_it_is_left(cx: &mut TestAppContext) {
    struct Both {
        email: Entity<Machine<TextField>>,
        other: Entity<Machine<Toggle>>,
    }
    impl Render for Both {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .child(part(&self.email, text_field::Part::Input, None, cx).size(px(ROW)))
                .child(part(&self.email, text_field::Part::Error, None, cx).size(px(ROW)))
                .child(part(&self.other, toggle::Part::Root, None, cx).size(px(ROW)))
        }
    }
    let (page, cx) = cx.add_window_view(|_, cx| Both {
        email: cx.new(|cx| {
            let field = TextField::new(FieldKind::Text).validate(|text| {
                (!text.contains('@')).then(|| "Enter an email address.".to_string())
            });
            Machine::new(field, "Email", Vec::<Item>::new(), cx)
        }),
        other: cx.new(|cx| Machine::new(Toggle::default(), "Other", Vec::<Item>::new(), cx)),
    });
    let (email, other) = page.update(cx, |page, _| (page.email.clone(), page.other.clone()));
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();

    focus(&email, cx, text_field::Part::Input, None);
    cx.simulate_keystrokes("a d a");
    assert_eq!(email.read_with(cx, |m, _| m.value()), ["ada"]);
    assert!(
        email.read_with(cx, |m, _| m.semantics(text_field::Part::Error, None).hidden),
        "not while typing"
    );
    focus(&other, cx, toggle::Part::Root, None);
    cx.simulate_keystrokes("space");
    assert!(!email.read_with(cx, |m, _| m.semantics(text_field::Part::Error, None).hidden));
    assert!(email.read_with(cx, |m, _| {
        m.semantics(text_field::Part::Input, None).invalid
    }));
}

#[gpui::test]
fn the_toast_hotkey_moves_focus_into_the_stack_and_back(cx: &mut TestAppContext) {
    struct Both {
        other: Entity<Machine<Toggle>>,
        toaster: Entity<Machine<Toaster>>,
    }
    impl Render for Both {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.toaster.update(cx, |m, cx| {
                m.track(toast::Part::Close, ["a".to_string()], cx);
            });
            div()
                .child(part(&self.other, toggle::Part::Root, None, cx).size(px(ROW)))
                .child(
                    part(&self.toaster, toast::Part::Region, None, cx).child(
                        part(&self.toaster, toast::Part::Close, Some("a"), cx).size(px(ROW)),
                    ),
                )
        }
    }
    let (page, cx) = cx.add_window_view(|_, cx| Both {
        other: cx.new(|cx| Machine::new(Toggle::default(), "Other", Vec::<Item>::new(), cx)),
        toaster: cx.new(|cx| {
            let mut toaster = Toaster::default();
            toaster.set_value(vec![Toast::new("a", "Saved").lasting(0).encode()]);
            Machine::new(toaster, "Notifications", Vec::<Item>::new(), cx)
        }),
    });
    let (other, toaster) = page.update(cx, |page, _| (page.other.clone(), page.toaster.clone()));
    cx.update(|window, _| window.activate_window());
    cx.simulate_mouse_move(point(px(300.), px(300.)), None, Modifiers::default());
    cx.run_until_parked();

    focus(&other, cx, toggle::Part::Root, None);
    cx.simulate_keystrokes("f6");
    cx.run_until_parked();
    let on_close = toaster.update_in(cx, |m, window, _| {
        m.focus_handle(toast::Part::Close, Some("a"))
            .unwrap()
            .is_focused(window)
    });
    assert!(on_close, "into the stack");
    cx.simulate_keystrokes("f6");
    cx.run_until_parked();
    assert!(
        is_focused(&other, cx, toggle::Part::Root),
        "and back to where it was"
    );
}

#[gpui::test]
fn a_right_click_opens_a_menu_where_the_pointer_is(cx: &mut TestAppContext) {
    let entries = vec![
        Entry::action("copy", "Copy"),
        Entry::action("paste", "Paste"),
    ];
    let (menu, cx) = open(
        cx,
        Menu::new(entries),
        |menu: &Entity<Machine<Menu>>, cx: &mut App| {
            let shown: Vec<String> = menu
                .read(cx)
                .primitive()
                .shown()
                .iter()
                .map(|entry| entry.key.clone())
                .collect();
            menu.update(cx, |m, cx| m.track(menu::Part::Item, shown.clone(), cx));
            div()
                .size_full()
                .child(part(menu, menu::Part::Area, None, cx).size(px(400.)))
                .child(
                    part(menu, menu::Part::Content, None, cx).children(
                        shown
                            .iter()
                            .map(|key| {
                                part(menu, menu::Part::Item, Some(key), cx)
                                    .h(px(ROW))
                                    .w(px(80.))
                            })
                            .collect::<Vec<_>>(),
                    ),
                )
        },
    );
    cx.simulate_mouse_down(
        point(px(120.), px(90.)),
        gpui::MouseButton::Right,
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        point(px(120.), px(90.)),
        gpui::MouseButton::Right,
        Modifiers::default(),
    );
    cx.run_until_parked();
    assert!(menu.read_with(cx, |m, _| m.primitive().is_open()));
    let at = menu
        .read_with(cx, |m, _| m.primitive().popup_point())
        .unwrap();
    assert_eq!((at.x, at.y), (120.0, 90.0));
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    assert!(!menu.read_with(cx, |m, _| m.primitive().is_open()));
}

#[gpui::test]
fn the_wheel_steps_a_number_field_only_while_it_has_focus(cx: &mut TestAppContext) {
    let (amount, cx) = open(
        cx,
        NumberInput::new(NumberRange::new(0.0, 100.0, 1.0), 20.0),
        |amount: &Entity<Machine<NumberInput>>, cx: &mut App| {
            part(amount, number_input::Part::Input, None, cx).size(px(100.))
        },
    );
    let wheel = |cx: &mut VisualTestContext| {
        cx.simulate_event(gpui::ScrollEvent {
            position: point(px(50.), px(50.)),
            delta: gpui::ScrollDelta::Lines(point(0., 1.)),
            ..Default::default()
        });
        cx.run_until_parked();
    };
    wheel(cx);
    assert_eq!(
        amount.read_with(cx, |m, _| m.value()),
        ["20"],
        "scrolling past it leaves it alone"
    );
    focus(&amount, cx, number_input::Part::Input, None);
    cx.simulate_keystrokes("up");
    wheel(cx);
    assert_eq!(
        amount.read_with(cx, |m, _| m.value()),
        ["22"],
        "one from the key, one from the wheel"
    );
}

#[gpui::test]
fn files_dragged_over_a_dropzone_light_it_up_and_dropping_takes_them(cx: &mut TestAppContext) {
    let (files, cx) = open(
        cx,
        FileInput::default(),
        |files: &Entity<Machine<FileInput>>, cx: &mut App| {
            div()
                .size_full()
                .child(part(files, file_input::Part::Dropzone, None, cx).size(px(100.)))
        },
    );
    let over = |cx: &mut VisualTestContext| {
        files.read_with(cx, |m, _| {
            m.semantics(file_input::Part::Dropzone, None).active
        })
    };
    let paths = gpui::ExternalPaths(vec![std::path::PathBuf::from("/nowhere/report.pdf")].into());
    cx.simulate_event(gpui::FileDropEvent::Entered {
        position: point(px(300.), px(300.)),
        paths,
    });
    cx.simulate_event(gpui::FileDropEvent::Pending {
        position: point(px(50.), px(50.)),
    });
    cx.run_until_parked();
    assert!(over(cx), "the files are over the dropzone");
    cx.simulate_event(gpui::FileDropEvent::Submit {
        position: point(px(50.), px(50.)),
    });
    cx.run_until_parked();
    assert_eq!(files.read_with(cx, |m, _| m.value()), ["report.pdf"]);
    assert!(!over(cx));
}

#[gpui::test]
fn a_panel_stays_open_while_the_pointer_crosses_the_gap_to_it(cx: &mut TestAppContext) {
    let nav = NavigationMenu::new(navigation_menu::Options::default(), &[("products", true)]);
    let (nav, cx) = open(
        cx,
        nav,
        |nav: &Entity<Machine<NavigationMenu>>, cx: &mut App| {
            div()
                .size_full()
                .child(
                    part(nav, navigation_menu::Part::Trigger, Some("products"), cx)
                        .w(px(80.))
                        .h(px(ROW)),
                )
                .child(
                    part(nav, navigation_menu::Part::Panel, Some("products"), cx)
                        .w(px(120.))
                        .h(px(60.)),
                )
        },
    );
    let open_now =
        |cx: &mut VisualTestContext| nav.read_with(cx, |m, _| m.primitive().open_panel().is_some());
    let move_to = |cx: &mut VisualTestContext, x: f32, y: f32, wait_ms: u64| {
        cx.simulate_mouse_move(point(px(x), px(y)), None, Modifiers::default());
        cx.executor().advance_clock(Duration::from_millis(wait_ms));
        cx.run_until_parked();
    };
    move_to(cx, 300., 300., 10);
    move_to(cx, 10., 10., 100);
    assert!(open_now(cx), "resting on the trigger opens its panel");

    // The panel sits a little below the trigger. Crossing that gap takes longer
    // than the close delay.
    move_to(cx, 12., 21., 60);
    move_to(cx, 14., 22., 60);
    assert!(open_now(cx), "still on its way to the panel");
    move_to(cx, 16., 40., 100);
    assert!(open_now(cx), "arrived");

    move_to(cx, 300., 300., 200);
    assert!(!open_now(cx), "gone elsewhere");
}

#[gpui::test]
fn an_outline_row_is_picked_up_moved_with_its_children_and_dropped(cx: &mut TestAppContext) {
    use armature_core::outline::{self, Outline, OutlineRow};
    let rows: Vec<OutlineRow> = [
        ("inbox", 0),
        ("projects", 0),
        ("site", 1),
        ("copy", 2),
        ("app", 1),
    ]
    .iter()
    .map(|(key, depth)| OutlineRow {
        key: key.to_string(),
        depth: *depth,
    })
    .collect();
    let changes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let heard = changes.clone();
    let (page, cx) = cx.add_window_view(|_, cx| Page {
        machine: cx.new(|cx| {
            Machine::new(
                Outline::new(outline::Options::default(), rows),
                "Notes",
                Vec::<Item>::new(),
                cx,
            )
            .on_value_change(move |change, _| {
                heard.borrow_mut().push(change.value.clone());
                true
            })
        }),
        draw: Box::new(|outline: &Entity<Machine<Outline>>, cx: &mut App| {
            let keys: Vec<String> = outline
                .read(cx)
                .primitive()
                .rows()
                .iter()
                .map(|row| row.key.clone())
                .collect();
            outline.update(cx, |m, cx| m.track(outline::Part::Row, keys.clone(), cx));
            part(outline, outline::Part::Root, None, cx)
                .children(
                    keys.iter()
                        .map(|key| part(outline, outline::Part::Row, Some(key), cx).h(px(ROW)))
                        .collect::<Vec<_>>(),
                )
                .into_any_element()
        }),
    });
    let outline = page.update(cx, |page, _| page.machine.clone());
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();

    focus(&outline, cx, outline::Part::Row, Some("site"));
    cx.simulate_keystrokes("space down");
    cx.run_until_parked();
    let drawn = outline.read_with(cx, |m, _| {
        m.primitive()
            .rows()
            .iter()
            .map(|row| format!("{}:{}", row.depth, row.key))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        drawn,
        ["0:inbox", "0:projects", "1:app", "1:site", "2:copy"]
    );
    assert!(
        changes.borrow().is_empty(),
        "nothing is committed while the row is up"
    );
    let still_focused = outline.update_in(cx, |m, window, _| {
        m.focus_handle(outline::Part::Row, Some("site"))
            .is_some_and(|handle| handle.is_focused(window))
    });
    assert!(still_focused, "focus stays on the row being moved");
    cx.simulate_keystrokes("left space");
    cx.run_until_parked();
    assert_eq!(
        *changes.borrow(),
        [vec!["0:inbox", "0:projects", "1:app", "0:site", "1:copy"]]
    );
}

#[gpui::test]
fn a_destructive_menu_entry_and_a_disabled_trigger_reach_the_adapter(cx: &mut TestAppContext) {
    let entries = vec![Entry::action("erase", "Erase all").destructive()];
    let (menu, cx) = open(
        cx,
        Menu::new(entries).disabled(),
        |menu: &Entity<Machine<Menu>>, cx: &mut App| {
            menu.update(cx, |m, cx| {
                m.track(menu::Part::Item, vec!["erase".to_string()], cx);
            });
            div()
                .child(part(menu, menu::Part::Trigger, None, cx).size(px(ROW)))
                .child(
                    part(menu, menu::Part::Content, None, cx)
                        .child(part(menu, menu::Part::Item, Some("erase"), cx).h(px(ROW))),
                )
        },
    );
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    assert!(menu.read_with(cx, |m, _| {
        m.semantics(menu::Part::Item, Some("erase")).destructive
    }));
    focus(&menu, cx, menu::Part::Trigger, None);
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    assert!(
        !menu.read_with(cx, |m, _| m.primitive().is_open()),
        "a disabled trigger never opens the menu"
    );
}

#[gpui::test]
fn a_token_editor_takes_typing_and_deletes_a_token_whole(cx: &mut TestAppContext) {
    use armature_core::token_editor::{self, TokenEditor};
    let (editor, cx) = open(
        cx,
        TokenEditor::default(),
        |editor: &Entity<Machine<TokenEditor>>, cx: &mut App| {
            part(editor, token_editor::Part::Root, None, cx).size(px(ROW))
        },
    );
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    focus(&editor, cx, token_editor::Part::Root, None);
    cx.simulate_keystrokes("h i space");
    cx.run_until_parked();
    assert_eq!(editor.read_with(cx, |m, _| m.primitive().value()), ["hi "]);
    // The owner sets the content with a token in it; the caret goes to the end.
    editor.update(cx, |m, cx| {
        m.set_value(vec!["hi [@a.ts](file:src/a.ts)".into()], cx);
    });
    cx.simulate_keystrokes("x left backspace");
    cx.run_until_parked();
    assert_eq!(editor.read_with(cx, |m, _| m.primitive().value()), ["hi x"]);
    assert_eq!(editor.read_with(cx, |m, _| m.primitive().caret()), 3);
}
