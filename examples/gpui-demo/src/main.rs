use armature_core::listbox::{self, Listbox};
use armature_core::primitive::{Item, PartOf, Primitive};
use armature_core::select::{self, Select};
use armature_core::selection::Mode;
use armature_core::tabs::{self, Tabs};
use armature_gpui::{Machine, PartElement, Source, part};
use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, Entity, KeyBinding, Window, WindowBounds, WindowOptions, actions, div,
    px, rgb, size,
};

actions!(demo, [FocusNext, FocusPrev]);

const INK: u32 = 0x1f2430;
const MUTED: u32 = 0x8a909c;
const LINE: u32 = 0xc9ced8;
const WASH: u32 = 0xe8eefc;
const ACCENT: u32 = 0x3b6fe0;

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
}

impl Demo {
    fn new(cx: &mut Context<Self>) -> Self {
        let sizes = items(&[("Small", false), ("Medium", true), ("Large", false)]);
        let sections = items(&[("Fruit", false), ("Billing", true), ("Access", false)]);
        let fruit = items(&[
            ("Apple", false),
            ("Banana", true),
            ("Blueberry", false),
            ("Cherry", false),
        ]);
        let access = items(&[
            ("Read", false),
            ("Write", false),
            ("Share", false),
            ("Delete", false),
        ]);
        let multiple = listbox::Options {
            mode: Mode::Multiple,
            ..Default::default()
        };
        Demo {
            size: cx.new(|cx| Machine::new(Select::default(), "Size", sizes, cx)),
            tabs: cx.new(|cx| Machine::new(Tabs::default(), "Settings", sections, cx)),
            fruit: cx.new(|cx| Machine::new(Listbox::default(), "Fruit", fruit, cx)),
            access: cx.new(|cx| {
                Machine::new(Listbox::new(multiple), "Access", access, cx).on_value_change(
                    |change, _| {
                        !(change.source == Source::Agent
                            && change.value.iter().any(|v| v == "Delete"))
                    },
                )
            }),
        }
    }
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
        .border_color(rgb(0xffffff))
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
    part(state, listbox::Part::Root, None, cx)
        .w(px(220.))
        .p_1()
        .border_1()
        .border_color(rgb(LINE))
        .rounded_md()
        .children(rows(state, listbox::Part::Option, cx))
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let value = |values: Vec<String>| values.join(", ");
        let chosen_size = self.size.read(cx).value().first().cloned();

        let trigger = part(&self.size, select::Part::Trigger, None, cx)
            .w(px(220.))
            .px_2()
            .py_1()
            .border_1()
            .border_color(rgb(LINE))
            .rounded_md()
            .focus(|style| style.border_color(rgb(ACCENT)))
            .child(chosen_size.unwrap_or("Choose a size".into()));
        let options = part(&self.size, select::Part::Content, None, cx)
            .w(px(220.))
            .p_1()
            .bg(rgb(0xffffff))
            .border_1()
            .border_color(rgb(LINE))
            .rounded_md()
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
                    0xffffff
                }))
                .focus(|style| style.bg(rgb(WASH)))
                .when(semantics.disabled, |el| el.text_color(rgb(MUTED)))
                .child(key.to_string())
        };

        div()
            .size_full()
            .bg(rgb(0xffffff))
            .text_color(rgb(INK))
            .p_6()
            .flex()
            .flex_col()
            .gap_4()
            .on_action(|_: &FocusNext, window, cx| window.focus_next(cx))
            .on_action(|_: &FocusPrev, window, cx| window.focus_prev(cx))
            .child(trigger)
            .child(options)
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
                    .child("Shift and Ctrl extend the selection. Agents may not grant Delete.")
                    .child(list(&self.access, cx)),
            )
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("tab", FocusNext, None),
            KeyBinding::new("shift-tab", FocusPrev, None),
        ]);
        let bounds = Bounds::centered(None, size(px(520.), px(460.)), cx);
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
    });
}
