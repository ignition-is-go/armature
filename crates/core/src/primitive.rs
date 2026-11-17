//! The contract between a primitive and a framework adapter. A primitive is defined
//! once against these types; an adapter is written once against the `Primitive`
//! trait and can then render every primitive.

use std::fmt::Debug;
use std::hash::Hash;

use crate::agent::{ActError, Snapshot};
use crate::change::Gate;
use crate::geometry::{Point, Rect};
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
    /// Focus left a part for somewhere outside the primitive.
    Blur { part: Part, item: Option<String> },
    /// The platform's find-in-page matched text inside a hidden part.
    Found { part: Part, item: Option<String> },
    /// A picture or other media inside a part finished loading, or failed to.
    Loaded { part: Part, ok: bool },
    /// A press landed outside every part of this primitive.
    PressOutside,
    /// The window stopped being the one the person is working in.
    WindowBlur,
    /// The time asked for by `Effect::Wake` has arrived.
    Tick { now_ms: u64 },
    /// The pointer came to rest over a part, or left it. Touch never hovers.
    Hover {
        part: Part,
        item: Option<String>,
        inside: bool,
        now_ms: u64,
    },
    /// A scrolling part moved, or changed size. Both are lengths along its axis.
    Scrolled {
        part: Part,
        offset: f64,
        viewport: f64,
        now_ms: u64,
    },
    /// The person pasted text while focus was inside the primitive.
    Paste { text: String },
    /// The person edited a text part.
    Text {
        part: Part,
        item: Option<String>,
        value: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerPhase {
    Down,
    Move,
    Up,
    /// The platform took the pointer away.
    Cancel,
}

/// A pointer gesture on a part that tracks the pointer.
#[derive(Clone, Debug)]
pub struct Pointer<Part> {
    pub phase: PointerPhase,
    pub part: Part,
    pub item: Option<String>,
    /// In the same space as the boxes `Measure` reports.
    pub at: Point,
    pub mods: Modifiers,
    pub now_ms: u64,
}

/// Reports where a part is right now, if it is rendered.
pub type Measure<'a, Part> = &'a dyn Fn(Part, Option<&str>) -> Option<Rect>;

/// Something the adapter must do beyond moving focus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Read a message to assistive technology.
    Announce { message: String, assertive: bool },
    /// Send `Input::Tick` at this time.
    Wake { at_ms: u64 },
    /// Keep sending this pointer's moves here until it is released, wherever it goes.
    CapturePointer,
    /// Scroll the primitive's scrolling part to this offset.
    ScrollTo { offset: u32 },
    /// Put this text on the clipboard.
    Copy { text: String },
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
    pub effects: Vec<Effect>,
}

impl<Part> Default for Reaction<Part> {
    fn default() -> Self {
        Reaction {
            handled: false,
            focus: None,
            effects: Vec::new(),
        }
    }
}

impl<Part> Reaction<Part> {
    pub fn handled() -> Self {
        Reaction {
            handled: true,
            focus: None,
            effects: Vec::new(),
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
            effects: Vec::new(),
        }
    }

    pub fn focusing(part: Part) -> Self {
        Reaction {
            handled: true,
            focus: Some(FocusTarget { part, item: None }),
            effects: Vec::new(),
        }
    }

    pub fn with(mut self, effect: Effect) -> Self {
        self.effects.push(effect);
        self
    }

    pub fn announcing(self, message: impl Into<String>) -> Self {
        self.with(Effect::Announce {
            message: message.into(),
            assertive: false,
        })
    }

