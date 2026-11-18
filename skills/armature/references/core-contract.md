# The core contract

Everything a primitive takes and returns, with the paths to import. Use this to drive
a primitive without a UI, as in a test or a new adapter. `crates/core/src/primitive.rs`
is the full definition.

## Imports

```rust
use armature_core::change::{Gate, Reason, Source, ValueChange};
use armature_core::keys::{Key, Modifiers};
use armature_core::primitive::{Input, Item, Primitive, Reaction};
use armature_core::agent::ActError;
use armature_core::tabs::{self, Tabs}; // one module per primitive: its type, `Part`, `Options`
```

Each primitive's type is its module name in CamelCase: `accordion::Accordion`,
`number_input::NumberInput`, `toggle::Toggle`, `listbox::Listbox`. Types used in
options and constructors live elsewhere:

```rust
use armature_core::number::NumberRange;      // NumberRange::new(min, max, step), all f64
use armature_core::keys::{Dir, Orientation}; // Dir::{Ltr, Rtl}; Orientation::{Horizontal, Vertical}
use armature_core::selection::Mode;          // Mode::{Single, Multiple}, for a listbox
use armature_core::calendar::Selecting;      // Selecting::{Single, Multiple, Range}
use armature_core::date::Date;               // Date::new(year, month, day) -> Option<Date>
```

`Source`, `Reason`, `Key`, `Modifiers` and every `Part` enum are `Copy`, `Debug` and
`PartialEq`. `ValueChange`, `ActError`, `FocusTarget` and `Item` are `Debug` and
`PartialEq`, so `assert_eq!` works on all of them.

## Making one

Every primitive implements `Default`. Most also have `new(Options)`, where `Options`
is in the primitive's module and implements `Default`:

```rust
let mut tabs = Tabs::default();
let mut manual = Tabs::new(tabs::Options { manual: true, ..Default::default() });
```

Primitives with items get them one at a time, in order:

```rust
for key in ["one", "two", "three"] {
    tabs.upsert_item(Item { key: key.into(), label: key.into(), disabled: false });
}
```

`remove_item(key)` takes one away and `set_order(&[(key, label)])` reorders them. In a
UI the adapter does this from the parts you render. A fresh `Tabs` has its first
enabled tab selected.

## Options

Each `Options` implements `Default`; set only what you need with `..Default::default()`.
Primitives not listed take their settings through `new(...)` arguments or builder methods.

| Module | `Options` fields |
| --- | --- |
| `accordion` | `open: selection::Choosing`, `findable: bool` |
| `calendar` | `week_start: u8`, `dir: Dir`, `min: Option<Date>`, `max: Option<Date>`, `mode: Selecting`, `unavailable: Option<fn(Date) -> bool>`, `deselectable: bool`, `months: usize` |
| `carousel` | `ends: roving::Ends`, `autoplay_ms: Option<u64>`, `dir: Dir`, `per_page: usize`, `orientation: crate::keys::Orientation` |
| `combobox` | `count: selection::Count`, `takes: Takes` (`Options`, `AnyText`), `auto_highlight: bool`, `open_on_focus: bool`, `load_more: bool`, `results: Results` (`Filtered`, `Requested`), `read_only: bool` |
| `dialog` | `modality: Modality` (`Modeless`, `Modal`, `Alert`), `closes: Closes` (`Never`, `By(layers::DismissedBy)`) |
| `event_list` | `orientation: Orientation`, `dir: Dir`, `ends: roving::Ends`, `feed: bool` |
| `file_input` | `count: selection::Count`, `accept: Vec<String>`, `max_size: Option<u64>` |
| `listbox` | `mode: Mode`, `orientation: Orientation`, `dir: Dir`, `ends: roving::Ends` |
| `navigation_menu` | `hover: bool`, `open_delay_ms: u64`, `close_delay_ms: u64`, `orientation: Orientation`, `dir: Dir` |
| `panel_group` | `orientation: Orientation`, `dir: Dir`, `min: f64`, `step: f64`, `large_step: f64` |
| `range_slider` | `range: NumberRange`, `orientation: Orientation`, `dir: Dir`, `large_step: f64`, `min_steps_between: f64` |
| `rating` | `max: u32`, `halves: bool`, `deselectable: bool`, `dir: Dir`, `read_only: bool` |
| `resizable` | `orientation: Orientation`, `dir: Dir`, `min: f64`, `max: f64` |
| `slider` | `range: NumberRange`, `orientation: Orientation`, `dir: Dir`, `large_step: f64` |
| `tabs` | `orientation: Orientation`, `dir: Dir`, `manual: bool`, `ends: roving::Ends` |
| `timeline` | `range: Span`, `grid: f64`, `min_length: f64` |
| `toggle_group` | `pressed: selection::Choosing`, `orientation: Orientation`, `dir: Dir` |
| `tooltip` | `open_delay_ms: u64`, `close_delay_ms: u64`, `card: bool`, `disabled: bool` |

