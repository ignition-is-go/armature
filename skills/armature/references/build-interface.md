# Render an existing primitive

## Choose the primitive

Each lives in `armature_core::<module>` and exports a `Part` enum.

| Primitive | Module | Parts | Value |
| --- | --- | --- | --- |
| Listbox | `listbox` | `Root`, `Option` | Selected keys. Single or multiple. |
| Select | `select` | `Trigger`, `Content`, `Option` | The chosen key. `Content` is a popup. |
| Command palette | `command` | `Input`, `List`, `Option` | The option that was run. |
| Tabs | `tabs` | `List`, `Tab`, `Panel` | The current tab. |
| Accordion | `accordion` | `Trigger`, `Panel` | The open sections. |
| Radio group, segmented control, rating | `radio` | `Root`, `Option` | The chosen option. |
| Switch | `toggle` | `Root` | `["on"]` or empty. |
| Toolbar | `toolbar` | `Root`, `Button` | None. A press is offered to the gate. |
| Slider | `slider` | `Track`, `Thumb` | The number. |
| Number input | `number_input` | `Input`, `Increment`, `Decrement` | The number. |
| One-time code | `otp_input` | `Input` | The digits. Draw cells from `cells()`. |
| Token input | `token_input` | `Root`, `Input`, `Token` | The tokens. |
| Tree | `tree_view` | `Root`, `Node` | The selected node. Render every node. |
| Sortable list | `sortable` | `Root`, `Item` | The keys in order. |
| Calendar | `calendar` | `Grid`, `Day`, `PreviousMonth`, `NextMonth` | The day as `YYYY-MM-DD`. Draw days from `weeks()`. |
| Resizable panels | `resizable` | `Root`, `Handle` | The first panel's share, in percent. |
| Dialog | `dialog` | `Trigger`, `Backdrop`, `Content`, `Close` | `["open"]` or empty. |
| Popover | `popover` | `Trigger`, `Content`, `Title`, `Description`, `Arrow`, `Close` | None. `Content` is a popup. |
| Tooltip | `tooltip` | `Trigger`, `Content` | None. Shows on hover after a delay, and on focus. |
| Sheet | `sheet` | `Trigger`, `Backdrop`, `Content`, `Handle` | The index of the height it rests at. Size it to `height()`. |
| Colour picker | `color_picker` | `Area`, `AreaThumb`, `Hue`, `HueThumb`, `Input` | The colour as `#rrggbb`. |
| Swipe row | `swipe_row` | `Row`, `Action` | None. A pressed action is offered to the gate. Move the row by `offset()`. |
| Virtual list | `virtual_list` | `Viewport`, `Row` | The selected row's index. Draw rows from `rows()`. |
| Timeline | `timeline` | `Track`, `Span`, `SpanEnd` | Each span as `key:start-end`. |
| Data grid | `data_grid` | `Root`, `Cell` | The selected cells as `row,column`. Edits arrive with `Reason::Edit`. |
| Canvas | `canvas` | `Surface`, `Node`, `Resize`, `PortOut`, `PortIn` | Each node as `key:x,y,width,height`. |

A part marked as an item part (`Primitive::is_item`) is rendered once per item and takes
that item's key. Some other parts also take a key: a tab's panel, an accordion's panel,
a calendar's day, a token.

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

- Each `Part` renders one unstyled `div`. A part the person types into renders an
  `input`.
- An item's label defaults to its rendered text. Pass `label` when the text is not the
  name. On a part that is not an item, `label` is that part's own accessible name.
- Items are ordered by their position in the document.
- Pass `value` to `Root` to control the value from a signal.
- Inside a `Root`, call `use_primitive::<P>()` to read what only the primitive knows,
  such as `Calendar::weeks`, `OtpInput::cells` or `Resizable::percent`.
- A popup part is positioned for you and follows its anchor on scroll and resize.
  `data-side` reports `top` or `bottom` after a flip.
- A modal part keeps Tab inside itself while it is shown.
- A popup's arrow is placed for you through the `--armature-arrow-x` custom property
  on the popup.
- A part that scrolls reports its offset, and is scrolled when the primitive asks.
- Copy and paste reach a primitive that handles them through the clipboard.
- `use_visible_viewport()` follows the part of the window an on-screen keyboard leaves
  showing, and `keyboard_lift` says how far to lift a field clear of it.
- `LocalStorage` implements `armature_core::preference::Storage`.
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
- Use `machine.on_value_change(|change, cx| ..)` for the change gate and
  `machine.set_value(value, cx)` to set the value yourself.
- Read `machine.primitive()` for what only the primitive knows, and
  `machine.text(part, item)` for the text of a part the person types into. GPUI has no
  text field element, so you draw that text yourself, with the caret at
  `machine.caret(part, item)`. Typing, Backspace, Delete, the arrows, Home and End all
  edit at the caret.
- For parts whose items appear over time, such as calendar days or tokens, call
  `machine.track(part, keys, cx)` before rendering them.
- Subscribe to the `Machine` for `Announcement` events and deliver them to assistive
  technology yourself. GPUI has no live regions.
- Tab traversal is the application's job. Bind `tab` and `shift-tab` to actions that
  call `window.focus_next(cx)` and `window.focus_prev(cx)`, as `examples/gpui-demo`
  does. While a modal part is shown, Tab cycles among that primitive's own parts. Other
  focusable elements you put inside it are not part of the cycle.
