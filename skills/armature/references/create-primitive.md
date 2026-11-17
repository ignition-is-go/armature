# Create a primitive

A primitive is one file in `crates/core/src`. It needs no adapter change. Use
`toggle.rs` as the model for the simplest one, `tabs.rs` for a collection, `select.rs`
for a popup, `slider.rs` for a pointer gesture, `number_input.rs` for text and timers,
`tooltip.rs` for hover, `virtual_list.rs` for scrolling and `canvas.rs` for several
gestures at once.

## Steps

1. **Name the parts.** Write a `Part` enum that derives
   `Clone, Copy, Debug, PartialEq, Eq, Hash`. Implement `PartOf` for it so adapters can
   infer the primitive from a part.
2. **Model the state.** Write a `Clone` struct. Hold items in `items::Items`, which
   already does ordering, lookup, enabled-only stepping and focus recovery on removal.
3. **Compose behaviors. Do not rewrite them.** The table below lists them.
4. **Implement `Primitive`.**
   - `parts()` lists every part. `is_item` marks the parts rendered once per item.
   - `semantics(part, item)` returns role, selected, checked, mixed, pressed,
     current, invalid, obscured, expanded, level, range, hidden, findable, modal,
     disabled, focusable and active. A focusable part receives presses
     and keys. Make exactly one item of a composite control a `TabStop`.
   - `send(input, gate)` handles `Key`, `Press`, `Focus`, `Blur`, `Found`, `Loaded`,
     `Files`, `DragOver`, `PressOutside`, `WindowBlur`, `Tick`, `Text`, `Hover`,
     `Scrolled` and `Paste`. `Blur` arrives
     when focus leaves the primitive, not when it moves between its parts. `Found`
     arrives when find-in-page matches inside a hidden part whose semantics set
     `findable`. Return `handled: true` only when the platform default must be suppressed.
     Return a focus target when focus must move, and effects for anything else.
   - `tracks_pointer(part)` and `pointer(pointer, measure, gate)` handle drags. Return
     `Effect::CapturePointer` on the press to keep receiving moves. Ask `measure` for
     any part's box.
   - `hovers(part)` asks for `Hover` inputs. `scrolls(part)` asks for `Scrolled`
     inputs and makes the adapter carry out `Effect::ScrollTo`. Return the offset you
     want from `wanted_scroll` too, so an agent action can scroll.
   - `drops(part)` asks for `DragOver` and `Files` when files are dragged onto a
     part. Return `Effect::PickFiles` to show the platform's file picker.
   - `wake_after()` asks for a `Tick` that nothing else will bring, such as after
     the owner set a value that starts a timer.
   - To build on another primitive, hold it as a field and hand it inputs with
     `Input::for_parts` and `Reaction::for_parts`. `date_picker.rs` and `menubar.rs`
     show how.
   - `text(part, item)` gives the text of a part the person types into. Its role must
     be `TextBox` or `SpinButton`.
   - `popup_of(part)` returns a `Placement` for a popup part. `relations(part)` links
     parts for assistive technology.
   - `value`, `set_value`, `snapshot` and `act` expose the value to the owner and to
     agents. List the agent actions in a const.
5. **Send every value change through the gate.** Build a `ValueChange`, call the gate,
   and commit only if it returns `true`. Mark changes from `send` and `pointer` as
   `Source::User` and changes from `act` as `Source::Agent`. `set_value` does not ask.
6. **Make buttons answer the keyboard.** A part with the `Button` or `Switch` role gets
   no keyboard activation for free. Track which part has focus and treat Enter and
   Space as a press.
7. **Register the module** in `crates/core/src/lib.rs`.
8. **Add a role** to `primitive::Role` if none fits, and map it in both adapters
   (`aria_role` in `crates/leptos/src/parts.rs`, `accessibility_role` in
   `crates/gpui/src/lib.rs`). This is the only adapter edit a new primitive may need.

## Behaviors to compose

| Need | Module |
| --- | --- |
| Arrow, Home and End movement through a list | `roving`, `items::Items::step` |
| Movement by row and column, rectangular selection, copy and paste | `grid` |
| Movement by position on screen | `spatial` |
| Jump to an item by typing | `typeahead` |
| Single, toggle and range selection | `selection` |
| Selection by a drawn rectangle | `lasso` |
| Filtering and highlighting typed text | `text_match` |
| Telling a drag from a click | `drag` |
| Reordering by pointer or keyboard | `reorder`, `transfer`, `tree::move_node` |
| Moving and resizing boxes | `motion` |
| A value along one or two axes | `number`, `area` |
| Ranges on a track | `spans` |
| Long-press and press-and-hold repeat | `timing` |
| Swipe actions, flicks, springs, rubber-banding | `swipe`, `gesture` |
| Rendering only visible rows, following new content, wheel pickers, edge scrolling | `scroll` |
| Pan and zoom, connections between ports | `viewport`, `connect` |
| Tree lookups, visible rows, checked states | `tree` |
| Popup placement, layer stack, focus trapping | `popup`, `layers` |
| Editing a line of text at a caret | `text_edit` |
| Image crop geometry, language tags and direction | `image`, `locale` |
| Staying rendered through an exit animation | `presence` |
| Undo, entry recall, try-then-commit, edit in place | `undo`, `input_history`, `preview`, `inline_edit` |
| Dates, colours, numbers | `date`, `color`, `number` |
| Shortcuts, preferences and storage, field hints | `shortcut`, `preference`, `input_hints` |

## Tests

Write tests in the same file. Test what a person or agent can observe: the value, which
part receives focus, which part is hidden, what an agent action returns. Do not assert
on private fields.

End with the conformance harness, which checks the rules every primitive must keep:

```rust
#[test]
fn it_conforms() {
    crate::testing::assert_conforms(MyPrimitive::default, &items);
}
```

It has caught real faults in this crate: a disabled header that was still a tab stop,
and Enter on a button doing something other than pressing it.

Then render the primitive in both demos and drive it for real. A primitive that passes
its core tests can still fail in a renderer.

## When the contract is not enough

If the primitive needs something an adapter cannot be told today, extend
`primitive.rs` with a neutral input, effect or declaration. Then implement it in every
adapter and test it in each. Do not special-case the primitive inside an adapter.
