# Render an existing primitive

## Choose the primitive

| Primitive | Module | Parts | Value |
| --- | --- | --- | --- |
| Listbox | `armature_core::listbox` | `Root`, `Option` | Selected keys. Single or multiple (`Options::mode`). |
| Tabs | `armature_core::tabs` | `List`, `Tab`, `Panel` | The current tab's key. |
| Select | `armature_core::select` | `Trigger`, `Content`, `Option` | The chosen key. `Content` is a popup. |

A part marked as an item part (`Primitive::is_item`) is rendered once per item and takes
that item's key. `Tabs` panels also take the key of the tab they belong to.

## Leptos

`Root` owns the state. Every `Part` inside it infers the primitive from the part enum.

```rust
use armature_core::select::{self, Select};
use armature_leptos::{Part, Root, ValueChange};

let on_change = Callback::new(move |change: ValueChange| {
    size.set(change.value);
    true // return false to cancel the change
});

view! {
    <Root machine=Select::default() label="Size" on_value_change=on_change>
        <Part part=select::Part::Trigger>{move || size.get().join("")}</Part>
        <Part part=select::Part::Content>
            <Part part=select::Part::Option item="small">"Small"</Part>
            <Part part=select::Part::Option item="large" disabled=true>"Large"</Part>
        </Part>
    </Root>
}
```

- Each `Part` renders one unstyled `div`.
- An item's label defaults to its rendered text. Pass `label` when the text is not the
  name, for example when the item holds an icon.
- Items are ordered by their position in the document, so conditional and reordered
  items need no extra work.
- A popup part is positioned for you with `position: fixed`. It follows its anchor on
  scroll and resize, and `data-side` reports `top` or `bottom` after a flip.
- Call `armature_leptos::agent::install()` once at startup to expose the agent bridge.

## GPUI

`Machine<P>` is an entity that owns the state. Pass the items when you create it.
`part(..)` returns an element you style and fill like a `div`.

```rust
use armature_core::primitive::Item;
use armature_core::select::{Part, Select};
use armature_gpui::{Machine, part};

let items = ["Small", "Large"].map(|key| Item { key: key.into(), label: key.into(), disabled: false });
let size = cx.new(|cx| Machine::new(Select::default(), "Size", items, cx));

// in render:
let option = |key| part(&size, Part::Option, Some(key), cx).px_2().child(key);
div()
    .child(part(&size, Part::Trigger, None, cx).border_1().child("Size"))
    .child(part(&size, Part::Content, None, cx).children(["Small", "Large"].map(option)))
```

- A hidden part renders nothing. A popup part paints above the window beside its
  anchor, and GPUI flips it when it would overflow.
- Use `machine.on_value_change(|change, cx| ..)` for the change gate.
- Tab traversal is the application's job in GPUI. Bind `tab` and `shift-tab` to actions
  that call `window.focus_next(cx)` and `window.focus_prev(cx)`, as `examples/gpui-demo`
  does. armature marks which part is the tab stop.
- Items are fixed when the `Machine` is created.
