use armature_core::accordion::{self, Accordion};
use armature_core::calendar::{self, Calendar};
use armature_core::command::{self, Command};
use armature_core::date::Date;
use armature_core::keys::Dir;
use armature_core::listbox::{self, Listbox};
use armature_core::number::NumberRange;
use armature_core::number_input::{self, NumberInput};
use armature_core::primitive::{Item, PartOf, Primitive};
use armature_core::radio::{self, Radio};
use armature_core::resizable::{self, Resizable};
use armature_core::select::{self, Select};
use armature_core::selection::Mode;
use armature_core::slider::{self, Slider};
use armature_core::sortable::{self, Sortable};
use armature_core::tabs::{self, Tabs};
use armature_core::toggle::{self, Toggle};
use armature_core::toolbar::{self, Toolbar};
use armature_core::tree::tree_from_paths;
use armature_core::tree_view::{self, TreeView};
use armature_gpui::{Machine, PartElement, Source, part};
use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, Div, Entity, KeyBinding, Window, WindowBounds, WindowOptions, actions,
    div, px, rgb, size,
};

actions!(demo, [FocusNext, FocusPrev]);

const INK: u32 = 0x1f2430;
const MUTED: u32 = 0x8a909c;
const LINE: u32 = 0xc9ced8;
const WASH: u32 = 0xe8eefc;
const ACCENT: u32 = 0x3b6fe0;
const WHITE: u32 = 0xffffff;
const FILES: [&str; 7] = [
    "src",
    "src/main.rs",
    "src/ui",
    "src/ui/button.rs",
    "docs",
    "docs/intro.md",
    "README.md",
];
const TRACK: f32 = 220.;

fn items(entries: &[(&str, bool)]) -> Vec<Item> {
    entries
        .iter()
        .map(|(key, disabled)| Item {
            key: key.to_string(),
            label: key.to_string(),
            disabled: *disabled,
        })
        .collect()
}

struct Demo {
    size: Entity<Machine<Select>>,
    tabs: Entity<Machine<Tabs>>,
    fruit: Entity<Machine<Listbox>>,
    access: Entity<Machine<Listbox>>,
    volume: Entity<Machine<Slider>>,
    wifi: Entity<Machine<Toggle>>,
    faq: Entity<Machine<Accordion>>,
    rating: Entity<Machine<Radio>>,
    files: Entity<Machine<TreeView>>,
    commands: Entity<Machine<Command>>,
    quantity: Entity<Machine<NumberInput>>,
    groceries: Entity<Machine<Sortable>>,
    date: Entity<Machine<Calendar>>,
    formatting: Entity<Machine<Toolbar>>,
    panels: Entity<Machine<Resizable>>,
    pressed: Entity<String>,
}

