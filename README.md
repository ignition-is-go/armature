# armature

Headless UI behaviors and primitives for Rust, defined once and rendered by any
framework. It follows the design of Vercel Labs' [UI SDK](https://github.com/vercel-labs/ui):
behaviors compose into primitives, thin adapters render them, and an agent bridge lets
a model read and operate the interface directly.

Status: experimental. The API will change.

## Layout

| Path | Holds |
| --- | --- |
| `crates/core` | `armature-core`: behaviors, the `Primitive` contract, 17 primitives, the agent registry, a conformance harness. No dependencies. |
| `crates/leptos` | `armature-leptos`: one generic adapter for Leptos. |
| `crates/gpui` | `armature-gpui`: one generic adapter for GPUI. |
| `examples/leptos-demo` | Every primitive in a browser. Run `trunk serve` there. |
| `examples/gpui-demo` | Primitives in a native window. Run `cargo run -p gpui-demo`. |
| `skills/armature` | The agent skill for building with armature. |

## How it fits together

A primitive is a state machine with named parts. It takes neutral inputs (keys,
presses, pointer gestures with measured boxes, typed text, timer ticks) and answers
with what the adapter must do (move focus, announce, wake later, capture the pointer)
and with the semantics of each part (role, state, visibility, focusability). An adapter
turns those into its framework's elements, events and accessibility tree. No adapter
names a primitive, so a new primitive needs no adapter work.

## Check it

```sh
cargo test -p armature-core -p armature-gpui
cargo clippy -p armature-core -p armature-gpui --all-targets
cargo clippy -p armature-leptos --target wasm32-unknown-unknown
```

## Publishing

`armature` and `armature-core` are taken on crates.io. The crates need another prefix
before they can be published.
