# Make the interface usable by agents

Every mounted `Root` (Leptos) or `Machine` (GPUI) registers with the agent bridge. An
agent reads components and operates them directly, with no screenshots or synthetic
input.

## What an agent sees

`snapshot` returns one entry per component:

| Field | Meaning |
| --- | --- |
| `ref` | Handle for `act`, such as `e3`. Valid while the component is mounted. |
| `name` | The primitive: `listbox`, `tabs` or `select`. |
| `label` | The label you gave the component. |
| `value` | The current value as a list of keys. |
| `expanded` | Whether its popup is open. Absent or null when it has none. |
| `actions` | The actions `act` accepts. |
| `items` | Each item's `key`, `label`, `disabled` and `selected`. |

`act(ref, action, item)` runs one action and returns the value afterwards. An unknown
action is refused and the error lists the valid ones.

## Reaching the bridge

- **Leptos.** Call `armature_leptos::agent::install()` at startup. Page scripts then
  call `window.armature.snapshot()` and `window.armature.act(ref, action, item)`.
- **GPUI, in process.** Call `armature_gpui::agent::snapshot(cx)` and
  `agent::act(cx, ref, action, item)`.
- **GPUI, from outside.** Call `armature_gpui::agent::serve(path, cx)` at startup. It
  listens on a Unix socket that only the current user can open. Send one JSON request
  per line and read one JSON line back:

  ```json
  {"method":"snapshot"}
  {"method":"act","ref":"e1","action":"select","item":"Large"}
  ```

  Responses carry `"ok": true` with `components` or `value`, or `"ok": false` with
  `error`. `examples/gpui-demo` serves on `$XDG_RUNTIME_DIR/armature-gpui-demo.sock`.

## Controlling what agents may do

Agent actions go through the same change gate as user input, marked
`Source::Agent`. Refuse there:

```rust
Callback::new(move |change: ValueChange| {
    !(change.source == Source::Agent && change.value.iter().any(|v| v == "delete"))
})
```

The agent receives `the application refused this change`. Decide in the gate, not by
hiding the control, so the rule holds for every route an agent could take.

## Test it

Add an agent test for each interface: take a snapshot, act on a component by its label,
and assert the value. Also assert that a forbidden action is refused. For GPUI, test
`agent::handle(request, cx)` with JSON, as `crates/gpui/tests/primitives.rs` does.