impl Demo {
    fn new(cx: &mut Context<Self>) -> Self {
        let multiple = listbox::Options {
            mode: Mode::Multiple,
            ..Default::default()
        };
        let pressed = cx.new(|_| String::new());
        cx.observe(&pressed, |_, _, cx| cx.notify()).detach();
        let all = |keys: &[&str]| items(&keys.iter().map(|key| (*key, false)).collect::<Vec<_>>());
        let tree = tree_from_paths(&[
            "src/main.rs",
            "src/ui/button.rs",
            "docs/intro.md",
            "README.md",
        ]);
        let file_labels: Vec<Item> = FILES
            .iter()
            .map(|path| Item {
                key: path.to_string(),
                label: path.rsplit('/').next().unwrap_or(path).to_string(),
                disabled: false,
            })
            .collect();
        Demo {
            size: cx.new(|cx| {
                let sizes = items(&[("Small", false), ("Medium", true), ("Large", false)]);
                Machine::new(Select::default(), "Size", sizes, cx)
            }),
            tabs: cx.new(|cx| {
                let sections = items(&[("Fruit", false), ("Billing", true), ("Access", false)]);
                Machine::new(Tabs::default(), "Settings", sections, cx)
            }),
            fruit: cx.new(|cx| {
                let fruit = items(&[
                    ("Apple", false),
                    ("Banana", true),
                    ("Blueberry", false),
                    ("Cherry", false),
                ]);
                Machine::new(Listbox::default(), "Fruit", fruit, cx)
            }),
            access: cx.new(|cx| {
                let access = all(&["Read", "Write", "Share", "Delete"]);
                Machine::new(Listbox::new(multiple), "Access", access, cx).on_value_change(
                    |change, _| {
                        !(change.source == Source::Agent
                            && change.value.iter().any(|v| v == "Delete"))
                    },
                )
            }),
            volume: cx
                .new(|cx| Machine::new(Slider::new(Default::default(), 30.0), "Volume", [], cx)),
            wifi: cx.new(|cx| Machine::new(Toggle::default(), "Wi-Fi", [], cx)),
            faq: cx.new(|cx| {
                Machine::new(
                    Accordion::default(),
                    "Questions",
                    all(&["Shipping", "Returns"]),
                    cx,
                )
            }),
            rating: cx.new(|cx| {
                Machine::new(
                    Radio::new(Dir::Ltr),
                    "Rating",
                    all(&["1", "2", "3", "4"]),
                    cx,
                )
            }),
            files: cx
                .new(|cx| Machine::new(TreeView::new(tree, Dir::Ltr), "Files", file_labels, cx)),
            commands: cx.new(|cx| {
                let commands = all(&["Open file", "Save file", "Close window"]);
                Machine::new(Command::default(), "Commands", commands, cx)
            }),
            quantity: cx.new(|cx| {
                let range = NumberRange::new(0.0, 20.0, 1.0);
                Machine::new(NumberInput::new(range, 2.0), "Quantity", [], cx)
            }),
            date: cx.new(|cx| {
                let start = Date::parse("2026-11-17").expect("a real date");
                Machine::new(Calendar::new(Default::default(), start), "Date", [], cx)
            }),
            formatting: cx.new(|cx| {
                let buttons = items(&[("Bold", false), ("Italic", true), ("Link", false)]);
                let pressed = pressed.clone();
                Machine::new(Toolbar::default(), "Formatting", buttons, cx).on_value_change(
                    move |change, cx| {
                        pressed.update(cx, |last, cx| {
                            *last = change.value.join("");
                            cx.notify();
                        });
                        true
                    },
                )
            }),
            panels: cx.new(|cx| Machine::new(Resizable::default(), "Panels", [], cx)),
            pressed,
            groceries: cx.new(|cx| {
                Machine::new(
                    Sortable::default(),
                    "Groceries",
                    all(&["Milk", "Eggs", "Bread", "Jam"]),
                    cx,
                )
            }),
        }
    }
}

fn boxed(el: PartElement) -> PartElement {
    el.border_1()
        .border_color(rgb(LINE))
        .rounded_md()
        .focus(|style| style.border_color(rgb(ACCENT)))
}

/// An item part styled from its own semantics: washed when active, ticked when
/// selected, greyed when disabled.
fn row<K: PartOf>(
    state: &Entity<Machine<K::Primitive>>,
    kind: K,
    key: &str,
    cx: &App,
) -> PartElement {
    let semantics = state.read(cx).semantics(kind, Some(key));
    let tick = if semantics.selected == Some(true) {
        "  ✓"
    } else {
        ""
    };
    part(state, kind, Some(key), cx)
        .px_2()
        .py_1()
        .rounded_sm()
        .border_1()
        .border_color(rgb(WHITE))
        .focus(|style| style.border_color(rgb(ACCENT)))
        .when(semantics.active, |el| el.bg(rgb(WASH)))
        .when(semantics.disabled, |el| el.text_color(rgb(MUTED)))
        .child(format!("{key}{tick}"))
}

fn rows<K: PartOf>(state: &Entity<Machine<K::Primitive>>, kind: K, cx: &App) -> Vec<PartElement>
where
    K::Primitive: Primitive,
{
    let keys: Vec<String> = state
        .read(cx)
        .snapshot()
        .items
        .into_iter()
        .map(|item| item.key)
        .collect();
    keys.iter().map(|key| row(state, kind, key, cx)).collect()
}

fn list(state: &Entity<Machine<Listbox>>, cx: &App) -> PartElement {
    boxed(part(state, listbox::Part::Root, None, cx))
        .w(px(220.))
        .p_1()
        .children(rows(state, listbox::Part::Option, cx))
}

fn section(title: &'static str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_xs().text_color(rgb(MUTED)).child(title))
}

