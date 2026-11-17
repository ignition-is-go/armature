//! Checks that hold for every primitive, whatever it is. Run them on your own
//! primitives the way this crate runs them on its own.

use std::fmt;

use crate::agent::ActError;
use crate::change::{Source, ValueChange};
use crate::geometry::Point;
use crate::keys::{Key, Modifiers};
use crate::primitive::{Focusable, Input, Item, Pointer, PointerPhase, Primitive, Role, Semantics};

/// One rule a primitive broke.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub rule: &'static str,
    pub detail: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.rule, self.detail)
    }
}

const KEYS: [Key; 14] = [
    Key::ArrowUp,
    Key::ArrowDown,
    Key::ArrowLeft,
    Key::ArrowRight,
    Key::Home,
    Key::End,
    Key::PageUp,
    Key::PageDown,
    Key::Enter,
    Key::Space,
    Key::Escape,
    Key::Tab,
    Key::Char('a'),
    Key::Char('z'),
];

type Instance<P> = (<P as Primitive>::Part, Option<String>);

/// Roles whose items form one composite control, which must have a single tab stop.
fn is_composite_item(role: Option<Role>) -> bool {
    matches!(
        role,
        Some(
            Role::Option
                | Role::Tab
                | Role::Radio
                | Role::TreeItem
                | Role::ListItem
                | Role::GridCell
        )
    )
}

/// Every part instance worth looking at: each part alone, and each part with each item.
fn instances<P: Primitive>(items: &[Item]) -> Vec<Instance<P>> {
    let mut all = Vec::new();
    for part in P::parts() {
        all.push((*part, None));
        all.extend(items.iter().map(|item| (*part, Some(item.key.clone()))));
    }
    all
}

/// Every input a primitive can receive, aimed at every part instance.
fn inputs<P: Primitive>(items: &[Item]) -> Vec<Input<P::Part>> {
    let mods = Modifiers::default();
    let mut all: Vec<Input<P::Part>> = vec![
        Input::PressOutside,
        Input::WindowBlur,
        Input::Tick { now_ms: 10_000 },
        Input::Paste {
            text: "a\tb\nc\td".into(),
        },
    ];
    all.extend(KEYS.iter().map(|key| Input::Key {
        key: *key,
        mods,
        now_ms: 0,
    }));
    for (part, item) in instances::<P>(items) {
        all.push(Input::Focus {
            part,
            item: item.clone(),
        });
        all.push(Input::Press {
            part,
            item: item.clone(),
            mods,
        });
        all.push(Input::Text {
            part,
            item: item.clone(),
            value: "12".into(),
        });
        all.push(Input::Blur {
            part,
            item: item.clone(),
        });
        all.push(Input::Found {
            part,
            item: item.clone(),
        });
        all.push(Input::Hover {
            part,
            item: item.clone(),
            inside: true,
            now_ms: 0,
        });
        all.push(Input::Hover {
            part,
            item,
            inside: false,
            now_ms: 0,
        });
        all.push(Input::Scrolled {
            part,
            offset: 40.0,
            viewport: 100.0,
            now_ms: 0,
        });
    }
    all
}

