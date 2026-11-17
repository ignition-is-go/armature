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

#[gpui::test]
fn the_json_protocol_reads_and_operates_components(cx: &mut TestAppContext) {
    let (demo, cx) = open(cx);
    let ask = |cx: &mut VisualTestContext, request: &str| -> serde_json::Value {
        let response = cx.update(|_, cx| agent::handle(request, cx));
        serde_json::from_str(&response).unwrap()
    };

    let listing = ask(cx, r#"{"method":"snapshot"}"#);
    let fruit = &listing["components"][1];
    assert_eq!(fruit["label"], "Fruit");
    assert_eq!(fruit["items"][1]["disabled"], true);
    let reference = fruit["ref"].as_str().unwrap();

    let select =
        format!(r#"{{"method":"act","ref":"{reference}","action":"select","item":"apple"}}"#);
    assert_eq!(ask(cx, &select)["value"][0], "apple");
    demo.update(cx, |demo, cx| {
        assert_eq!(demo.fruit.read(cx).value(), ["apple"])
    });

    assert_eq!(ask(cx, "not json")["ok"], false);
    let unknown = ask(cx, r#"{"method":"click"}"#);
    assert_eq!(
        unknown["error"],
        "unknown method; valid methods: snapshot, query, act, run, history, tools"
    );
    let missing = ask(cx, r#"{"method":"act","ref":"e404","action":"select"}"#);
    assert_eq!(missing["error"], "no component with ref \"e404\"");
}

#[gpui::test]
fn agents_search_run_steps_and_respect_access_rules(cx: &mut TestAppContext) {
    use armature_core::agent::{Access, Step};
    use armature_core::toggle::Toggle;

    let (_demo, cx) = open(cx);
    let (guarded, pin, hidden) = cx.update(|_, cx| {
        let confirm = Access {
            confirm: Some("Delete the account".into()),
            ..Default::default()
        };
        let secret = Access {
            secret: true,
            ..Default::default()
        };
        let absent = Access {
            hidden: true,
            ..Default::default()
        };
        (
            cx.new(|cx| {
                Machine::new(Toggle::default(), "Delete account", [], cx).agent_access(confirm, cx)
            }),
            cx.new(|cx| {
                Machine::new(Toggle::default(), "PIN lock", [], cx).agent_access(secret, cx)
            }),
            cx.new(|cx| {
                Machine::new(Toggle::default(), "Admin mode", [], cx).agent_access(absent, cx)
            }),
        )
    });
    let step = |target: &str, action: &str, item: Option<&str>| Step {
        target: target.into(),
        action: action.into(),
        item: item.map(str::to_string),
    };

    cx.update(|_, cx| {
        let labels: Vec<String> = agent::snapshot(cx)
            .into_iter()
            .map(|(_, s)| s.label)
            .collect();
        assert_eq!(
            labels,
            ["Settings", "Fruit", "Delete account", "PIN lock"],
            "the hidden one is absent"
        );
        assert_eq!(
            agent::query(cx, "blue")[0].item.as_deref(),
            Some("blueberry")
        );

        let steps = [
            step("Fruit", "select", Some("apple")),
            step("Settings", "select", Some("billing")),
        ];
        let outcome = agent::run(cx, &steps, |_, _| true);
        assert_eq!(outcome.changes.len(), 1);
        assert_eq!(
            outcome.failed.unwrap().1.to_string(),
            "item \"billing\" is disabled"
        );

        let delete = [step("Delete account", "check", None)];
        assert!(!agent::run(cx, &delete, |_, _| false).ok());
        assert!(agent::run(cx, &delete, |_, what| what == "Delete the account").ok());

        let locked = agent::run(cx, &[step("PIN lock", "check", None)], |_, _| true);
        assert_eq!(locked.changes[0].after, ["[redacted]"]);

        let history: Vec<String> = agent::history(cx)
            .iter()
            .map(armature_core::agent::describe_entry)
            .collect();
        assert_eq!(
            history,
            [
                "Fruit changed from nothing to apple by an agent.",
                "Delete account changed from nothing to on by an agent.",
                "PIN lock changed from [redacted] to [redacted] by an agent.",
            ]
        );
    });
    assert!(guarded.update(cx, |m, _| m.primitive().is_on()));
    assert!(
        pin.update(cx, |m, _| m.primitive().is_on()),
        "the secret change still happened"
    );
    assert!(!hidden.update(cx, |m, _| m.primitive().is_on()));
}
