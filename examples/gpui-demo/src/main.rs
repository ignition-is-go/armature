use armature_core::accordion::{self, Accordion};
use armature_core::calendar::{self, Calendar};
use armature_core::canvas::{self, Canvas, Node};
use armature_core::checkbox::{self, CheckboxGroup};
use armature_core::color_picker::{self, ColorPicker};
use armature_core::command::{self, Command};
use armature_core::date::Date;
use armature_core::editable::{self, Editable};
use armature_core::geometry::Rect;
use armature_core::hold_button::{self, HoldButton};
use armature_core::keys::Dir;
use armature_core::listbox::{self, Listbox};
use armature_core::menu::{self, Entry, Kind as EntryKind, Menu};
use armature_core::number::NumberRange;
use armature_core::number_input::{self, NumberInput};
use armature_core::primitive::{Item, PartOf, Primitive};
use armature_core::prompt::{self, Prompt};
use armature_core::radio::{self, Radio};
use armature_core::range_slider::{self, RangeSlider};
use armature_core::resizable::{self, Resizable};
use armature_core::scroll_area::{self, ScrollArea};
use armature_core::select::{self, Select};
use armature_core::selection::Mode;
use armature_core::slider::{self, Slider};
use armature_core::sortable::{self, Sortable};
use armature_core::tabs::{self, Tabs};
use armature_core::text_field::{self, Kind as FieldKind, TextField};
use armature_core::toast::{self, Toast, Toaster};
use armature_core::toggle::{self, Toggle};
use armature_core::toolbar::{self, Toolbar};
use armature_core::transfer_list::{self, TransferList};
use armature_core::tree::tree_from_paths;
use armature_core::tree_view::{self, TreeView};
use armature_core::virtual_list::{self, VirtualList};
use armature_gpui::{Machine, PartElement, Source, live_region, part};
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
    colour: Entity<Machine<ColorPicker>>,
    rows: Entity<Machine<VirtualList>>,
    board: Entity<Machine<Canvas>>,
    title: Entity<Machine<Editable>>,
    message: Entity<Machine<Prompt>>,
    sent: Entity<Vec<String>>,
    delete: Entity<Machine<HoldButton>>,
    permissions: Entity<Machine<TransferList>>,
    actions: Entity<Machine<Menu>>,
    alerts: Entity<Machine<CheckboxGroup>>,
    email: Entity<Machine<TextField>>,
    price: Entity<Machine<RangeSlider>>,
    strip: Entity<Machine<ScrollArea>>,
    notices: Entity<Machine<Toaster>>,
}