impl Demo {
    fn classic(&self, cx: &App) -> Div {
        let value = |values: Vec<String>| values.join(", ");
        let chosen_size = self.size.read(cx).value().first().cloned();
        let trigger = boxed(part(&self.size, select::Part::Trigger, None, cx))
            .w(px(220.))
            .px_2()
            .py_1()
            .child(chosen_size.unwrap_or("Choose a size".into()));
        let options = boxed(part(&self.size, select::Part::Content, None, cx))
            .w(px(220.))
            .p_1()
            .bg(rgb(WHITE))
            .shadow_md()
            .children(rows(&self.size, select::Part::Option, cx));
        let tab = |key: &str| {
            let semantics = self.tabs.read(cx).semantics(tabs::Part::Tab, Some(key));
            part(&self.tabs, tabs::Part::Tab, Some(key), cx)
                .px_3()
                .py_1()
                .border_b_2()
                .border_color(rgb(if semantics.selected == Some(true) {
                    ACCENT
                } else {
                    WHITE
                }))
                .focus(|style| style.bg(rgb(WASH)))
                .when(semantics.disabled, |el| el.text_color(rgb(MUTED)))
                .child(key.to_string())
        };
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(section("SELECT").child(trigger).child(options))
            .child(
                section("TABS AND LISTBOX")
                    .child(
                        part(&self.tabs, tabs::Part::List, None, cx)
                            .flex()
                            .gap_1()
                            .border_b_1()
                            .border_color(rgb(LINE))
                            .children(["Fruit", "Billing", "Access"].map(tab)),
                    )
                    .child(
                        part(&self.tabs, tabs::Part::Panel, Some("Fruit"), cx)
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(format!("Fruit: {}", value(self.fruit.read(cx).value())))
                            .child(list(&self.fruit, cx)),
                    )
                    .child(
                        part(&self.tabs, tabs::Part::Panel, Some("Access"), cx)
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(format!("Access: {}", value(self.access.read(cx).value())))
                            .child("Agents may not grant Delete.")
                            .child(list(&self.access, cx)),
                    ),
            )
            .child(
                section("SORTABLE").child(
                    part(&self.groceries, sortable::Part::Root, None, cx)
                        .flex()
                        .flex_col()
                        .gap_1()
                        .children(self.groceries.read(cx).value().iter().map(|key| {
                            let grabbed = self
                                .groceries
                                .read(cx)
                                .semantics(sortable::Part::Item, Some(key));
                            boxed(part(&self.groceries, sortable::Part::Item, Some(key), cx))
                                .w(px(220.))
                                .px_2()
                                .py_1()
                                .when(grabbed.selected == Some(true), |el| el.bg(rgb(WASH)))
                                .child(key.clone())
                        })),
                ),
            )
    }

    fn newer(&self, cx: &App) -> Div {
        let volume = self.volume.read(cx).primitive();
        let fraction = volume.fraction() as f32;
        let slider = part(&self.volume, slider::Part::Track, None, cx)
            .relative()
            .w(px(TRACK))
            .h(px(6.))
            .my_2()
            .rounded_sm()
            .bg(rgb(LINE))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .h_full()
                    .w(px(TRACK * fraction))
                    .rounded_sm()
                    .bg(rgb(ACCENT)),
            )
            .child(
                part(&self.volume, slider::Part::Thumb, None, cx)
                    .absolute()
                    .top(px(-5.))
                    .left(px(TRACK * fraction - 8.))
                    .size(px(16.))
                    .rounded_full()
                    .bg(rgb(WHITE))
                    .border_2()
                    .border_color(rgb(ACCENT))
                    .focus(|style| style.bg(rgb(WASH))),
            );

        let on = self.wifi.read(cx).primitive().is_on();
        let switch = part(&self.wifi, toggle::Part::Root, None, cx)
            .w(px(40.))
            .h(px(22.))
            .rounded_full()
            .bg(rgb(if on { ACCENT } else { LINE }))
            .border_2()
            .border_color(rgb(WHITE))
            .focus(|style| style.border_color(rgb(INK)))
            .child(
                div()
                    .size(px(18.))
                    .rounded_full()
                    .bg(rgb(WHITE))
                    .when(on, |el| el.ml(px(18.))),
            );