    /// Announces at once, interrupting what is being read. For something the person
    /// is in the middle of doing, such as moving an item.
    pub fn alerting(self, message: impl Into<String>) -> Self {
        self.with(Effect::Announce {
            message: message.into(),
            assertive: true,
        })
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
    Button,
    Switch,
    RadioGroup,
    Radio,
    Toolbar,
    Slider,
    SpinButton,
    TextBox,
    Tree,
    TreeItem,
    /// A set of related parts, such as the children of a tree item.
    Group,
    /// A titled section of content, such as an accordion panel.
    Region,
    Grid,
    GridCell,
    Dialog,
    List,
    ListItem,
    /// A draggable divider between two panels.
    Separator,
    Tooltip,
    Checkbox,
    Menu,
    MenuItem,
    MenuItemCheckbox,
    MenuItemRadio,
    MenuBar,
    /// A reading within a known range, such as disk space used.
    Meter,
    /// How far a task has come.
    ProgressBar,
    /// A set of links for getting around.
    Navigation,
    Link,
    Image,
    /// Text that changes and is read out when there is a pause.
    Status,
    /// Text that is read out at once.
    Alert,
    ScrollBar,
}

/// A number within limits, as a slider or stepper reports it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ValueRange {
    pub now: f64,
    pub min: f64,
    pub max: f64,
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
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Semantics {
    pub role: Option<Role>,
    pub selected: Option<bool>,
    pub expanded: Option<bool>,
    pub checked: Option<bool>,
    /// Checked in part, as a checkbox over a group with only some checked is.
    pub mixed: bool,
    /// Whether a button that stays down is down.
    pub pressed: Option<bool>,
    /// The one among its siblings that shows where the person is, such as the
    /// current page.
    pub current: bool,
    /// Its value was refused.
    pub invalid: bool,
    /// Its text must not be shown as typed.
    pub obscured: bool,
    /// Depth in a tree, starting at 1.
    pub level: Option<u32>,
    pub range: Option<ValueRange>,
    pub multiselectable: Option<bool>,
    pub orientation: Option<Orientation>,
    pub disabled: bool,
    pub hidden: bool,
    /// While hidden, the platform's find-in-page may still search this part, and
    /// reports a match as `Input::Found`.
    pub findable: bool,
    /// Keeps Tab inside this part while it is shown, as a modal dialog must.
    pub modal: bool,
    pub focusable: Focusable,
    /// The item the user is currently on, for styling.
    pub active: bool,
}

/// Links from one part to others.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Relations<Part> {
    /// The part whose text names this one.
    pub labelled_by: Option<Part>,
    /// The part this one shows, hides or changes.
    pub controls: Option<Part>,
    /// The part whose text explains this one.
    pub described_by: Option<Part>,
}

impl<Part> Default for Relations<Part> {
    fn default() -> Self {
        Relations {
            labelled_by: None,
            controls: None,
            described_by: None,
        }
    }
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

    /// Whether the adapter should report the pointer hovering over a part.
    fn hovers(_part: Self::Part) -> bool {
        false
    }

    /// Whether a part scrolls, so the adapter reports its offset and size and carries
    /// out `Effect::ScrollTo` on it.
    fn scrolls(_part: Self::Part) -> bool {
        false
    }

    /// How soon, in milliseconds, the primitive wants a `Tick` that nothing else will
    /// bring. A value set from outside starts no timer by itself, so the adapter asks
    /// this after every change and sends the tick.
    fn wake_after(&self) -> Option<u64> {
        None
    }

    /// Where the primitive wants its scrolling part scrolled to. The adapter brings the
    /// part there after an agent action, which has no other way to ask.
    fn wanted_scroll(&self) -> Option<f64> {
        None
    }

    /// How a part relates to others, for assistive technology. A relation between
    /// parts that take an item joins the two that share an item.
    fn relations(_part: Self::Part) -> Relations<Self::Part> {
        Relations::default()
    }

    /// Where a part floats, if it is a popup. The adapter measures and positions it.
    fn popup_of(_part: Self::Part) -> Option<Placement<Self::Part>> {
        None
    }

    /// Whether presses on a part are gestures to follow with `pointer`, such as a
    /// slider thumb, and not plain presses.
    fn tracks_pointer(_part: Self::Part) -> bool {
        false
    }

    /// The part that carries the component's accessible name: the first part with a
    /// role that is not repeated per item.
    fn named_part(&self) -> Option<Self::Part> {
        let named =
            |part: &Self::Part| !Self::is_item(*part) && self.semantics(*part, None).role.is_some();
        Self::parts().iter().copied().find(named)
    }

    fn upsert_item(&mut self, item: Item);
    fn remove_item(&mut self, key: &str);
    /// Aligns items with the rendered order. `rendered` is `(key, text)`; the text
    /// fills in the label of any item registered without one.
    fn set_order(&mut self, rendered: &[(String, String)]);

    fn semantics(&self, part: Self::Part, item: Option<&str>) -> Semantics;
    fn send(&mut self, input: Input<Self::Part>, gate: Gate) -> Reaction<Self::Part>;

    /// Handles a gesture on a part that tracks the pointer.
    fn pointer(
        &mut self,
        _pointer: Pointer<Self::Part>,
        _measure: Measure<Self::Part>,
        _gate: Gate,
    ) -> Reaction<Self::Part> {
        Reaction::default()
    }

    /// The text to show in a text-entry part.
    fn text(&self, _part: Self::Part, _item: Option<&str>) -> Option<String> {
        None
    }

    fn value(&self) -> Vec<String>;
    /// Sets the value on the owner's say-so, without asking the gate. Keys that are
    /// not valid choices are dropped.
    fn set_value(&mut self, value: Vec<String>);
    fn snapshot(&self, label: &str) -> Snapshot;
    /// Runs an agent action through the same change gate a user's input goes through.
    fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError>;
}