impl Demo {
    fn new(cx: &mut Context<Self>) -> Self {
        let multiple = listbox::Options {
            mode: Mode::Multiple,
            ..Default::default()
        };
        let pressed = cx.new(|_| String::new());
        cx.observe(&pressed, |_, _, cx| cx.notify()).detach();
        let sent = cx.new(|_| Vec::<String>::new());
        cx.observe(&sent, |_, _, cx| cx.notify()).detach();
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
            colour: cx.new(|cx| Machine::new(ColorPicker::default(), "Colour", [], cx)),
            rows: cx.new(|cx| Machine::new(VirtualList::new(10_000, 24.0), "Rows", [], cx)),
            board: cx.new(|cx| {
                let nodes = [("a", 10.0, 10.0), ("b", 150.0, 10.0), ("c", 10.0, 100.0)]
                    .map(|(key, x, y)| Node {
                        key: key.into(),
                        rect: Rect::new(x, y, 90.0, 50.0),
                    })
                    .to_vec();
                Machine::new(Canvas::new(nodes), "Board", [], cx)
            }),
            title: cx.new(|cx| Machine::new(Editable::new("Untitled"), "Title", [], cx)),
            message: cx.new(|cx| {
                let sent = sent.clone();
                Machine::new(Prompt::default(), "Message", [], cx).on_value_change(
                    move |change, cx| {
                        sent.update(cx, |sent, cx| {
                            sent.extend(change.value.iter().cloned());
                            cx.notify();
                        });
                        true
                    },
                )
            }),
            delete: cx.new(|cx| {
                let sent = sent.clone();
                Machine::new(HoldButton::default(), "Delete", [], cx).on_value_change(
                    move |_, cx| {
                        sent.update(cx, |sent, cx| {
                            sent.clear();
                            cx.notify();
                        });
                        true
                    },
                )
            }),
            permissions: cx.new(|cx| {
                let keys = ["Read", "Write", "Share", "Delete"];
                Machine::new(TransferList::new(&keys), "Permissions", all(&keys), cx)
            }),
            actions: cx.new(|cx| {
                let entries = vec![
                    Entry::action("new", "New file"),
                    Entry::action("wrap", "Word wrap").of(EntryKind::Checkbox),
                    Entry::action("share", "Share").of(EntryKind::Submenu),
                    Entry::action("email", "Email").inside("share"),
                    Entry::action("link", "Copy link").inside("share"),
                ];
                Machine::new(Menu::new(entries), "Actions", [], cx)
            }),
            alerts: cx.new(|cx| {
                Machine::new(
                    CheckboxGroup::default(),
                    "Alerts",
                    all(&["Email", "SMS", "Push"]),
                    cx,
                )
            }),
            email: cx.new(|cx| {
                let field = TextField::new(FieldKind::Text).validate(|text| {
                    (!text.contains('@')).then(|| "Enter an email address.".to_string())
                });
                Machine::new(field, "Email", [], cx)
            }),
            price: cx.new(|cx| Machine::new(RangeSlider::default(), "Price", [], cx)),
            strip: cx.new(|cx| Machine::new(ScrollArea::horizontal(), "Days", [], cx)),
            notices: cx.new(|cx| {
                let mut toaster = Toaster::default();
                toaster.set_value(vec![
                    Toast::new("a", "Saved").action("Undo").lasting(0).encode(),
                ]);
                Machine::new(toaster, "Notifications", [], cx)
            }),
            sent,
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

        let tree_rows = self.files.read(cx).primitive().rows();
        let node = |path: &str| {
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
            .child(
                section("TREE").child(
                    part(&self.files, tree_view::Part::Root, None, cx)
                        .children(tree_rows.iter().map(|row| node(&row.key))),
                ),
            )
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

impl Demo {
    fn advanced(&self, cx: &App) -> Div {
        let picker = self.colour.read(cx).primitive();
        let hex = |color: armature_core::color::Rgb| {
            rgb(((color.r as u32) << 16) | ((color.g as u32) << 8) | color.b as u32)
        };
        let (across, down) = picker.area_position();
        let dot = |kind, left: f32, top: f32| {
            part(&self.colour, kind, None, cx)
                .absolute()
                .left(px(left - 7.))
                .top(px(top - 7.))
                .size(px(14.))
                .rounded_full()
                .bg(rgb(WHITE))
                .border_2()
                .border_color(rgb(INK))
                .focus(|style| style.border_color(rgb(ACCENT)))
        };
        let (pad_w, pad_h) = (200., 110.);
        let field = self
            .colour
            .read(cx)
            .text(color_picker::Part::Input, None)
            .unwrap_or_default();

        let visible = self.rows.read(cx).primitive().rows();
        let total = self.rows.read(cx).primitive().total_size() as f32;
        let row = |row: armature_core::virtual_list::VirtualRow| {
            let semantics = self
                .rows
                .read(cx)
                .semantics(virtual_list::Part::Row, Some(&row.key));
            let tick = if semantics.selected == Some(true) {
                "  ✓"
            } else {
                ""
            };
            part(&self.rows, virtual_list::Part::Row, Some(&row.key), cx)
                .absolute()
                .top(px(row.offset as f32))
                .h(px(24.))
                .w_full()
                .px_2()
                .when(semantics.active, |el| el.bg(rgb(WASH)))
                .child(format!("Row {}{tick}", row.index))
        };

        let board = self.board.read(cx).primitive();
        let node = |key: &'static str| {
            let place = board.screen_rect(key).unwrap_or_default();
            let chosen = board.selected().iter().any(|selected| selected == key);
            part(&self.board, canvas::Part::Node, Some(key), cx)
                .absolute()
                .left(px(place.x as f32))
                .top(px(place.y as f32))
                .w(px(place.width as f32))
                .h(px(place.height as f32))
                .rounded_md()
                .border_1()
                .border_color(rgb(if chosen { ACCENT } else { LINE }))
                .bg(rgb(if chosen { WASH } else { WHITE }))
                .p_1()
                .child(key)
        };
        let links: Vec<String> = board
            .connections()
            .iter()
            .map(|link| format!("{} > {}", link.from, link.to))
            .collect();

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                section("COLOUR")
                    .child(format!("Colour: {}", self.colour.read(cx).value().join("")))
                    .child(
                        part(&self.colour, color_picker::Part::Area, None, cx)
                            .relative()
                            .w(px(pad_w))
                            .h(px(pad_h))
                            .rounded_md()
                            .bg(hex(picker.hue_color()))
                            .child(dot(
                                color_picker::Part::AreaThumb,
                                pad_w * across as f32,
                                pad_h * down as f32,
                            )),
                    )
                    .child(
                        part(&self.colour, color_picker::Part::Hue, None, cx)
                            .relative()
                            .w(px(pad_w))
                            .h(px(10.))
                            .rounded_md()
                            .bg(rgb(LINE))
                            .child(dot(
                                color_picker::Part::HueThumb,
                                pad_w * picker.hue_position() as f32,
                                5.,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child(div().size(px(26.)).rounded_md().bg(hex(picker.color())))
                            .child(
                                boxed(part(&self.colour, color_picker::Part::Input, None, cx))
                                    .w(px(90.))
                                    .px_2()
                                    .child(field),
                            ),
                    ),
            )
            .child(
                section("VIRTUAL LIST")
                    .child(format!("Row: {}", self.rows.read(cx).value().join("")))
                    .child(
                        boxed(part(&self.rows, virtual_list::Part::Viewport, None, cx))
                            .w(px(pad_w))
                            .h(px(120.))
                            .child(
                                div()
                                    .relative()
                                    .h(px(total))
                                    .children(visible.into_iter().map(row)),
                            ),
                    ),
            )
            .child(
                section("CANVAS")
                    .child(format!("Links: {}", links.join(", ")))
                    .child(
                        boxed(part(&self.board, canvas::Part::Surface, None, cx))
                            .relative()
                            .w(px(260.))
                            .h(px(170.))
                            .overflow_hidden()
                            .children(["a", "b", "c"].map(node)),
                    ),
            )
    }
}

impl Demo {
    fn editing(&self, cx: &App) -> Div {
        let title = self.title.read(cx);
        let draft = title.text(editable::Part::Input, None).unwrap_or_default();
        let saved = title.value().join("");
        let typed = self
            .message
            .read(cx)
            .text(prompt::Part::Input, None)
            .unwrap_or_default();
        let holding = self
            .delete
            .read(cx)
            .semantics(hold_button::Part::Root, None)
            .active;
        let lists = self.permissions.read(cx).primitive();
        let column = |kind, chosen: bool| {
            boxed(part(&self.permissions, kind, None, cx))
                .w(px(96.))
                .min_h(px(110.))
                .p_1()
                .children(
                    lists
                        .list(chosen)
                        .iter()
                        .map(|key| row(&self.permissions, transfer_list::Part::Option, key, cx)),
                )
        };
        let mover = |kind, sign: &'static str| {
            let off = self.permissions.read(cx).semantics(kind, None).disabled;
            boxed(part(&self.permissions, kind, None, cx))
                .px_2()
                .when(off, |el| el.text_color(rgb(MUTED)))
                .child(sign)
        };
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                section("EDITABLE")
                    .child(format!("Title: {saved}"))
                    .child(
                        boxed(part(&self.title, editable::Part::Preview, None, cx))
                            .w(px(200.))
                            .px_2()
                            .child(saved.clone()),
                    )
                    .child(
                        boxed(part(&self.title, editable::Part::Input, None, cx))
                            .w(px(200.))
                            .px_2()
                            .child(draft),
                    ),
            )
            .child(
                section("PROMPT")
                    .child(format!("Sent: {}", self.sent.read(cx).join(" | ")))
                    .child(
                        boxed(part(&self.message, prompt::Part::Input, None, cx))
                            .w(px(200.))
                            .h(px(26.))
                            .px_2()
                            .child(typed),
                    ),
            )
            .child(
                section("HOLD TO CONFIRM").child(
                    boxed(part(&self.delete, hold_button::Part::Root, None, cx))
                        .px_2()
                        .py_1()
                        .when(holding, |el| el.bg(rgb(WASH)))
                        .child("Hold to clear sent"),
                ),
            )
            .child(
                section("TRANSFER LIST")
                    .child(format!(
                        "Chosen: {}",
                        self.permissions.read(cx).value().join(", ")
                    ))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child(column(transfer_list::Part::Available, false))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(mover(transfer_list::Part::Add, "→"))
                                    .child(mover(transfer_list::Part::Remove, "←")),
                            )
                            .child(column(transfer_list::Part::Chosen, true)),
                    ),
            )
    }
}

