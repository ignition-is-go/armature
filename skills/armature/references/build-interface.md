# Render an existing primitive

## Choose the primitive

Each lives in `armature_core::<module>` and exports a `Part` enum.

| Primitive | Module | Parts | Value |
| --- | --- | --- | --- |
| Listbox | `listbox` | `Root`, `Option` | Selected keys. Single or multiple. |
| Select | `select` | `Trigger`, `Content`, `Option` | The chosen key. `Content` is a popup. `Select::multiple()` takes several. |
| Command palette | `command` | `Input`, `List`, `Option` | The option that was run. `.page(name, opener, options)` adds a page of options. |
| Tabs | `tabs` | `List`, `Tab`, `Panel` | The current tab, which starts as the first enabled one. With `manual` (an `Options` field), arrows only move focus. |
| Accordion | `accordion` | `Trigger`, `Panel` | The open sections. |
| Radio group, segmented control, rating | `radio` | `Root`, `Option` | The chosen option. |
| Switch | `toggle` | `Root` | `["on"]` or empty. |
| Toolbar | `toolbar` | `Root`, `Button` | None. A press is offered to the gate. |
| Slider | `slider` | `Track`, `Thumb` | The number. |
| Number input | `number_input` | `Input`, `Increment`, `Decrement`, `Scrub` | The number. The wheel steps it while focused, and `Scrub` is dragged sideways. |
| One-time code | `otp_input` | `Input` | The digits. Draw cells from `cells()`. |
| Token input | `token_input` | `Root`, `Input`, `Token` | The tokens. |
| Tree | `tree_view` | `Root`, `Node` | The selected node. `.multiple()` selects several, `.checkable(..)` makes checkboxes, and `.lazy(keys)` loads children when a node is first opened. Draw nodes from `rows()`. Dragging a node, or Alt with an arrow, moves it. The move is offered to the gate as `key>parent#index` with `Reason::Edit`. `landing()` says where a dragged node would drop. |
| Sortable list | `sortable` | `Root`, `Item` | The keys in order. |
| Calendar | `calendar` | `Grid`, `Day`, `PreviousMonth`, `NextMonth` | The day as `YYYY-MM-DD`. Draw days from `weeks()`. `Options::mode` chooses several days or a span, and `unavailable` rules days out. |
| Resizable panels | `resizable` | `Root`, `Handle` | The first panel's share, in percent. |
| Dialog | `dialog` | `Trigger`, `Backdrop`, `Content`, `Close` | `["open"]` or empty; `is_open()`. `Options` has `modality` (`Modeless`, `Modal`, `Alert`) and `closes` (`Never`, or `By` which gestures). |
| Popover | `popover` | `Trigger`, `Content`, `Title`, `Description`, `Arrow`, `Close` | None. `Content` is a popup. |
| Tooltip | `tooltip` | `Trigger`, `Content` | None. Shows on hover after a delay, and on focus. |
| Sheet and drawer | `sheet` | `Trigger`, `Backdrop`, `Content`, `Handle`, `Scroller` | The index of the height it rests at. Size it to `height()`. |
| Colour picker | `color_picker` | `Area`, `AreaThumb`, `Hue`, `HueThumb`, `Input` | The colour as `#rrggbb`. |
| Swipe row | `swipe_row` | `Row`, `Action` | None. A pressed action is offered to the gate. Move the row by `offset()`. |
| Virtual list | `virtual_list` | `Viewport`, `Row` | The selected row's index. Draw rows from `rows()`. |
| Timeline | `timeline` | `Track`, `Span`, `SpanEnd` | Each span as `key:start-end`. |
| Data grid | `data_grid` | `Root`, `Cell` | The selected cells as `row,column`. Edits arrive with `Reason::Edit`. |
| Canvas | `canvas` | `Surface`, `Node`, `Resize`, `PortOut`, `PortIn` | Each node as `key:x,y,width,height`. Alt with an arrow selects the nearest node that way. |
| Editable text | `editable` | `Preview`, `Input` | The text. Enter, F2, a press or typing starts editing. Enter, Tab or leaving saves. Escape cancels. |
| Prompt | `prompt` | `Input` | None. Enter offers the entry to the gate and clears the field. ArrowUp and ArrowDown recall earlier entries. |
| Hold button | `hold_button` | `Root` | None. Offers `confirmed` to the gate once held. Draw progress from `progress(now_ms)`. |
| Transfer list | `transfer_list` | `Available`, `Chosen`, `Option`, `Add`, `Remove` | The chosen keys. Build it with `TransferList::new(keys)` and draw each list from `list(chosen)`. |
| Wheel picker | `picker` | `Viewport`, `Option` | The option in the middle. Pad the content so the first and last options can reach the middle. |
| Button | `button` | `Root` | None. A press offers `pressed` to the gate. Set `["pending"]` while its action runs. |
| Checkbox | `checkbox` (`Checkbox`) | `Root`, `Indicator` | `["on"]`, `["indeterminate"]` or empty. |
| Checkbox group | `checkbox` (`CheckboxGroup`) | `GroupPart::Root`, `All`, `Option` | The checked keys. `All` checks or clears every box. |
| Toggle and toggle group | `toggle_group` | `Root`, `Toggle` | The keys of the toggles that are down. |
| Collapsible | `collapsible` | `Trigger`, `Panel` | `["open"]` or empty. |
| Meter, progress, separator | `gauge` (`Meter`, `Progress`, `Separator`) | `Root`, `Track`, `Indicator`, `Label`, `Value` | The number. Read-only: the owner sets it. |
| Avatar | `avatar` | `Root`, `Image`, `Fallback` | None. Put the picture inside `Image`. Read `status()`. |
| Pagination | `pagination` | `Root`, `First`, `Previous`, `Page`, `Next`, `Last` | The page, from 1. Draw the row from `range()`. |
| Text field, password, multi-line, label and field | `text_field` | `Root`, `Label`, `Input`, `Description`, `Error`, `Clear`, `Reveal` | The text. A submit arrives with `Reason::Keyboard`. Fill `Error` from `error()`. |
| Combobox and autocomplete | `combobox` | `Input`, `List`, `Option`, `Chip`, `Clear`, `Empty`, `Status` | The chosen keys, or the text with `takes: Takes::AnyText` (an `Options` field). Render every option: the primitive hides those that do not match. Typing arrives as `Input::Text` on `Input`; `results()` lists the matches and `is_open()` says whether the list shows. |
| Menu | `menu` | `Trigger`, `Content`, `Item`, `Area` | The checked entries, with `group=key` for radios. Running an action offers `run:<key>`. Draw the entries of `shown()`. |
| Menubar | `menubar` | `Root`, `Trigger`, `Content`, `Item` | As a menu, with entries keyed `menu/entry`. Draw each menu from `shown(menu)`. |
| Navigation menu | `navigation_menu` | `Root`, `Trigger`, `Link`, `Panel` | The open panel. Following a link offers `link:<key>`. |
| Hover card | `tooltip` (`Tooltip::hover_card()`) | `Trigger`, `Content` | None. |
| Trigger menu (mentions, commands) | `trigger_menu` | `Input`, `List`, `Option`, `Empty` | The text. Key each option with its trigger character, such as `@ada`. |
| Toasts | `toast` (`Toaster`) | `Region`, `Toast`, `Title`, `Description`, `Action`, `Close` | The toasts, each from `Toast::encode`. You add to the value. The toaster offers the list without a toast when it goes. Draw from `visible()`. |
| Date input | `date_input` | `Root`, `Segment` | `YYYY-MM-DD`, with `THH:MM` for a time. `DateInput::new(Precision::Day | Minute | Second)` sets how much it takes; `.with_clock(Clock::Twelve)` adds a `period` segment, and `.between(..)` sets limits. Key segments `year`, `month`, `day`, `hour`, `minute`. |
| Date picker | `date_picker` | `Segment`, `Trigger`, `Content`, `Grid`, `Day`, `PreviousMonth`, `NextMonth` | `YYYY-MM-DD`. Draw days from `calendar().weeks()`. |
| Carousel | `carousel` | `Root`, `Viewport`, `Slide`, `Previous`, `Next`, `Indicator`, `Play` | The slide on show. Move the strip by `position()`. |
| Scroll area | `scroll_area` | `Root`, `Viewport`, `Scrollbar`, `Thumb` | None. Place the grip with `thumb(track_length)`. `ScrollArea::horizontal()` scrolls sideways. |
| Image cropper | `image_cropper` | `Area`, `Selection`, `Handle` | `x,y,width,height` in the picture's pixels. Place the selection with `fractions()`. Key handles by compass point. |
| File input | `file_input` | `Dropzone`, `Trigger`, `List`, `File`, `Remove` | The file names. Draw from `files()`, and `rejected()` says what was turned away. |
| Event list (timeline, feed) | `event_list` | `Root`, `Event` | None. `set("current", key)` marks the step in progress, and `status(key)` says done, current or upcoming. With `feed`, PageUp and PageDown move between articles. |
| Range slider | `range_slider` | `Track`, `Thumb` | Each grip's number, lowest first. Key grips `0`, `1`. Place them with `fractions()`. |
| Panel group | `panel_group` | `Root`, `Panel`, `Handle` | Each panel's share in percent. Key panels and the divider after each `0`, `1`. |
| Rating | `rating` | `Root`, `Star` | The number of stars, in halves if allowed. Fill each star by `fill(star)`. |
| Waterfall | `waterfall` | `Root`, `Row`, `Bar`, `Axis` | The selected span. Draw from `rows()`, `bar(key)` and `ticks(most)`. |
| Outline | `outline` | `Root`, `Row` | The rows in order, each `depth:key`. The outline owns its rows: `Outline::new(options, rows)`. Draw from `rows()`, indenting by depth. Space picks a row up, arrows move and nest it, Space drops it. |
| Text with tokens | `token_editor` | `Root`, `Token` | The content as text, each token written `[@label](kind:value)`. The primitive edits its own content key by key, so `Root` is not a native field: draw `segments()` with the caret at `caret()` (`split_at_caret()` gives both sides). `trigger(rules)` finds an `@` or `/` being typed and `complete(match, token, gate)` replaces it with a token. For plain tags use `token_input`. |

