use std::fmt;
use std::rc::Rc;

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
            ActError::UnknownAction { action, valid } => {
                write!(
                    f,
                    "unknown action \"{action}\"; valid actions: {}",
                    valid.join(", ")
                )
            }
            ActError::MissingItem { action } => write!(f, "action \"{action}\" needs an item"),
            ActError::UnknownItem(key) => write!(f, "no item \"{key}\""),
            ActError::Disabled(key) => write!(f, "item \"{key}\" is disabled"),
            ActError::Refused => write!(f, "the application refused this change"),
        }
    }
}

impl std::error::Error for ActError {}

/// How a registry reaches one mounted component. `Cx` is whatever the framework
/// needs to read and write state, such as `()` for signals or GPUI's `App`.
type SnapshotFn<Cx> = dyn Fn(&Cx) -> Option<Snapshot>;
type ActFn<Cx> = dyn Fn(&mut Cx, &str, Option<&str>) -> Result<Vec<String>, ActError>;

pub struct Target<Cx> {
    /// `None` once the component is gone.
    pub snapshot: Rc<SnapshotFn<Cx>>,
    /// Runs one action and returns the value afterwards.
    pub act: Rc<ActFn<Cx>>,
}

impl<Cx> Clone for Target<Cx> {
    fn clone(&self) -> Self {
        Target {
            snapshot: self.snapshot.clone(),
            act: self.act.clone(),
        }
    }
}

impl<Cx> Target<Cx> {
    pub fn act(&self, cx: &mut Cx, action: &str, item: Option<&str>) -> Result<Vec<String>, Error> {
        (self.act)(cx, action, item).map_err(Error::Act)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    UnknownRef(String),
    Act(ActError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UnknownRef(r) => write!(f, "no component with ref \"{r}\""),
            Error::Act(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

/// Every mounted component, addressable by an agent through a stable ref.
pub struct Registry<Cx> {
    next_id: u32,
    targets: Vec<(u32, Target<Cx>)>,
}

impl<Cx> Default for Registry<Cx> {
    fn default() -> Self {
        Registry {
            next_id: 1,
            targets: Vec::new(),
        }
    }
}

impl<Cx> Registry<Cx> {
    pub fn register(&mut self, target: Target<Cx>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.targets.push((id, target));
        id
    }

    pub fn unregister(&mut self, id: u32) {
        self.targets.retain(|(i, _)| *i != id);
    }

    pub fn snapshot(&self, cx: &Cx) -> Vec<(String, Snapshot)> {
        self.targets
            .iter()
            .filter_map(|(id, target)| Some((format!("e{id}"), (target.snapshot)(cx)?)))
            .collect()
    }

    /// The target behind a ref, cloned out so the registry is not borrowed while
    /// application code runs.
    pub fn target(&self, reference: &str) -> Result<Target<Cx>, Error> {
        self.targets
            .iter()
            .find(|(id, _)| format!("e{id}") == reference)
            .map(|(_, target)| target.clone())
            .ok_or_else(|| Error::UnknownRef(reference.to_string()))
    }
}