impl Demo {
    fn forms(&self, cx: &App) -> Div {
        let menu = self.actions.read(cx).primitive();
        let entries: Vec<(String, String)> = menu
            .shown()
            .iter()
            .map(|entry| (entry.key.clone(), entry.label.clone()))
            .collect();
        let entry = |(key, label): &(String, String)| {
            let semantics = self.actions.read(cx).semantics(menu::Part::Item, Some(key));
            let tick = if semantics.checked == Some(true) {
                "  ✓"
            } else {
                ""
            };
            part(&self.actions, menu::Part::Item, Some(key), cx)
                .px_2()
                .py_1()
                .rounded_sm()
                .when(semantics.active, |el| el.bg(rgb(WASH)))
                .child(format!("{label}{tick}"))
        };
        let check = |kind, key: Option<&'static str>, label: &'static str| {
            let semantics = self.alerts.read(cx).semantics(kind, key);
            let mark = match (semantics.mixed, semantics.checked) {
                (true, _) => "[-] ",
                (_, Some(true)) => "[x] ",
                _ => "[ ] ",
            };
            part(&self.alerts, kind, key, cx)
                .px_1()
                .border_1()
                .border_color(rgb(WHITE))
                .focus(|style| style.border_color(rgb(ACCENT)))
                .child(format!("{mark}{label}"))
        };
        let field = self.email.read(cx);
        let typed = field
            .text(text_field::Part::Input, None)
            .unwrap_or_default();
        let problem = field.primitive().error().unwrap_or_default();
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                section("MENU")
                    .child(format!(
                        "Menu: {}",
                        self.actions.read(cx).value().join(", ")
                    ))
                    .child(
                        boxed(part(&self.actions, menu::Part::Trigger, None, cx))
                            .px_2()
                            .child("Actions"),
                    )
                    .child(
                        boxed(part(&self.actions, menu::Part::Area, None, cx))
                            .px_2()
                            .py_1()
                            .text_color(rgb(MUTED))
                            .child("or right-click here"),
                    )
                    .child(
                        boxed(part(&self.actions, menu::Part::Content, None, cx))
                            .bg(rgb(WHITE))
                            .w(px(150.))
                            .p_1()
                            .children(entries.iter().map(entry)),
                    ),
            )
            .child(
                section("CHECKBOX GROUP")
                    .child(format!(
                        "Alerts: {}",
                        self.alerts.read(cx).value().join(", ")
                    ))
                    .child(
                        part(&self.alerts, checkbox::GroupPart::Root, None, cx)
                            .child(check(checkbox::GroupPart::All, None, "All"))
                            .child(check(checkbox::GroupPart::Option, Some("Email"), "Email"))
                            .child(check(checkbox::GroupPart::Option, Some("SMS"), "SMS"))
                            .child(check(checkbox::GroupPart::Option, Some("Push"), "Push")),
                    ),
            )
            .child(
                section("TEXT FIELD")
                    .child(format!("Email: {}", field.value().join("")))
                    .child(
                        boxed(part(&self.email, text_field::Part::Input, None, cx))
                            .w(px(200.))
                            .h(px(26.))
                            .px_2()
                            .child(typed),
                    )
                    .child(
                        part(&self.email, text_field::Part::Error, None, cx)
                            .text_color(rgb(0xb3261e))
                            .child(problem),
                    ),
            )
    }
}

