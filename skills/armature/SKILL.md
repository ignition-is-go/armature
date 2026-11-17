---
name: armature
description: Build interfaces with armature, the Rust library of headless UI behaviors and primitives (listbox, select, tabs, dialog, popover, slider, calendar, tree, data grid, canvas, command palette and more) that runs in Leptos and GPUI from one core. Use when a project depends on armature-core, armature-leptos or armature-gpui, when asked to render or style an armature primitive, to create a primitive armature does not ship, to add a framework adapter, or to make an interface operable by agents through the armature agent bridge.
---

# Build with armature

armature defines each UI primitive once, in `armature-core`, as a state machine with no
DOM or framework dependency. One generic adapter per framework renders every primitive.
Primitives ship unstyled. You supply the look and the children.

| Crate | Holds |
| --- | --- |
| `armature-core` | Behaviors, the `Primitive` contract, the primitives, the agent registry, the conformance harness |
| `armature-leptos` | `Root` and `Part` components, `use_primitive`, `use_shortcut`, `LocalStorage`, `CookieStorage`, `window.armature` agent bridge |
| `armature-gpui` | `Machine` entity, `part()` and `live_region()` elements, agent bridge and Unix-socket transport |

## Read first

Start with `references/core-contract.md`: the types, import paths and signatures you
need to drive any primitive, and each primitive's options. Read the source only for
edge behavior the references do not cover.

1. `crates/core/src/primitive.rs` is the whole contract between primitives and adapters.
2. The primitive you will use, in `crates/core/src`. Its `Part` enum lists what you
   render. Its tests state its behavior.
3. The demo for your framework: `examples/leptos-demo` or `examples/gpui-demo`.

## Pick your path

- **Render an existing primitive.** Read [references/build-interface.md](references/build-interface.md).
- **Create a primitive that armature does not ship.** Read
  [references/create-primitive.md](references/create-primitive.md).

Both paths then continue to [references/agent-ready.md](references/agent-ready.md).

## Rules for every path

- Put behavior in `armature-core`. An adapter must never name a primitive. If a new
  primitive needs an adapter change, the contract is missing a capability. Add the
  capability to `primitive.rs` and implement it in every adapter.
- Keep `armature-core` free of dependencies and of web or framework vocabulary. It speaks
  in `Input`, `Pointer`, `Reaction`, `Effect` and `Semantics`. ARIA attribute names, DOM
  key names and GPUI types belong in adapters.
- Never write `role`, `aria-*`, `tabindex`, `id` or `hidden` by hand on a part. The
  adapter sets them from the part's `Semantics` and `Relations`.
- Never decide visibility with styles. A part's `Semantics::hidden` decides it, and both
  adapters enforce it after your styling.
- Give every `Root` or `Machine` a label. It is the accessible name and the name an
  agent uses to find the component. Give a part that is a button its own name too.
- Style from state. In Leptos, select on `aria-selected`, `aria-checked`,
  `aria-expanded`, `aria-disabled`, `data-active` and `data-side`. In GPUI, read
  `machine.semantics(part, item)` and branch with `.when(..)`.
- Every value change goes through the change gate, whether a person or an agent caused
  it. To veto a change, return `false` from the gate. Do not undo it afterwards.
- Run `armature_core::testing::assert_conforms` on every primitive you write.

## When you are done

1. Run `cargo test -p armature-core -p armature-gpui`.
2. Run `cargo clippy -p armature-core -p armature-gpui --all-targets` and
   `cargo clippy -p armature-leptos --target wasm32-unknown-unknown`.
3. Run `cargo fmt --all --check`.
4. Run the browser checks: `trunk build` in `examples/leptos-demo`, then
   `node checks/run.mjs`. Add a check for anything new you put in the demo.
5. Drive the result in a real renderer. Headless tests do not catch layout, styling or
   focus faults. For Leptos, run `trunk serve` in `examples/leptos-demo`. For GPUI, run
   `cargo run -p gpui-demo` and operate the window through its agent socket.
6. Report each command with its result, and name anything you did not verify.

In your own crate that depends on armature, steps 1 to 3 are your crate's own
`cargo test`, `cargo clippy` and `cargo fmt --check`, with
`--target wasm32-unknown-unknown` for a Leptos crate, and step 5 is your own app.
