//! Checks that hold for every primitive, whatever it is. Run them on your own
//! primitives the way this crate runs them on its own.

use std::fmt;

use crate::agent::ActError;
use crate::change::{Source, ValueChange};
use crate::geometry::Point;
use crate::keys::{Key, Modifiers};
use crate::primitive::{
    Focusable, Input, Item, Outcome, Pointer, PointerPhase, Primitive, Reaction, Role, Semantics,
};

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
const fn is_composite_item(role: Option<Role>) -> bool {
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
        Input::Hotkey {
            shortcut: crate::shortcut::Shortcut {
                key: Key::F6,
                mods: Modifiers::default(),
            },
        },
        Input::DragOver { inside: true },
        Input::Files {
            files: vec![crate::primitive::FileInfo {
                name: "a.png".into(),
                kind: "image/png".into(),
                size: 1,
                path: None,
            }],
            dropped: true,
        },
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
        all.push(Input::Loaded {
            part,
            outcome: Outcome::Loaded,
        });
        all.push(Input::Wheel {
            part,
            item: item.clone(),
            delta: 1.0,
            mods,
        });
        all.push(Input::ContextMenu {
            part,
            item: item.clone(),
            at: crate::geometry::Point::new(5.0, 5.0),
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
            content: 10_000.0,
            now_ms: 0,
        });
    }
    all
}

/// One run of the conformance rules over a primitive, collecting what it breaks.
struct Checker<'a, P, M> {
    make: M,
    items: &'a [Item],
    violations: Vec<Violation>,
    primitive: std::marker::PhantomData<P>,
}

