use std::fmt;
use std::rc::Rc;

use crate::change::Source;
use crate::text_match::fuzzy_score;

/// What an agent sees of one component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub name: &'static str,
    pub label: String,
    pub value: Vec<String>,
    /// Whether a popup is open, for components that have one.
    pub expanded: Option<bool>,
    pub actions: &'static [&'static str],
    pub items: Vec<SnapshotItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotItem {
    pub key: String,
    pub label: String,
    pub disabled: bool,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActError {
    UnknownAction {
        action: String,
        valid: &'static [&'static str],
    },
    MissingItem {
        action: &'static str,
    },
    UnknownItem(String),
    Disabled(String),
    /// The owner's change gate cancelled it.
    Refused,
}

impl fmt::Display for ActError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownAction { action, valid } => {
                write!(
                    f,
                    "unknown action \"{action}\"; valid actions: {}",
                    valid.join(", ")
                )
            }
            Self::MissingItem { action } => write!(f, "action \"{action}\" needs an item"),
            Self::UnknownItem(key) => write!(f, "no item \"{key}\""),
            Self::Disabled(key) => write!(f, "item \"{key}\" is disabled"),
            Self::Refused => write!(f, "the application refused this change"),
        }
    }
}

impl std::error::Error for ActError {}

type SnapshotFn<Cx> = dyn Fn(&Cx) -> Option<Snapshot>;
type ActFn<Cx> = dyn Fn(&mut Cx, &str, Option<&str>) -> Result<Vec<String>, ActError>;
type SetFn<Cx> = dyn Fn(&mut Cx, Vec<String>);

/// What an agent is allowed to see and do with one component.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Access {
    /// The agent is not told the component exists.
    pub hidden: bool,
    /// The agent sees the component but not its value.
    pub secret: bool,
    /// Acting on the component needs a person's approval. The text says what is being
    /// approved.
    pub confirm: Option<String>,
}

const REDACTED: &str = "[redacted]";

/// How a registry reaches one mounted component. `Cx` is whatever the framework
/// needs to read and write state, such as `()` for signals or GPUI's `App`.
pub struct Target<Cx> {
    /// `None` once the component is gone.
    pub snapshot: Rc<SnapshotFn<Cx>>,
    /// Runs one action and returns the value afterwards.
    pub act: Rc<ActFn<Cx>>,
    /// Puts a value back without asking the owner, to undo or revert.
    pub set: Rc<SetFn<Cx>>,
    pub access: Access,
}

impl<Cx> Clone for Target<Cx> {
    fn clone(&self) -> Self {
        Self {
            snapshot: self.snapshot.clone(),
            act: self.act.clone(),
            set: self.set.clone(),
            access: self.access.clone(),
        }
    }
}

impl<Cx> Target<Cx> {
    /// Runs one action. A component that needs approval refuses; use `run` with an
    /// approver for those.
    ///
    /// # Errors
    ///
    /// When the component needs approval, or refuses the action.
    pub fn act(&self, cx: &mut Cx, action: &str, item: Option<&str>) -> Result<Vec<String>, Error> {
        self.access.confirm.as_ref().map_or_else(
            || self.act_approved(cx, action, item),
            |what| Err(Error::NeedsApproval(what.clone())),
        )
    }

    fn act_approved(
        &self,
        cx: &mut Cx,
        action: &str,
        item: Option<&str>,
    ) -> Result<Vec<String>, Error> {
        let value = (self.act)(cx, action, item).map_err(Error::Act)?;
        Ok(if self.access.secret {
            vec![REDACTED.to_string()]
        } else {
            value
        })
    }