        let question = |key: &'static str, answer: &'static str| {
            div()
                .child(
                    part(&self.faq, accordion::Part::Trigger, Some(key), cx)
                        .w(px(TRACK))
                        .px_2()
                        .py_1()
                        .border_b_1()
                        .border_color(rgb(LINE))
                        .focus(|style| style.bg(rgb(WASH)))
                        .child(key),
                )
                .child(
                    part(&self.faq, accordion::Part::Panel, Some(key), cx)
                        .px_2()
                        .py_1()
                        .text_color(rgb(MUTED))
                        .child(answer),
                )
        };

        let star = |key: &'static str| {
            let filled = self.rating.read(cx).primitive().is_filled(key);
            part(&self.rating, radio::Part::Option, Some(key), cx)
                .px_1()
                .text_xl()
                .text_color(rgb(if filled { 0xe0a23b } else { LINE }))
                .border_1()
                .border_color(rgb(WHITE))
                .focus(|style| style.border_color(rgb(ACCENT)))
                .child("★")
        };

        let node = |path: &'static str| {
            let semantics = self
                .files
                .read(cx)
                .semantics(tree_view::Part::Node, Some(path));
            let marker = match semantics.expanded {
                Some(true) => "▾ ",
                Some(false) => "▸ ",
                None => "",
            };
            let name = path.rsplit('/').next().unwrap_or(path);
            let tick = if semantics.selected == Some(true) {
                "  ✓"
            } else {
                ""
            };
            part(&self.files, tree_view::Part::Node, Some(path), cx)
                .ml(px(18. * (semantics.level.unwrap_or(1) - 1) as f32))
                .px_1()
                .border_1()
                .border_color(rgb(WHITE))
                .focus(|style| style.border_color(rgb(ACCENT)))
                .child(format!("{marker}{name}{tick}"))
        };

        let query = self
            .commands
            .read(cx)
            .text(command::Part::Input, None)
            .unwrap_or_default();
        let quantity = self
            .quantity
            .read(cx)
            .text(number_input::Part::Input, None)
            .unwrap_or_default();
        let stepper = |kind, sign: &'static str| {
            let disabled = self.quantity.read(cx).semantics(kind, None).disabled;
            boxed(part(&self.quantity, kind, None, cx))
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .when(disabled, |el| el.text_color(rgb(LINE)))
                .child(sign)
        };

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                section("SLIDER")
                    .child(format!("Volume: {}%", volume.number()))
                    .child(slider),
            )
            .child(
                section("SWITCH").child(div().flex().gap_2().child(switch).child(if on {
                    "On"
                } else {
                    "Off"
                })),
            )
            .child(
                section("ACCORDION")
                    .child(question("Shipping", "Ships in two days."))
                    .child(question("Returns", "Thirty days.")),
            )
            .child(
                section("RATING").child(
                    part(&self.rating, radio::Part::Root, None, cx)
                        .flex()
                        .children(["1", "2", "3", "4"].map(star)),
                ),
            )
            .child(section("TREE").child(
                part(&self.files, tree_view::Part::Root, None, cx).children(FILES.map(node)),
            ))
            .child(
                section("COMMAND")
                    .child(
                        boxed(part(&self.commands, command::Part::Input, None, cx))
                            .w(px(TRACK))
                            .h(px(28.))
                            .px_2()
                            .py_1()
                            .child(if query.is_empty() {
                                "Type to filter".to_string()
                            } else {
                                query
                            }),
                    )
                    .child(
                        part(&self.commands, command::Part::List, None, cx).children(rows(
                            &self.commands,
                            command::Part::Option,
                            cx,
                        )),
                    ),
            )
            .child(
                section("NUMBER").child(
                    div()
                        .flex()
                        .gap_1()
                        .items_center()
                        .child(stepper(number_input::Part::Decrement, "−"))
                        .child(
                            boxed(part(&self.quantity, number_input::Part::Input, None, cx))
                                .w(px(60.))
                                .h(px(28.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(quantity),
                        )
                        .child(stepper(number_input::Part::Increment, "+")),
                ),
            )
    }
}

