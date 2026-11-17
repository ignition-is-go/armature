use armature_core::listbox::{self, Listbox};
use armature_core::primitive::Item;
use armature_core::tabs::{self, Tabs};
use armature_gpui::{Machine, Source, agent, part};
use gpui::prelude::*;
use gpui::{Context, Entity, TestAppContext, VisualTestContext, Window, div};

fn item(key: &str, disabled: bool) -> Item {
    Item {
        key: key.into(),
        label: key.into(),
        disabled,
    }
}

struct Demo {
    tabs: Entity<Machine<Tabs>>,
    fruit: Entity<Machine<Listbox>>,
}

impl Demo {
    fn new(cx: &mut Context<Self>) -> Self {
        let tabs = [
            item("fruit", false),
            item("billing", true),
            item("access", false),
        ];
        let fruit = [
            item("apple", false),
            item("banana", true),
            item("blueberry", false),
        ];
        Demo {
            tabs: cx.new(|cx| Machine::new(Tabs::default(), "Settings", tabs, cx)),
            fruit: cx.new(|cx| {
                Machine::new(Listbox::default(), "Fruit", fruit, cx).on_value_change(|change, _| {
                    !(change.source == Source::Agent && change.value == ["blueberry"])
                })
            }),
        }
    }
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = |key| part(&self.tabs, tabs::Part::Tab, Some(key), cx).child(key);
        let option = |key| part(&self.fruit, listbox::Part::Option, Some(key), cx).child(key);
        div()
            .child(
                part(&self.tabs, tabs::Part::List, None, cx)
                    .flex()
                    .children(["fruit", "billing", "access"].map(tab)),
            )
            .child(
                part(&self.tabs, tabs::Part::Panel, Some("fruit"), cx).child(
                    part(&self.fruit, listbox::Part::Root, None, cx)
                        .children(["apple", "banana", "blueberry"].map(option)),
                ),
            )
            .child(part(&self.tabs, tabs::Part::Panel, Some("access"), cx).child("access"))
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<Demo>, &mut VisualTestContext) {
    cx.add_window_view(|_, cx| Demo::new(cx))
}

#[gpui::test]
fn a_listbox_is_driven_by_real_keystrokes(cx: &mut TestAppContext) {
    let (demo, cx) = open(cx);
    demo.update_in(cx, |demo, window, cx| {
        let first = demo.fruit.read(cx).tab_stop(listbox::Part::Option).unwrap();
        first.focus(window, cx);
    });

    cx.simulate_keystrokes("down space");

    demo.update(cx, |demo, cx| {
        let fruit = demo.fruit.read(cx);
        assert_eq!(
            fruit.value(),
            ["blueberry"],
            "down skipped the disabled item"
        );
        assert!(
            fruit
                .semantics(listbox::Part::Option, Some("blueberry"))
                .active
        );
    });
}

#[gpui::test]
fn tabs_are_driven_by_the_same_adapter(cx: &mut TestAppContext) {
    let (demo, cx) = open(cx);
    demo.update_in(cx, |demo, window, cx| {
        let current = demo.tabs.read(cx).tab_stop(tabs::Part::Tab).unwrap();
        current.focus(window, cx);
    });

    cx.simulate_keystrokes("right");

    demo.update(cx, |demo, cx| {
        let tabs = demo.tabs.read(cx);
        assert_eq!(tabs.value(), ["access"]);
        assert!(tabs.semantics(tabs::Part::Panel, Some("fruit")).hidden);
    });
}

#[gpui::test]
fn an_agent_reads_and_operates_components_without_input_events(cx: &mut TestAppContext) {
    let (demo, cx) = open(cx);
    cx.update(|_, cx| {
        let snapshot = agent::snapshot(cx);
        let labels: Vec<_> = snapshot.iter().map(|(_, s)| s.label.as_str()).collect();
        assert_eq!(labels, ["Settings", "Fruit"]);
        let fruit = &snapshot[1].0;

        assert_eq!(
            agent::act(cx, fruit, "select", Some("apple")).unwrap(),
            ["apple"]
        );
        let refused = agent::act(cx, fruit, "select", Some("blueberry")).unwrap_err();
        assert_eq!(refused.to_string(), "the application refused this change");
        let unknown = agent::act(cx, fruit, "open", None).unwrap_err();
        assert!(
            unknown
                .to_string()
                .contains("valid actions: select, deselect, clear")
        );
    });
    demo.update(cx, |demo, cx| {
        assert_eq!(demo.fruit.read(cx).value(), ["apple"])
    });
}

#[gpui::test]
fn a_dropped_component_leaves_the_agent_registry(cx: &mut TestAppContext) {
    let extra = cx.new(|cx| Machine::new(Tabs::default(), "Extra", [item("a", false)], cx));
    assert_eq!(cx.update(|cx| agent::snapshot(cx).len()), 1);
    drop(extra);
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| agent::snapshot(cx).len()), 0);
}

struct StyledPanels {
    tabs: Entity<Machine<Tabs>>,
    fruit: Entity<Machine<Listbox>>,
}

impl Render for StyledPanels {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let apple = part(&self.fruit, listbox::Part::Option, Some("apple"), cx)
            .w(gpui::px(100.))
            .h(gpui::px(20.));
        div()
            .child(
                part(&self.tabs, tabs::Part::Panel, Some("fruit"), cx)
                    .flex()
                    .child(apple),
            )
            .child(
                part(&self.tabs, tabs::Part::Panel, Some("access"), cx)
                    .flex()
                    .child("access"),
            )
    }
}

#[gpui::test]
fn styling_a_part_cannot_reveal_a_hidden_panel(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| StyledPanels {
        tabs: cx.new(|cx| {
            let tabs = [item("fruit", false), item("access", false)];
            Machine::new(Tabs::default(), "Settings", tabs, cx)
        }),
        fruit: cx.new(|cx| Machine::new(Listbox::default(), "Fruit", [item("apple", false)], cx)),
    });
    let in_apple = gpui::point(gpui::px(10.), gpui::px(10.));
    let fruit = |cx: &mut VisualTestContext| view.update(cx, |v, cx| v.fruit.read(cx).value());

    cx.update(|_, cx| agent::act(cx, "e1", "select", Some("access")).unwrap());
    cx.run_until_parked();
    cx.simulate_click(in_apple, gpui::Modifiers::default());
    assert!(
        fruit(cx).is_empty(),
        "the fruit panel is hidden, so nothing was pressed"
    );

    cx.update(|_, cx| agent::act(cx, "e1", "select", Some("fruit")).unwrap());
    cx.run_until_parked();
    cx.simulate_click(in_apple, gpui::Modifiers::default());
    assert_eq!(fruit(cx), ["apple"]);
}