    fn seen(&self, cx: &Cx) -> Option<Snapshot> {
        let mut snapshot = (self.snapshot)(cx).filter(|_| !self.access.hidden)?;
        if self.access.secret {
            snapshot.value = vec![REDACTED.to_string()];
            snapshot
                .items
                .iter_mut()
                .for_each(|item| item.selected = false);
        }
        Some(snapshot)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    UnknownRef(String),
    /// A label that names no component, or more than one.
    UnknownLabel {
        label: String,
        matches: Vec<String>,
    },
    NeedsApproval(String),
    NotApproved,
    Act(ActError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownRef(r) => write!(f, "no component with ref \"{r}\""),
            Self::UnknownLabel { label, matches } if matches.is_empty() => {
                write!(f, "no component labelled \"{label}\"")
            }
            Self::UnknownLabel { label, matches } => {
                write!(
                    f,
                    "\"{label}\" names several components; use a ref: {}",
                    matches.join(", ")
                )
            }
            Self::NeedsApproval(what) => write!(f, "this needs a person's approval: {what}"),
            Self::NotApproved => write!(f, "the action was not approved"),
            Self::Act(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

/// A component or item whose label matches a search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub reference: String,
    /// The matching item's key, or `None` when the component itself matched.
    pub item: Option<String>,
    pub label: String,
}

/// One action in a sequence. `target` is a component's ref or its label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub target: String,
    pub action: String,
    pub item: Option<String>,
}

/// What one step did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub reference: String,
    pub label: String,
    pub before: Vec<String>,
    pub after: Vec<String>,
}

/// The outcome of a sequence of steps. It stops at the first failure.
#[derive(Debug, PartialEq, Eq)]
pub struct Run {
    pub changes: Vec<Change>,
    /// The step that failed, counted from 0, and why.
    pub failed: Option<(usize, Error)>,
    /// Refs of components that were not there before the run, such as a dialog a step
    /// opened.
    pub appeared: Vec<String>,
}

impl Run {
    #[must_use]
    pub const fn ok(&self) -> bool {
        self.failed.is_none()
    }
}

/// Values tried on real components, with what they replaced.
pub struct Staged {
    previous: Vec<(String, Vec<String>)>,
}

/// Every mounted component, addressable by an agent through a stable ref.
pub struct Registry<Cx> {
    next_id: u32,
    targets: Vec<(u32, Target<Cx>)>,
}

impl<Cx> Default for Registry<Cx> {
    fn default() -> Self {
        Self {
            next_id: 1,
            targets: Vec::new(),
        }
    }
}

impl<Cx> Clone for Registry<Cx> {
    fn clone(&self) -> Self {
        Self {
            next_id: self.next_id,
            targets: self.targets.clone(),
        }
    }
}

impl<Cx> Registry<Cx> {
    pub fn register(&mut self, target: Target<Cx>) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.targets.push((id, target));
        id
    }

    /// Changes what an agent may see and do with a registered component.
    pub fn set_access(&mut self, id: u32, access: Access) {
        if let Some((_, target)) = self.targets.iter_mut().find(|(i, _)| *i == id) {
            target.access = access;
        }
    }

    pub fn unregister(&mut self, id: u32) {
        self.targets.retain(|(i, _)| *i != id);
    }

    /// What an agent may see: hidden components are left out and secret values are
    /// redacted.
    pub fn snapshot(&self, cx: &Cx) -> Vec<(String, Snapshot)> {
        self.targets
            .iter()
            .filter_map(|(id, target)| Some((format!("e{id}"), target.seen(cx)?)))
            .collect()
    }

    /// The target behind a ref, cloned out so the registry is not borrowed while
    /// application code runs.
    ///
    /// # Errors
    ///
    /// When no component has that ref.
    pub fn target(&self, reference: &str) -> Result<Target<Cx>, Error> {
        self.targets
            .iter()
            .find(|(id, target)| format!("e{id}") == reference && !target.access.hidden)
            .map(|(_, target)| target.clone())
            .ok_or_else(|| Error::UnknownRef(reference.to_string()))
    }

    /// Components and items whose labels match `text`, best match first.
    pub fn query(&self, cx: &Cx, text: &str) -> Vec<Found> {
        let mut scored = Vec::new();
        for (reference, snapshot) in self.snapshot(cx) {
            if let Some(score) = fuzzy_score(text, &snapshot.label) {
                scored.push((
                    score,
                    Found {
                        reference: reference.clone(),
                        item: None,
                        label: snapshot.label.clone(),
                    },
                ));
            }
            for item in &snapshot.items {
                if let Some(score) = fuzzy_score(text, &item.label) {
                    let found = Found {
                        reference: reference.clone(),
                        item: Some(item.key.clone()),
                        label: item.label.clone(),
                    };
                    scored.push((score, found));
                }
            }
        }
        scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        scored.into_iter().map(|(_, found)| found).collect()
    }

    /// The ref for a step's target, which may be a ref or a component's label.
    fn resolve(&self, cx: &Cx, target: &str) -> Result<String, Error> {
        let seen = self.snapshot(cx);
        if seen.iter().any(|(reference, _)| reference == target) {
            return Ok(target.to_string());
        }
        let named: Vec<String> = seen
            .iter()
            .filter(|(_, snapshot)| snapshot.label.eq_ignore_ascii_case(target))
            .map(|(reference, _)| reference.clone())
            .collect();
        match named.as_slice() {
            [only] => Ok(only.clone()),
            _ => Err(Error::UnknownLabel {
                label: target.to_string(),
                matches: named,
            }),
        }
    }