/// Checks a primitive against the rules every primitive must keep. `make` builds a
/// fresh instance and `items` are registered on it. An empty result means it conforms.
pub fn check<P: Primitive>(make: impl Fn() -> P, items: &[Item]) -> Vec<Violation> {
    let build = |with_items: bool| {
        let mut primitive = make();
        if with_items {
            items
                .iter()
                .cloned()
                .for_each(|item| primitive.upsert_item(item));
        }
        primitive
    };
    let mut violations = Vec::new();
    let mut fail = |rule: &'static str, detail: String| violations.push(Violation { rule, detail });
    let seen = |primitive: &P| -> Vec<(Instance<P>, Semantics)> {
        instances::<P>(items)
            .into_iter()
            .map(|(part, item)| {
                let semantics = primitive.semantics(part, item.as_deref());
                ((part, item), semantics)
            })
            .collect()
    };

    // What the interface offers at rest, and after every input the adapter can send.
    let mut states = vec![("at rest".to_string(), build(true))];
    for input in inputs::<P>(items) {
        let mut primitive = build(true);
        let described = format!("after {input:?}");
        primitive.send(input, &mut |_| true);
        states.push((described, primitive));
    }
    for (when, primitive) in &states {
        let parts = seen(primitive);
        for ((part, item), semantics) in &parts {
            if semantics.hidden && semantics.focusable == Focusable::TabStop {
                fail(
                    "a hidden part is never a tab stop",
                    format!("{part:?} {item:?} {when}"),
                );
            }
            if semantics.disabled && semantics.focusable == Focusable::TabStop && item.is_some() {
                fail(
                    "a disabled item is never the tab stop",
                    format!("{part:?} {item:?} {when}"),
                );
            }
        }
        let stops = parts
            .iter()
            .filter(|((part, item), semantics)| {
                P::is_item(*part)
                    && item.is_some()
                    && is_composite_item(semantics.role)
                    && semantics.focusable == Focusable::TabStop
            })
            .count();
        if stops > 1 {
            fail(
                "a composite control has one tab stop",
                format!("{stops} items are tab stops {when}"),
            );
        }
        let selected: Vec<String> = primitive
            .snapshot("")
            .items
            .into_iter()
            .filter(|item| item.selected)
            .map(|item| item.key)
            .collect();
        let value = primitive.value();
        if let Some(stray) = selected.iter().find(|key| !value.contains(key)) {
            fail(
                "an item shown as selected is in the value",
                format!("{stray} is not in {value:?} {when}"),
            );
        }
    }
    let reachable = seen(&build(true))
        .iter()
        .any(|(_, semantics)| !semantics.hidden && semantics.focusable == Focusable::TabStop);
    let focusable = seen(&build(true))
        .iter()
        .any(|(_, semantics)| semantics.focusable != Focusable::No);
    if focusable && !reachable {
        fail(
            "the keyboard can reach it",
            "nothing is a tab stop at rest".into(),
        );
    }

    // A button or switch that a press changes must answer Enter the same way, since
    // an unstyled part gets no keyboard activation for free.
    for (part, item) in instances::<P>(items) {
        let at_rest = build(true).semantics(part, item.as_deref());
        if !matches!(at_rest.role, Some(Role::Button | Role::Switch))
            || at_rest.focusable == Focusable::No
        {
            continue;
        }
        let mods = Modifiers::default();
        let mut pressed = build(true);
        pressed.send(
            Input::Press {
                part,
                item: item.clone(),
                mods,
            },
            &mut |_| true,
        );
        let mut keyed = build(true);
        keyed.send(
            Input::Focus {
                part,
                item: item.clone(),
            },
            &mut |_| true,
        );
        keyed.send(
            Input::Key {
                key: Key::Enter,
                mods,
                now_ms: 0,
            },
            &mut |_| true,
        );
        if pressed.value() != keyed.value() {
            fail(
                "Enter presses a focused button",
                format!("{part:?} {item:?}"),
            );
        }
    }

    // Robustness: no input may panic, with items or without, and a refused change
    // leaves the value alone.
    for with_items in [true, false] {
        for input in inputs::<P>(items) {
            let mut primitive = build(with_items);
            let before = primitive.value();
            let described = format!("{input:?}");
            let mut sources = Vec::new();
            primitive.send(input, &mut |change: &ValueChange| {
                sources.push(change.source);
                false
            });
            if primitive.value() != before {
                fail("a refused change leaves the value alone", described.clone());
            }
            if sources.iter().any(|source| *source != Source::User) {
                fail("a person's change is marked as theirs", described);
            }
        }
        for phase in [
            PointerPhase::Down,
            PointerPhase::Move,
            PointerPhase::Up,
            PointerPhase::Cancel,
            PointerPhase::Move,
        ] {
            for (part, item) in instances::<P>(items) {
                let mut primitive = build(with_items);
                let pointer = Pointer {
                    phase,
                    part,
                    item,
                    at: Point::new(5.0, 5.0),
                    mods: Modifiers::default(),
                    now_ms: 0,
                };
                primitive.pointer(pointer, &|_, _| None, &mut |_| true);
            }
        }
    }

    // The agent's side.
    let actions = build(true).snapshot("").actions;
    match build(true).act("no such action", None, &mut |_| true) {
        Err(ActError::UnknownAction { valid, .. }) if valid == actions && !valid.is_empty() => {}
        other => fail(
            "an unknown action names the valid ones",
            format!("got {other:?}"),
        ),
    }
    let mut targets: Vec<Option<&str>> = vec![None];
    targets.extend(items.iter().map(|item| Some(item.key.as_str())));
    for action in actions {
        for target in &targets {
            let mut refused = build(true);
            let before = refused.value();
            let mut sources = Vec::new();
            let _ = refused.act(action, *target, &mut |change: &ValueChange| {
                sources.push(change.source);
                false
            });
            if refused.value() != before {
                fail(
                    "a refused agent action leaves the value alone",
                    format!("{action} {target:?}"),
                );
            }
            if sources.iter().any(|source| *source != Source::Agent) {
                fail(
                    "an agent's change is marked as an agent's",
                    format!("{action} {target:?}"),
                );
            }

            // Whatever value an action reaches, the owner can put back.
            let mut acted = build(true);
            if acted.act(action, *target, &mut |_| true).is_ok() {
                let reached = acted.value();
                let mut restored = build(true);
                restored.set_value(reached.clone());
                if restored.value() != reached {
                    fail(
                        "the owner can set any value an action can reach",
                        format!(
                            "{action} {target:?} reached {reached:?} but set_value gave {:?}",
                            restored.value()
                        ),
                    );
                }
            }
        }
    }
    // One report per rule, with how often it broke.
    let mut reported: Vec<Violation> = Vec::new();
    for violation in violations {
        match reported.iter_mut().find(|seen| seen.rule == violation.rule) {
            Some(seen) if seen.detail.ends_with(" more)") => {}
            Some(seen) => seen.detail.push_str(" (and more)"),
            None => reported.push(violation),
        }
    }
    reported
}

