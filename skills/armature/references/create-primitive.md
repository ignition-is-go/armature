# Create a primitive

A primitive is one file in `crates/core/src`. It needs no adapter change. Use
`tabs.rs` as the model for a simple one and `select.rs` for one with a popup.

## Steps

1. **Name the parts.** Write a `Part` enum that derives
   `Clone, Copy, Debug, PartialEq, Eq, Hash`. Implement `PartOf` for it so adapters can
   infer the primitive from a part.
2. **Model the state.** Write a `Clone + Default` struct. Hold items in
   `items::Items`, which already does ordering, lookup, enabled-only stepping and
   focus recovery on removal.
3. **Compose behaviors. Do not rewrite them.**
   - `roving::nav_for_key` and `Items::step` for arrow, Home and End movement.
   - `typeahead::Typeahead` for jump-to-match typing.
   - `selection::propose` for single, toggle and range selection.
   - `popup::Placement` to declare where a popup part floats.
   - `grid::step` and `grid::CellRange` for two-dimensional focus and rectangular
     selection, with `cells_to_text` and `text_to_cells` for copy and paste.
   - `tree::Tree` for parents, children, visible rows and checked states that roll up.
   - `text_match` to filter by typed text and mark the matched runs.
   - `undo::UndoStack`, `input_history::InputHistory` and `preview::Preview` for state
     that steps back, recalls earlier entries, or is tried before it is committed.
   - `gesture` and `spans` for flick, spring and rubber-band arithmetic and for laying
     out ranges on a track.
   - Another primitive as a field, as `Select` holds a `Listbox`.
4. **Implement `Primitive`.**
   - `parts()` lists every part. `is_item` marks the parts rendered once per item.
   - `semantics(part, item)` returns role, selected, expanded, hidden, disabled, focusable
     and active. A part that is focusable receives presses and keys. Make exactly one
     part instance a `TabStop`.
   - `send(input, gate)` handles `Key`, `Press`, `Focus` and `PressOutside`. Return
     `handled: true` only when the platform default must be suppressed. Return a focus
     target when focus must move.
   - `popup_of(part)` returns a `Placement` for a popup part. The adapters then measure,
     position, and report outside presses.
   - `snapshot` and `act` expose the primitive to agents. List the actions in a const.
5. **Send every value change through the gate.** Build a `ValueChange`, call the gate,
   and commit only if it returns `true`. Mark user changes `Source::User` and `act`
   changes `Source::Agent`.
6. **Register the module** in `crates/core/src/lib.rs`.
7. **Add a role** to `primitive::Role` if none fits, and map it in both adapters
   (`aria_role` in `crates/leptos/src/parts.rs`, `accessibility_role` in
   `crates/gpui/src/lib.rs`). This is the only adapter edit a new primitive may need.

## Tests

Write tests in the same file. Test what a user or agent can observe: the value, which
item receives focus, which part is hidden, what an agent action returns. Do not assert
on private fields. Cover these cases:

- Movement skips disabled items.
- Exactly one tab stop.
- A gate that returns `false` leaves the value unchanged.
- Removing the focused item moves focus to a neighbour.
- An unknown agent action names the valid actions.

Then render the primitive in both demos and drive it for real. A primitive that passes
its core tests can still fail in a renderer.

## When the contract is not enough

If the primitive needs something an adapter cannot be told today, such as pointer
capture for dragging or measured sizes for virtualization, extend `primitive.rs` with a
neutral input, reaction or declaration. Then implement it in every adapter and test it
in each. Do not special-case the primitive inside an adapter.