    /// Tries `values` on the components they name, remembering what they replace.
    /// Nothing asks the owner, since the point is to look before deciding.
    ///
    /// # Errors
    ///
    /// When a ref names no component. What was already tried is put back first.
    pub fn stage(&self, cx: &mut Cx, values: &[(String, Vec<String>)]) -> Result<Staged, Error> {
        let mut previous = Vec::new();
        for (target, value) in values {
            let reference = self.resolve(cx, target)?;
            let target = self.target(&reference)?;
            let before = (target.snapshot)(cx)
                .map(|snapshot| snapshot.value)
                .unwrap_or_default();
            (target.set)(cx, value.clone());
            previous.push((reference, before));
        }
        Ok(Staged { previous })
    }

    /// Puts back everything a stage replaced, last change first.
    pub fn revert(&self, cx: &mut Cx, staged: Staged) {
        for (reference, before) in staged.previous.into_iter().rev() {
            if let Ok(target) = self.target(&reference) {
                (target.set)(cx, before);
            }
        }
    }
}

/// Runs steps in order against `registry`, stopping at the first that fails.
/// `approve` is asked before any step on a component that needs approval.
///
/// The registry is passed by value because steps run application code, which may
/// mount or remove components. Pass a clone taken for this run.
pub fn run<Cx>(
    registry: impl Fn(&Cx) -> Registry<Cx>,
    cx: &mut Cx,
    steps: &[Step],
    mut approve: impl FnMut(&Step, &str) -> bool,
) -> Run {
    let refs = |cx: &Cx| -> Vec<String> {
        registry(cx)
            .snapshot(cx)
            .into_iter()
            .map(|(reference, _)| reference)
            .collect()
    };
    let before_run = refs(cx);
    let mut changes = Vec::new();
    let mut failed = None;

    for (index, step) in steps.iter().enumerate() {
        let current = registry(cx);
        let outcome = (|| {
            let reference = current.resolve(cx, &step.target)?;
            let target = current.target(&reference)?;
            if let Some(what) = &target.access.confirm
                && !approve(step, what)
            {
                return Err(Error::NotApproved);
            }
            let seen = target
                .seen(cx)
                .ok_or_else(|| Error::UnknownRef(reference.clone()))?;
            let after = target.act_approved(cx, &step.action, step.item.as_deref())?;
            Ok(Change {
                reference,
                label: seen.label,
                before: seen.value,
                after,
            })
        })();
        match outcome {
            Ok(change) => changes.push(change),
            Err(error) => {
                failed = Some((index, error));
                break;
            }
        }
    }

    let appeared = refs(cx)
        .into_iter()
        .filter(|reference| !before_run.contains(reference))
        .collect();
    Run {
        changes,
        failed,
        appeared,
    }
}

/// A committed change, kept so it can be described and undone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub label: String,
    pub before: Vec<String>,
    pub after: Vec<String>,
    pub source: Source,
}

/// A short sentence saying what an entry changed and who changed it.
#[must_use]
pub fn describe_entry(entry: &Entry) -> String {
    let list = |value: &[String]| {
        if value.is_empty() {
            "nothing".to_string()
        } else {
            value.join(", ")
        }
    };
    let who = match entry.source {
        Source::User => "the user",
        Source::Agent => "an agent",
    };
    format!(
        "{} changed from {} to {} by {who}.",
        entry.label,
        list(&entry.before),
        list(&entry.after)
    )
}

/// A tool an agent can be given to operate the interface.
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON Schema for the tool's input.
    pub input_schema: &'static str,
}