impl Demo {
    fn latest(&self, cx: &App) -> Div {
        let calendar = self.date.read(cx).primitive();
        let (year, month) = calendar.month();
        let month_button =
            |kind, sign: &'static str| boxed(part(&self.date, kind, None, cx)).px_2().child(sign);
        let day = |date: Date| {
            let key = date.format();
            let semantics = self
                .date
                .read(cx)
                .semantics(calendar::Part::Day, Some(&key));
            let chosen = semantics.selected == Some(true);
            part(&self.date, calendar::Part::Day, Some(&key), cx)
                .size(px(30.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .border_1()
                .border_color(rgb(WHITE))
                .focus(|style| style.border_color(rgb(ACCENT)))
                .when(!semantics.active, |el| el.text_color(rgb(LINE)))
                .when(chosen, |el| el.bg(rgb(ACCENT)).text_color(rgb(WHITE)))
                .child(date.day.to_string())
        };
        let weeks = calendar.weeks();
        let chosen_day = self.date.read(cx).value().join("");

        let button = |key: &'static str| {
            let disabled = self
                .formatting
                .read(cx)
                .semantics(toolbar::Part::Button, Some(key))
                .disabled;
            boxed(part(&self.formatting, toolbar::Part::Button, Some(key), cx))
                .px_2()
                .py_1()
                .when(disabled, |el| el.text_color(rgb(LINE)))
                .child(key)
        };

        let share = self.panels.read(cx).primitive().percent() as f32;
        let dragging = self
            .panels
            .read(cx)
            .semantics(resizable::Part::Handle, None)
            .active;
        let width = 220.;
        let panels = part(&self.panels, resizable::Part::Root, None, cx)
            .flex()
            .w(px(width))
            .h(px(70.))
            .border_1()
            .border_color(rgb(LINE))
            .rounded_md()
            .child(div().w(px(width * share / 100. - 4.)).p_1().child("Left"))
            .child(
                part(&self.panels, resizable::Part::Handle, None, cx)
                    .w(px(6.))
                    .h_full()
                    .bg(rgb(if dragging { ACCENT } else { LINE }))
                    .focus(|style| style.bg(rgb(ACCENT))),
            )
            .child(div().p_1().child("Right"));

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                section("CALENDAR")
                    .child(format!("Day: {chosen_day}"))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child(month_button(calendar::Part::PreviousMonth, "‹"))
                            .child(format!("{year}-{month:02}"))
                            .child(month_button(calendar::Part::NextMonth, "›")),
                    )
                    .child(
                        part(&self.date, calendar::Part::Grid, None, cx)
                            .flex()
                            .flex_col()
                            .children(
                                weeks
                                    .into_iter()
                                    .map(|week| div().flex().children(week.map(day))),
                            ),
                    ),
            )
            .child(
                section("TOOLBAR")
                    .child(format!("Pressed: {}", self.pressed.read(cx)))
                    .child(
                        part(&self.formatting, toolbar::Part::Root, None, cx)
                            .flex()
                            .gap_1()
                            .children(["Bold", "Italic", "Link"].map(button)),
                    ),
            )
            .child(
                section("RESIZABLE")
                    .child(format!("Split: {share}%"))
                    .child(panels),
            )
    }
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let days: Vec<String> = self
            .date
            .read(cx)
            .primitive()
            .weeks()
            .iter()
            .flatten()
            .map(|day| day.format())
            .collect();
        self.date
            .update(cx, |date, cx| date.track(calendar::Part::Day, days, cx));
        div()
            .size_full()
            .bg(rgb(WHITE))
            .text_color(rgb(INK))
            .p_6()
            .flex()
            .gap_8()
            .on_action(|_: &FocusNext, window, cx| window.focus_next(cx))
            .on_action(|_: &FocusPrev, window, cx| window.focus_prev(cx))
            .child(self.classic(cx))
            .child(self.newer(cx))
            .child(self.latest(cx))
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("tab", FocusNext, None),
            KeyBinding::new("shift-tab", FocusPrev, None),
        ]);
        let bounds = Bounds::centered(None, size(px(900.), px(900.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("armature on GPUI".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(Demo::new),
        )
        .unwrap();
        cx.activate(true);

        let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
        let socket = format!("{runtime_dir}/armature-gpui-demo.sock");
        match armature_gpui::agent::serve(&socket, cx) {
            Ok(()) => println!("agent bridge listening on {socket}"),
            Err(error) => eprintln!("agent bridge unavailable: {error}"),
        }
    });
}