impl Demo {
    fn extras(&self, cx: &App) -> Div {
        let price = self.price.read(cx);
        let spots = price.primitive().fractions();
        let thumb = |at: usize| {
            part(
                &self.price,
                range_slider::Part::Thumb,
                Some(&at.to_string()),
                cx,
            )
            .absolute()
            .left(px(TRACK * spots[at] as f32 - 8.))
            .top(px(-5.))
            .size(px(16.))
            .rounded_full()
            .bg(rgb(WHITE))
            .border_2()
            .border_color(rgb(ACCENT))
        };
        let area = self.strip.read(cx).primitive();
        let (start, length) = area.thumb(200.0);
        let shown: Vec<Toast> = self.notices.read(cx).primitive().visible().to_vec();
        let notice = |toast: &Toast| {
            div()
                .flex()
                .gap_2()
                .child(
                    part(&self.notices, toast::Part::Toast, Some(&toast.id), cx)
                        .child(toast.title.clone()),
                )
                .child(
                    boxed(part(
                        &self.notices,
                        toast::Part::Action,
                        Some(&toast.id),
                        cx,
                    ))
                    .px_1()
                    .child(toast.action.clone()),
                )
                .child(
                    boxed(part(&self.notices, toast::Part::Close, Some(&toast.id), cx))
                        .px_1()
                        .child("×"),
                )
        };
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                section("RANGE SLIDER")
                    .child(format!("Price: {}", price.value().join(" to ")))
                    .child(
                        part(&self.price, range_slider::Part::Track, None, cx)
                            .relative()
                            .w(px(TRACK))
                            .h(px(6.))
                            .my_2()
                            .rounded_md()
                            .bg(rgb(LINE))
                            .child(thumb(0))
                            .child(thumb(1)),
                    ),
            )
            .child(
                section("SIDEWAYS SCROLL").child(
                    div()
                        .w(px(200.))
                        .child(
                            part(&self.strip, scroll_area::Part::Viewport, None, cx)
                                .w(px(200.))
                                .child(
                                    div().flex().flex_none().w(px(1300.)).gap_3().children(
                                        (1..=20).map(|day| {
                                            div().flex_none().child(format!("Day {day}"))
                                        }),
                                    ),
                                ),
                        )
                        .child(
                            part(&self.strip, scroll_area::Part::Scrollbar, None, cx)
                                .relative()
                                .w(px(200.))
                                .h(px(8.))
                                .bg(rgb(WASH))
                                .child(
                                    part(&self.strip, scroll_area::Part::Thumb, None, cx)
                                        .absolute()
                                        .left(px(start as f32))
                                        .w(px(length as f32))
                                        .h(px(8.))
                                        .bg(rgb(MUTED)),
                                ),
                        ),
                ),
            )
            .child(
                section("TOASTS (F6)")
                    .child(format!("Showing: {}", shown.len()))
                    .child(
                        part(&self.notices, toast::Part::Region, None, cx)
                            .children(shown.iter().map(notice)),
                    ),
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
        let shown: Vec<String> = self
            .rows
            .read(cx)
            .primitive()
            .rows()
            .into_iter()
            .map(|row| row.key)
            .collect();
        self.rows.update(cx, |rows, cx| {
            rows.track(virtual_list::Part::Row, shown, cx)
        });
        let entries: Vec<String> = self
            .actions
            .read(cx)
            .primitive()
            .shown()
            .iter()
            .map(|entry| entry.key.clone())
            .collect();
        self.actions.update(cx, |actions, cx| {
            actions.track(menu::Part::Item, entries, cx)
        });
        let ids: Vec<String> = self
            .notices
            .read(cx)
            .primitive()
            .visible()
            .iter()
            .map(|toast| toast.id.clone())
            .collect();
        self.notices.update(cx, |notices, cx| {
            for kind in [toast::Part::Toast, toast::Part::Action, toast::Part::Close] {
                notices.track(kind, ids.clone(), cx);
            }
        });
        self.price.update(cx, |price, cx| {
            price.track(
                range_slider::Part::Thumb,
                ["0".to_string(), "1".to_string()],
                cx,
            )
        });
        let nodes = ["a", "b", "c"].map(String::from);
        self.board
            .update(cx, |board, cx| board.track(canvas::Part::Node, nodes, cx));
        div()
            .size_full()
            .bg(rgb(WHITE))
            .text_color(rgb(INK))
            .p_6()
            .flex()
            .flex_wrap()
            .items_start()
            .gap_8()
            .on_action(|_: &FocusNext, window, cx| window.focus_next(cx))
            .on_action(|_: &FocusPrev, window, cx| window.focus_prev(cx))
            .child(self.classic(cx))
            .child(self.newer(cx))
            .child(self.latest(cx))
            .child(self.advanced(cx))
            .child(self.editing(cx))
            .child(live_region(&self.groceries, cx))
            .child(live_region(&self.permissions, cx))
            .child(self.forms(cx))
            .child(self.extras(cx))
            .child(live_region(&self.notices, cx))
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("tab", FocusNext, None),
            KeyBinding::new("shift-tab", FocusPrev, None),
        ]);
        let bounds = Bounds::centered(None, size(px(1480.), px(900.)), cx);
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