/// The tools that expose the bridge to a model.
pub const TOOLS: [Tool; 3] = [
    Tool {
        name: "snapshot",
        description: "List the interface's components with their ref, label, current value, items and the actions each accepts.",
        input_schema: r#"{"type":"object","properties":{}}"#,
    },
    Tool {
        name: "query",
        description: "Find components and items whose label matches some text. Returns refs and item keys to use with act.",
        input_schema: r#"{"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}"#,
    },
    Tool {
        name: "act",
        description: "Run steps in order on components, stopping at the first failure. Each step names a component by ref or label, one of its actions, and an item key when the action needs one.",
        input_schema: r#"{"type":"object","properties":{"steps":{"type":"array","items":{"type":"object","properties":{"target":{"type":"string"},"action":{"type":"string"},"item":{"type":"string"}},"required":["target","action"]}}},"required":["steps"]}"#,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::listbox::Listbox;
    use crate::primitive::{Item, Primitive};
    use crate::toggle::Toggle;
    use std::cell::RefCell;

    /// A stand-in for an application: named primitives the registry reaches through
    /// shared cells.
    type World = ();

    fn target<P: Primitive>(
        label: &'static str,
        primitive: P,
        access: Access,
    ) -> (Rc<RefCell<P>>, Target<World>) {
        let cell = Rc::new(RefCell::new(primitive));
        let (seen, acted, set) = (cell.clone(), cell.clone(), cell.clone());
        let target = Target {
            snapshot: Rc::new(move |_| Some(seen.borrow().snapshot(label))),
            act: Rc::new(move |_, action, item| {
                acted.borrow_mut().act(action, item, &mut |_| true)?;
                Ok(acted.borrow().value())
            }),
            set: Rc::new(move |_, value| set.borrow_mut().set_value(value)),
            access,
        };
        (cell, target)
    }

    fn fruit() -> Listbox {
        let mut fruit = Listbox::default();
        for key in ["apple", "cherry"] {
            fruit.upsert_item(Item {
                key: key.into(),
                label: key.into(),
                disabled: false,
            });
        }
        fruit
    }

    fn registry() -> Registry<World> {
        let mut registry = Registry::default();
        registry.register(target("Fruit", fruit(), Access::default()).1);
        registry.register(target("Wi-Fi", Toggle::default(), Access::default()).1);
        registry
    }

    fn step(target: &str, action: &str, item: Option<&str>) -> Step {
        Step {
            target: target.into(),
            action: action.into(),
            item: item.map(str::to_string),
        }
    }

    #[test]
    fn steps_run_in_order_by_label_or_ref_and_report_each_change() {
        let registry = registry();
        let steps = [
            step("fruit", "select", Some("cherry")),
            step("e2", "check", None),
        ];
        let run = run(|_| registry.clone(), &mut (), &steps, |_, _| true);
        assert!(run.ok());
        assert_eq!(run.changes[0].after, ["cherry"]);
        assert_eq!(
            (
                run.changes[1].label.as_str(),
                run.changes[1].before.len(),
                run.changes[1].after.as_slice()
            ),
            ("Wi-Fi", 0, ["on".to_string()].as_slice())
        );
    }

    #[test]
    fn a_run_stops_at_the_first_failure_and_says_which_step() {
        let registry = registry();
        let steps = [
            step("Fruit", "select", Some("cherry")),
            step("Fruit", "explode", None),
            step("Wi-Fi", "check", None),
        ];
        let run = run(|_| registry.clone(), &mut (), &steps, |_, _| true);
        let (index, error) = run.failed.unwrap();
        assert_eq!(index, 1);
        assert!(
            error
                .to_string()
                .contains("valid actions: select, deselect, clear")
        );
        assert_eq!(run.changes.len(), 1);
        assert!(
            registry.snapshot(&())[1].1.value.is_empty(),
            "the third step never ran"
        );
    }

    #[test]
    fn a_label_must_name_exactly_one_component() {
        let mut registry = registry();
        registry.register(target("Fruit", fruit(), Access::default()).1);
        let ambiguous = run(
            |_| registry.clone(),
            &mut (),
            &[step("Fruit", "clear", None)],
            |_, _| true,
        );
        assert_eq!(
            ambiguous.failed.unwrap().1.to_string(),
            "\"Fruit\" names several components; use a ref: e1, e3"
        );
        let missing = run(
            |_| registry.clone(),
            &mut (),
            &[step("Veg", "clear", None)],
            |_, _| true,
        );
        assert_eq!(
            missing.failed.unwrap().1.to_string(),
            "no component labelled \"Veg\""
        );
    }

    #[test]
    fn hidden_components_do_not_exist_for_an_agent() {
        let mut registry = Registry::default();
        registry.register(
            target(
                "Admin",
                Toggle::default(),
                Access {
                    hidden: true,
                    ..Default::default()
                },
            )
            .1,
        );
        assert!(registry.snapshot(&()).is_empty());
        assert_eq!(
            registry.target("e1").err(),
            Some(Error::UnknownRef("e1".into()))
        );
        assert!(registry.query(&(), "admin").is_empty());
    }

    #[test]
    fn secret_values_are_redacted_everywhere_an_agent_looks() {
        let mut registry = Registry::default();
        let (cell, secret) = target(
            "Fruit",
            fruit(),
            Access {
                secret: true,
                ..Default::default()
            },
        );
        registry.register(secret);
        let run = run(
            |_| registry.clone(),
            &mut (),
            &[step("Fruit", "select", Some("apple"))],
            |_, _| true,
        );
        assert_eq!(run.changes[0].after, ["[redacted]"]);
        assert_eq!(registry.snapshot(&())[0].1.value, ["[redacted]"]);
        assert!(!registry.snapshot(&())[0].1.items[0].selected);
        assert_eq!(
            cell.borrow().value(),
            ["apple"],
            "the change itself still happened"
        );
    }

    #[test]
    fn a_component_that_needs_approval_acts_only_when_a_person_agrees() {
        let mut registry = Registry::default();
        let confirm = Access {
            confirm: Some("Delete the account".into()),
            ..Default::default()
        };
        let (cell, guarded) = target("Delete", Toggle::default(), confirm);
        registry.register(guarded);

        let direct = registry.target("e1").unwrap().act(&mut (), "check", None);
        assert_eq!(
            direct.unwrap_err().to_string(),
            "this needs a person's approval: Delete the account"
        );

        let steps = [step("Delete", "check", None)];
        let mut asked = Vec::new();
        let refused = run(
            |_| registry.clone(),
            &mut (),
            &steps,
            |_, what| {
                asked.push(what.to_string());
                false
            },
        );
        assert_eq!(refused.failed.unwrap().1, Error::NotApproved);
        assert_eq!(asked, ["Delete the account"]);
        assert!(!cell.borrow().is_on());

        assert!(run(|_| registry.clone(), &mut (), &steps, |_, _| true).ok());
        assert!(cell.borrow().is_on());
    }

    #[test]
    fn a_search_finds_components_and_items_by_label() {
        let registry = registry();
        let found = registry.query(&(), "cher");
        assert_eq!(
            found,
            [Found {
                reference: "e1".into(),
                item: Some("cherry".into()),
                label: "cherry".into()
            }]
        );
        assert_eq!(registry.query(&(), "wi")[0].item, None);
    }

    #[test]
    fn staged_values_can_be_looked_at_and_then_put_back() {
        let registry = registry();
        run(
            |_| registry.clone(),
            &mut (),
            &[step("Fruit", "select", Some("apple"))],
            |_, _| true,
        );
        let staged = registry
            .stage(
                &mut (),
                &[
                    ("Fruit".into(), vec!["cherry".into()]),
                    ("Wi-Fi".into(), vec!["on".into()]),
                ],
            )
            .unwrap();
        let values = |registry: &Registry<World>| -> Vec<Vec<String>> {
            registry
                .snapshot(&())
                .into_iter()
                .map(|(_, s)| s.value)
                .collect()
        };
        assert_eq!(
            values(&registry),
            [vec!["cherry".to_string()], vec!["on".to_string()]]
        );
        registry.revert(&mut (), staged);
        assert_eq!(values(&registry), [vec!["apple".to_string()], vec![]]);
    }

    #[test]
    fn a_step_that_mounts_a_component_reports_it_as_appeared() {
        let shared = Rc::new(RefCell::new(registry()));
        let opener = shared.clone();
        let dialog = Target {
            snapshot: Rc::new(|_: &World| Some(Toggle::default().snapshot("Opener"))),
            act: Rc::new(move |_, _, _| {
                opener
                    .borrow_mut()
                    .register(target("Dialog", Toggle::default(), Access::default()).1);
                Ok(Vec::new())
            }),
            set: Rc::new(|_, _| {}),
            access: Access::default(),
        };
        shared.borrow_mut().register(dialog);
        let run = run(
            |_| shared.borrow().clone(),
            &mut (),
            &[step("Opener", "toggle", None)],
            |_, _| true,
        );
        assert_eq!(run.appeared, ["e4"]);
    }

    #[test]
    fn an_entry_reads_as_a_sentence() {
        let entry = Entry {
            label: "Volume".into(),
            before: vec!["30".into()],
            after: vec!["70".into()],
            source: Source::Agent,
        };
        assert_eq!(
            describe_entry(&entry),
            "Volume changed from 30 to 70 by an agent."
        );
        let cleared = Entry {
            label: "Fruit".into(),
            before: vec!["apple".into()],
            after: vec![],
            source: Source::User,
        };
        assert_eq!(
            describe_entry(&cleared),
            "Fruit changed from apple to nothing by the user."
        );
    }

    #[test]
    fn tool_schemas_are_objects_a_model_can_be_given() {
        for tool in &TOOLS {
            assert!(
                tool.input_schema.starts_with(r#"{"type":"object""#),
                "{}",
                tool.name
            );
            assert!(!tool.description.is_empty());
        }
    }
}