`selection::Choosing` is `One`, `OneOrNone`, `AtLeastOne` or `Any`: how many may be
open or pressed, and whether the last one can be let go. `selection::Count` is `One` or
`Many { max }`. An accordion opens `Choosing::One` by default: its open section stays
open, so `toggle` and `close` on it change nothing. Use `OneOrNone` to allow closing.
An action that changes nothing returns `Ok(())` without asking the gate.

## Sending input

```rust
fn send(&mut self, input: Input<Part>, gate: Gate) -> Reaction<Part>;
```

`Input` is an enum of struct variants. The common ones:

```rust
Input::Key { key: Key::ArrowRight, mods: Modifiers::default(), now_ms: 0 }
Input::Press { part: tabs::Part::Tab, item: Some("two".into()), mods: Modifiers::default() }
Input::Text { part: combobox::Part::Input, item: None, value: "ap".into() }
Input::Focus { part, item }      Input::Blur { part, item }
Input::Hover { part, item, inside: true, now_ms }
Input::Tick { now_ms }           Input::PressOutside        Input::WindowBlur
```

`Key` has `ArrowUp/Down/Left/Right`, `Home`, `End`, `PageUp`, `PageDown`, `Enter`,
`Space`, `Escape`, `Tab`, `Backspace`, `F2`, `F6` and `Char(char)`.
`Modifiers { shift, ctrl, alt }`: `ctrl` is Cmd on macOS. `now_ms` is any clock in
milliseconds; tests pass made-up times.

Keys go to whatever the primitive last heard was focused, so send `Input::Focus` first
when a key should act on a particular part.

## The gate

`Gate<'a>` is `&'a mut dyn FnMut(&ValueChange) -> bool`. The primitive offers every
change of value to it before committing. Return `false` to cancel.

```rust
let mut seen = Vec::new();
tabs.send(input, &mut |change: &ValueChange| {
    seen.push((change.value.clone(), change.source, change.reason));
    true
});
```

`ValueChange { value: Vec<String>, source: Source, reason: Reason }`. `Source` is `User`
or `Agent`. `Reason` is `Press`, `Keyboard`, `Drag`, `Text`, `Edit`, `Action`, `Timer`
or `Hover`.

## What comes back

```rust
pub struct Reaction<Part> {
    pub handled: bool,                       // suppress the platform's default
    pub focus: Option<FocusTarget<Part>>,    // FocusTarget { part, item: Option<String> }
    pub effects: Vec<Effect>,                // Announce, Wake, CapturePointer, ScrollTo, Copy, ...
}
```

`reaction.focus_item()` is the item key focus should move to, if any.

## Reading state

```rust
tabs.value()                                  // Vec<String>, the same encoding the gate sees
tabs.set_value(vec!["two".into()])            // the owner's own change; no gate
tabs.semantics(tabs::Part::Panel, Some("one")) // role, hidden, expanded, disabled, focusable, ...
tabs.snapshot("Sections")                     // what an agent sees
```

Popups have `is_open()`. A combobox has `results()` for the options that match what
was typed, and `text(Part::Input, None)` for the text in its field.

## Agent actions

```rust
fn act(&mut self, action: &str, item: Option<&str>, gate: Gate) -> Result<(), ActError>;
```