A part marked as an item part (`Primitive::is_item`) is rendered once per item and takes
that item's key. Some other parts also take a key: a tab's panel, an accordion's panel,
a calendar's day, a token.

## Leptos

`Root` owns the state. Every `Part` inside it infers the primitive from the part enum.

In your crate's `Cargo.toml`, with armature checked out beside it:

```toml
[dependencies]
armature-core = { path = "../armature/crates/core" }
armature-leptos = { path = "../armature/crates/leptos" }
leptos = { version = "0.8", features = ["csr"] }
```

The snippets below also need `use leptos::prelude::*;` for `view!`, `Callback` and
signals. `armature_leptos` re-exports `ValueChange`, `Source` and `Reason`.

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
- A part that closes stays rendered until its CSS transitions and animations end. It
  carries `data-ending-style` while it leaves and `data-starting-style` for its first
  frame, so `[data-starting-style], [data-ending-style] { opacity: 0 }` with a
  `transition` fades it both ways.
- While a modal part is shown, a press outside it does not reach popups beneath it.
- An accordion built with `findable: true` hides its panels with
  `hidden="until-found"`, so find-in-page searches them and opens the one it matches.
- Pass `settings` to `Root`, a signal of names and values, for what the owner
  changes while a component runs, such as a combobox's `status` (`loading`, `error`
  or `idle`).