/// Panics with every broken rule, for use in a test.
pub fn assert_conforms<P: Primitive>(make: impl Fn() -> P, items: &[Item]) {
    let violations = check(make, items);
    let listed: Vec<String> = violations.iter().map(Violation::to_string).collect();
    assert!(
        violations.is_empty(),
        "{} rules broken:\n  {}",
        listed.len(),
        listed.join("\n  ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accordion::{self, Accordion};
    use crate::command::Command;
    use crate::keys::Dir;
    use crate::listbox::{self, Listbox};
    use crate::number_input::NumberInput;
    use crate::radio::Radio;
    use crate::select::Select;
    use crate::selection::Mode;
    use crate::slider::Slider;
    use crate::sortable::Sortable;
    use crate::tabs::Tabs;
    use crate::toggle::Toggle;
    use crate::tree::tree_from_paths;
    use crate::tree_view::TreeView;

    fn items(keys: &[(&str, bool)]) -> Vec<Item> {
        keys.iter()
            .map(|(key, disabled)| Item {
                key: key.to_string(),
                label: key.to_string(),
                disabled: *disabled,
            })
            .collect()
    }

    fn mixed() -> Vec<Item> {
        items(&[("alpha", false), ("beta", true), ("gamma", false)])
    }

    #[test]
    fn listbox_conforms_in_both_modes() {
        assert_conforms(Listbox::default, &mixed());
        assert_conforms(
            || {
                Listbox::new(listbox::Options {
                    mode: Mode::Multiple,
                    ..Default::default()
                })
            },
            &mixed(),
        );
    }

    #[test]
    fn tabs_conform() {
        assert_conforms(Tabs::default, &mixed());
    }

    #[test]
    fn select_conforms() {
        assert_conforms(Select::default, &mixed());
    }

    #[test]
    fn slider_conforms() {
        assert_conforms(Slider::default, &[]);
    }

    #[test]
    fn switch_conforms() {
        assert_conforms(Toggle::default, &[]);
    }

    #[test]
    fn accordion_conforms_in_every_mode() {
        for (multiple, collapsible) in [(false, false), (true, false), (false, true), (true, true)]
        {
            assert_conforms(
                || {
                    Accordion::new(accordion::Options {
                        multiple,
                        collapsible,
                        findable: true,
                    })
                },
                &mixed(),
            );
        }
    }

    #[test]
    fn radio_group_conforms() {
        assert_conforms(|| Radio::new(Dir::Ltr), &mixed());
    }

    #[test]
    fn tree_view_conforms() {
        let nodes = items(&[
            ("src", false),
            ("src/main.rs", false),
            ("src/ui", true),
            ("docs", false),
        ]);
        assert_conforms(
            || {
                TreeView::new(
                    tree_from_paths(&["src/main.rs", "src/ui/x.rs", "docs"]),
                    Dir::Ltr,
                )
            },
            &nodes,
        );
    }

    #[test]
    fn command_palette_conforms() {
        assert_conforms(Command::default, &mixed());
    }

    #[test]
    fn number_input_conforms() {
        assert_conforms(NumberInput::default, &[]);
    }

    #[test]
    fn sortable_list_conforms() {
        assert_conforms(
            Sortable::default,
            &items(&[("alpha", false), ("beta", false), ("gamma", false)]),
        );
    }

    #[test]
    fn a_broken_primitive_is_caught() {
        /// A listbox that marks its changes as the agent's whoever made them.
        #[derive(Clone, Default)]
        struct Mislabelled(Listbox);
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        struct Only;
        impl crate::primitive::PartOf for Only {
            type Primitive = Mislabelled;
        }
        impl Primitive for Mislabelled {
            type Part = Only;
            fn parts() -> &'static [Only] {
                &[Only]
            }
            fn is_item(_: Only) -> bool {
                true
            }
            fn upsert_item(&mut self, item: Item) {
                self.0.upsert_item(item);
            }
            fn remove_item(&mut self, key: &str) {
                self.0.remove_item(key);
            }
            fn set_order(&mut self, rendered: &[(String, String)]) {
                self.0.set_order(rendered);
            }
            fn semantics(&self, _: Only, item: Option<&str>) -> Semantics {
                self.0.semantics(listbox::Part::Option, item)
            }
            fn send(
                &mut self,
                input: Input<Only>,
                gate: crate::change::Gate,
            ) -> crate::primitive::Reaction<Only> {
                if let Input::Press { item, mods, .. } = input {
                    let press = Input::Press {
                        part: listbox::Part::Option,
                        item,
                        mods,
                    };
                    self.0.send(press, &mut |change| {
                        gate(&ValueChange {
                            source: Source::Agent,
                            ..change.clone()
                        })
                    });
                }
                Default::default()
            }
            fn value(&self) -> Vec<String> {
                self.0.value()
            }
            fn set_value(&mut self, value: Vec<String>) {
                self.0.set_value(value);
            }
            fn snapshot(&self, label: &str) -> crate::agent::Snapshot {
                self.0.snapshot(label)
            }
            fn act(
                &mut self,
                action: &str,
                item: Option<&str>,
                gate: crate::change::Gate,
            ) -> Result<(), ActError> {
                self.0.act(action, item, gate)
            }
        }
        let rules: Vec<&str> = check(Mislabelled::default, &mixed())
            .into_iter()
            .map(|violation| violation.rule)
            .collect();
        assert!(
            rules.contains(&"a person's change is marked as theirs"),
            "{rules:?}"
        );
    }
}