The change is offered to the gate with `Source::Agent`. `ActError` is `UnknownAction {
action, valid }`, `MissingItem { action }`, `UnknownItem(key)`, `Disabled(key)` or
`Refused` when the gate said no. Each primitive's actions are the `ACTIONS` constant at
the top of its file, and `snapshot().actions`. Those used most:

| Primitive | Actions |
| --- | --- |
| accordion, collapsible | `open`, `close`, `toggle` (accordion also `open_all`, `close_all`) |
| checkbox, toggle | `toggle`, `check`, `uncheck` |
| combobox | `select`, `remove`, `clear`, `type`, `open`, `close` |
| dialog, popover | `open`, `close` |
| listbox | `select`, `deselect`, `clear` |
| menu, menubar | `open`, `close`, `run`, `toggle` |
| number_input, slider | `set`, `increment`, `decrement` |
| radio, tabs, picker | `select` |
| select | `open`, `close`, `select`, `clear` |
| toast | `dismiss`, `action`, `dismiss_all` |
| tooltip | `show`, `hide` |
| tree_view | `select`, `expand`, `collapse` |

Actions that name something take it as `item`: the key for `select`, `open`, `close`,
`toggle`, `remove`, `expand`; the new value as text for `set` (`act("set", Some("70"), ..)`)
and `type`. `clear`, `increment`, `decrement`, `show`, `hide` and a dialog's or popover's
`open` and `close` take `None`. `snapshot().name` is a short name that can differ from
the module, such as `"number"` for `number_input`.

## Details that are easy to guess wrong

- **Number input.** Its value is one string written as Rust prints an `f64`: `["7"]`,
  `["0.3"]`. `NumberInput::new(NumberRange::new(min, max, step), value)` sets the range;
  the default is 0 to 100 by 1. Typed text arrives as `Input::Text` and is committed by
  Enter, Tab or `Input::Blur`, clamped to the range, with `Reason::Text`. Arrow keys step
  with `Reason::Keyboard`.
- **Tooltip.** It opens 500 ms after `Input::Hover { inside: true, .. }` (the
  `open_delay_ms` option), when it next hears `Input::Tick` at or past that time. The
  hover returns `Effect::Wake { at_ms }` saying when to tick. Focus opens it at once.
  Escape hides it whether or not anything was focused. It has `is_open()`.
- **Lists.** In a listbox, tabs, radio group or menu, arrows skip disabled items, a
  press on one does nothing, and `act("select", Some(key))` on one is
  `Err(ActError::Disabled(key))`. Send `Input::Focus` to the item part with the item's
  key. In a single-choice listbox arrows only move focus; Enter, Space or a press
  chooses.
- **Dialog.** Options default to `Modality::Modal` and `Closes::By(DismissedBy::EITHER)`.
  `Closes::Never` refuses Escape, outside presses and Back; `DismissedBy` picks which of
  Escape and an outside press close it. `Modality::Alert` is modal, and an outside press
  never closes it. The Close part, the trigger and agent actions always close.
  Escape returns focus to `Part::Trigger`.
- **Accordion.** A fresh one has nothing open.
- **Agent actions.** The gate sees `Source::Agent` and `Reason::Action` for every one.
- **Keys and focus.** Keys that act on one part or item (Enter on a button, arrows in
  a list) need `Input::Focus` first. Keys that mean the whole primitive, such as
  Escape on a popup, do not.
- **Traits.** `upsert_item`, `send`, `act`, `value`, `set_value`, `semantics` and
  `snapshot` are `Primitive` methods, so import the trait. `is_open()`, `results()` and
  the like are the type's own.

## A whole test

```rust
#[test]
fn a_refused_change_keeps_the_tab() {
    let mut tabs = Tabs::default();
    for key in ["one", "two", "three"] {
        tabs.upsert_item(Item { key: key.into(), label: key.into(), disabled: false });
    }
    let right = || Input::Key { key: Key::ArrowRight, mods: Modifiers::default(), now_ms: 0 };
    let moved = tabs.send(right(), &mut |_| true);
    assert_eq!(tabs.value(), ["two"]);
    assert_eq!(moved.focus_item(), Some("two"));
    tabs.send(right(), &mut |_| false);
    assert_eq!(tabs.value(), ["two"]);
}
```
