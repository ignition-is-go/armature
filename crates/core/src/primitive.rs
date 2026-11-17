//! The contract between a primitive and a framework adapter. A primitive is defined
//! once against these types; an adapter is written once against the `Primitive`
//! trait and can then render every primitive.

use std::fmt::Debug;
use std::hash::Hash;

use crate::agent::{ActError, Snapshot};
use crate::change::Gate;
use crate::keys::{Key, Modifiers, Orientation};
use crate::popup::Placement;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub key: String,
    /// Text matched by typeahead and shown to agents.
    pub label: String,
    pub disabled: bool,
}

/// What the adapter reports to the primitive.
#[derive(Clone, Debug)]
pub enum Input<Part> {
    Key {
        key: Key,
        mods: Modifiers,
        now_ms: u64,
    },
    Press {
        part: Part,
        item: Option<String>,
        mods: Modifiers,
    },
    /// Focus landed on a part by some route other than a `Reaction`.
    Focus { part: Part, item: Option<String> },
    /// A press landed outside every part of this primitive.
    PressOutside,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FocusTarget<Part> {
    pub part: Part,
    pub item: Option<String>,
}

/// What the adapter must do after an input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reaction<Part> {
    /// The input was consumed; suppress the platform's default for it.
    pub handled: bool,
    pub focus: Option<FocusTarget<Part>>,
}

impl<Part> Default for Reaction<Part> {
    fn default() -> Self {
        Reaction {
            handled: false,
            focus: None,
        }
    }
}

impl<Part> Reaction<Part> {
    pub fn handled() -> Self {
        Reaction {
            handled: true,
            focus: None,
        }
    }

    /// A reaction that may move focus to an item of `part`.
    pub fn item(part: Part, handled: bool, focus: Option<String>) -> Self {
        Reaction {
            handled,
            focus: focus.map(|item| FocusTarget {
                part,
                item: Some(item),
            }),
        }
    }

    pub fn focusing(part: Part) -> Self {
        Reaction {
            handled: true,
            focus: Some(FocusTarget { part, item: None }),
        }
    }

    pub fn focus_item(&self) -> Option<&str> {
        self.focus.as_ref()?.item.as_deref()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Listbox,
    Option,
    TabList,
    Tab,
    TabPanel,
    /// A control that opens a popup to choose a value.
    ComboBox,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Focusable {
    #[default]
    No,
    /// Focusable by pointer or by a `Reaction`, skipped by Tab.
    Programmatic,
    TabStop,
}

/// Renderer-neutral description of one rendered part. A web adapter maps it to ARIA
/// attributes, a native one to its accessibility tree. A part that is focusable also
/// reports presses and keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Semantics {
    pub role: Option<Role>,
    pub selected: Option<bool>,
    pub expanded: Option<bool>,
    pub multiselectable: Option<bool>,
    pub orientation: Option<Orientation>,
    pub disabled: bool,
    pub hidden: bool,
    pub focusable: Focusable,
    /// The item the user is currently on, for styling.
    pub active: bool,
}

/// Names the primitive a part belongs to, so adapters can infer it from the part.
pub trait PartOf: Copy + Debug + Eq + Hash + Send + Sync + 'static {
    type Primitive: Primitive<Part = Self>;
}

pub trait Primitive: Clone + 'static {
    /// The parts a consumer renders, such as a listbox's root and options.
    type Part: PartOf<Primitive = Self>;

    fn parts() -> &'static [Self::Part];

    /// Whether a part is rendered once per item and registers that item.
    fn is_item(part: Self::Part) -> bool;

    /// Where a part floats, if it is a popup. The adapter measures and positions it.
    fn popup_of(_part: Self::Part) -> Option<Placement<Self::Part>> {
        None
    }

    fn upsert_item(&mut self, item: Item);
    fn remove_item(&mut self, key: &str);
    /// Aligns items with the rendered order. `rendered` is `(key, text)`; the text
    /// fills in the label of any item registered without one.
    fn set_order(&mut self, rendered: &[(String, String)]);

    fn semantics(&self, part: Self::Part, item: Option<&str>) -> Semantics;
    fn send(&mut self, input: Input<Self::Part>, gate: Gate) -> Reaction<Self::Part>;

    fn value(&self) -> Vec<String>;
    fn snapshot(&self, label: &str) -> Snapshot;
    /// Runs an agent action through the same change gate a user's input goes through.
    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError>;
}