- `use_shortcut("mod+k", action)` runs an action from anywhere on the page.
- `chosen_file(name)` returns the browser `File` behind a name in a file input's
  value, to read or upload.
- A picture inside a part reports loading and failure to the primitive by itself.
- `LocalStorage` and `CookieStorage` keep a `Preference` between visits.
  `armature_core::preference::preferences_script` writes the script for the page head
  that applies stored choices before the first paint. `system_prefers_dark()` resolves
  a stored `system` colour scheme.
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
- Render `live_region(&machine, cx)` once in the view that owns a machine that
  announces, such as a sortable list. It carries the latest announcement to a screen
  reader. The `Machine` also emits `Announcement` events if you want them yourself.
- GPUI's own key bindings and actions are the way to bind shortcuts.
  `binding_for(&Shortcut)` turns one written as `mod+shift+k` into the text
  `KeyBinding::new` takes.
- Parts that name, control or describe one another are linked in the accessibility
  tree for you.
- `machine.set(name, value, cx)` changes a setting while the component runs.
- A file dropped on a part that takes drops, or chosen in the picker, arrives with
  its path. Call `machine.loaded(part, ok, window, cx)` when a picture inside a part
  loads or fails, since GPUI's `img` does not report it.
- Tab traversal is the application's job. Bind `tab` and `shift-tab` to actions that
  call `window.focus_next(cx)` and `window.focus_prev(cx)`, as `examples/gpui-demo`
  does. While a modal part is shown, Tab cycles among that primitive's own parts. Other
  focusable elements you put inside it are not part of the cycle.