impl<P: Primitive, M: Fn() -> P> Checker<'_, P, M> {
    fn build(&self, with_items: bool) -> P {
        let mut primitive = (self.make)();
        if with_items {
            self.items
                .iter()
                .cloned()
                .for_each(|item| primitive.upsert_item(item));
        }
        primitive
    }

    fn fail(&mut self, rule: &'static str, detail: String) {
        self.violations.push(Violation { rule, detail });
    }

    /// Every part and item, with what the primitive says of it.
    fn seen(&self, primitive: &P) -> Vec<(Instance<P>, Semantics)> {
        instances::<P>(self.items)
            .into_iter()
            .map(|(part, item)| {
                let semantics = primitive.semantics(part, item.as_deref());
                ((part, item), semantics)
            })
            .collect()
    }

    /// What the interface offers at rest, and after every input the adapter can send.
    fn states(&self) -> Vec<(String, P)> {
        let mut states = vec![("at rest".to_string(), self.build(true))];
        for input in inputs::<P>(self.items) {
            let mut primitive = self.build(true);
            let described = format!("after {input:?}");
            primitive.send(input, &mut |_| true);
            states.push((described, primitive));
        }
        states
    }

    /// Focus rules, for one state: hidden and disabled parts are not tab stops, and a
    /// composite control has one.
    fn check_tab_stops(&mut self, when: &str, primitive: &P) {
        let parts = self.seen(primitive);
        for ((part, item), semantics) in &parts {
            if semantics.hidden && semantics.focusable == Focusable::TabStop {
                self.fail(
                    "a hidden part is never a tab stop",
                    format!("{part:?} {item:?} {when}"),
                );
            }
            if semantics.disabled && semantics.focusable == Focusable::TabStop && item.is_some() {
                self.fail(
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
            self.fail(
                "a composite control has one tab stop",
                format!("{stops} items are tab stops {when}"),
            );
        }
    }

    /// An item shown as selected is in the value.
    fn check_selection(&mut self, when: &str, primitive: &P) {
        let selected: Vec<String> = primitive
            .snapshot("")
            .items
            .into_iter()
            .filter(|item| item.selected)
            .map(|item| item.key)
            .collect();
        let value = primitive.value();
        if let Some(stray) = selected.iter().find(|key| !value.contains(key)) {
            self.fail(
                "an item shown as selected is in the value",
                format!("{stray} is not in {value:?} {when}"),
            );
        }
    }

    fn check_keyboard_reach(&mut self) {
        let at_rest = self.seen(&self.build(true));
        let reachable = at_rest
            .iter()
            .any(|(_, semantics)| !semantics.hidden && semantics.focusable == Focusable::TabStop);
        let focusable = at_rest
            .iter()
            .any(|(_, semantics)| semantics.focusable != Focusable::No);
        if focusable && !reachable {
            self.fail(
                "the keyboard can reach it",
                "nothing is a tab stop at rest".into(),
            );
        }
    }

    /// A button or switch that a press changes must answer Enter the same way, since
    /// an unstyled part gets no keyboard activation for free.
    fn check_enter_presses(&mut self) {
        for (part, item) in instances::<P>(self.items) {
            let at_rest = self.build(true).semantics(part, item.as_deref());
            if !matches!(at_rest.role, Some(Role::Button | Role::Switch))
                || at_rest.focusable == Focusable::No
            {
                continue;
            }
            let mods = Modifiers::default();
            let mut pressed = self.build(true);
            pressed.send(
                Input::Press {
                    part,
                    item: item.clone(),
                    mods,
                },
                &mut |_| true,
            );
            let mut keyed = self.build(true);
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
                self.fail(
                    "Enter presses a focused button",
                    format!("{part:?} {item:?}"),
                );
            }
        }
    }

    /// A refused change leaves the value alone, and a person's change is marked as theirs.
    fn check_refusals(&mut self, with_items: bool) {
        for input in inputs::<P>(self.items) {
            let mut primitive = self.build(with_items);
            let before = primitive.value();
            let described = format!("{input:?}");
            let mut sources = Vec::new();
            primitive.send(input, &mut |change: &ValueChange| {
                sources.push(change.source);
                false
            });
            if primitive.value() != before {
                self.fail("a refused change leaves the value alone", described.clone());
            }
            if sources.iter().any(|source| *source != Source::User) {
                self.fail("a person's change is marked as theirs", described);
            }
        }
    }

    /// No pointer gesture may panic, on any part, in any order.
    fn check_pointers(&self, with_items: bool) {
        for phase in [
            PointerPhase::Down,
            PointerPhase::Move,
            PointerPhase::Up,
            PointerPhase::Cancel,
            PointerPhase::Move,
        ] {
            for (part, item) in instances::<P>(self.items) {
                let mut primitive = self.build(with_items);
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

    /// The agent's side: an unknown action names the valid ones, and every action
    /// keeps the rules below on every target.
    fn check_agent(&mut self) {
        let actions = self.build(true).snapshot("").actions;
        match self.build(true).act("no such action", None, &mut |_| true) {
            // A read-only primitive, such as a meter, has no actions to name.
            Err(ActError::UnknownAction { valid, .. }) if valid == actions => {}
            other => self.fail(
                "an unknown action names the valid ones",
                format!("got {other:?}"),
            ),
        }
        let mut targets: Vec<Option<&str>> = vec![None];
        targets.extend(self.items.iter().map(|item| Some(item.key.as_str())));
        for action in actions {
            for target in &targets {
                self.check_refused_action(action, *target);
                self.check_owner_can_restore(action, *target);
            }
        }
    }

    fn check_refused_action(&mut self, action: &'static str, target: Option<&str>) {
        let mut refused = self.build(true);
        let before = refused.value();
        let mut sources = Vec::new();
        let _ = refused.act(action, target, &mut |change: &ValueChange| {
            sources.push(change.source);
            false
        });
        if refused.value() != before {
            self.fail(
                "a refused agent action leaves the value alone",
                format!("{action} {target:?}"),
            );
        }
        if sources.iter().any(|source| *source != Source::Agent) {
            self.fail(
                "an agent's change is marked as an agent's",
                format!("{action} {target:?}"),
            );
        }
    }

    /// Whatever value an action reaches, the owner can put back.
    fn check_owner_can_restore(&mut self, action: &'static str, target: Option<&str>) {
        let mut acted = self.build(true);
        if acted.act(action, target, &mut |_| true).is_ok() {
            let reached = acted.value();
            let mut restored = self.build(true);
            restored.set_value(reached.clone());
            if restored.value() != reached {
                self.fail(
                    "the owner can set any value an action can reach",
                    format!(
                        "{action} {target:?} reached {reached:?} but set_value gave {:?}",
                        restored.value()
                    ),
                );
            }
        }
    }

    /// One report per rule, with a note when it broke more than once.
    fn report(self) -> Vec<Violation> {
        let mut reported: Vec<Violation> = Vec::new();
        for violation in self.violations {
            match reported.iter_mut().find(|seen| seen.rule == violation.rule) {
                Some(seen) if seen.detail.ends_with(" more)") => {}
                Some(seen) => seen.detail.push_str(" (and more)"),
                None => reported.push(violation),
            }
        }
        reported
    }
}

/// Sends one key with no modifiers held, allowing whatever change it proposes, as
/// tests do to step through a primitive.
pub fn press_key<P: Primitive>(primitive: &mut P, key: Key) -> Reaction<P::Part> {
    primitive.send(
        Input::Key {
            key,
            mods: Modifiers::default(),
            now_ms: 0,
        },
        &mut |_| true,
    )
}

/// Checks a primitive against the rules every primitive must keep. `make` builds a
/// fresh instance and `items` are registered on it. An empty result means it conforms.
pub fn check<P: Primitive>(make: impl Fn() -> P, items: &[Item]) -> Vec<Violation> {
    let mut checker = Checker {
        make,
        items,
        violations: Vec::new(),
        primitive: std::marker::PhantomData,
    };
    for (when, primitive) in &checker.states() {
        checker.check_tab_stops(when, primitive);
        checker.check_selection(when, primitive);
    }
    checker.check_keyboard_reach();
    checker.check_enter_presses();
    // Robustness: no input may panic, with items or without.
    for with_items in [true, false] {
        checker.check_refusals(with_items);
        checker.check_pointers(with_items);
    }
    checker.check_agent();
    checker.report()
}

/// Panics with every broken rule, for use in a test.
///
/// # Panics
///
/// When the primitive breaks any rule, listing each one.
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
                crate::primitive::Reaction::default()
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
