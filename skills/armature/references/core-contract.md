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
