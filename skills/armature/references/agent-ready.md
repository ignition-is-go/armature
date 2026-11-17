# Make the interface usable by agents

Every mounted `Root` (Leptos) or `Machine` (GPUI) registers with the agent bridge. An
agent reads components and operates them directly, with no screenshots or synthetic
input.

## What an agent can do

| Call | Does |
| --- | --- |
| `snapshot` | Lists components: `ref`, `name`, `label`, `value`, `expanded`, `actions`, and each item's `key`, `label`, `disabled`, `selected`. |
| `query(text)` | Finds components and items whose label matches, best first. |
| `act(ref, action, item)` | Runs one action and returns the value afterwards. |
| `run(steps)` | Runs steps in order and stops at the first failure. Each step names a component by ref or label. Reports each change, the failing step, and any components that appeared. |
| `history` | Every committed change, with who made it and a sentence describing it. |
| `tools` | The `snapshot`, `query` and `act` tools as name, description and JSON Schema, to hand to a model. |

An unknown action is refused, and the error lists the valid ones.

## Reaching the bridge

- **Leptos.** Call `armature_leptos::agent::install()` at startup. Page scripts use
  `window.armature`. Call `agent::register_tools()` to offer the tools to a browser's
  built-in agent where it supports WebMCP.
- **GPUI, in process.** Use the functions in `armature_gpui::agent`.
- **GPUI, from outside.** Call `armature_gpui::agent::serve(path, cx)` at startup. It
  listens on a Unix socket that only the current user can open. Send one JSON request
  per line and read one JSON line back:

  ```json
  {"method":"snapshot"}
  {"method":"query","text":"volume"}
  {"method":"act","ref":"e1","action":"select","item":"Large"}
  {"method":"run","steps":[{"target":"Volume","action":"set","item":"70"}]}
  {"method":"history"}
  {"method":"tools"}
  ```

  `examples/gpui-demo` serves on `$XDG_RUNTIME_DIR/armature-gpui-demo.sock`.

## Controlling what agents may do

Set these per component. In Leptos they are props on `Root`: `agent_hidden`,
`agent_secret` and `agent_confirm`. In GPUI call
`machine.agent_access(Access { .. }, cx)`.

| Rule | Effect |
| --- | --- |
| Hidden | The agent is not told the component exists. |
| Secret | The agent sees the component but its value reads `[redacted]`, in snapshots, results and history. |
| Confirm | Acting needs a person's approval. `act` refuses. `run` asks its approver, with the text you gave. |

In Leptos, `run` asks `window.armature.approve(step, what)` and refuses if the page has
not defined it. Over the GPUI socket there is nobody to ask, so such steps are refused.

Agent actions also go through the same change gate as a person's input, marked
`Source::Agent`. Refuse there for rules that depend on the value:

```rust
Callback::new(move |change: ValueChange| {
    !(change.source == Source::Agent && change.value.iter().any(|v| v == "delete"))
})
```

Decide in the gate or with an access rule, not by hiding the control with styles, so
the rule holds for every route an agent could take.

## Test it

Add an agent test for each interface: take a snapshot, run steps against components by
label, and assert the values. Also assert that a forbidden action is refused and that a
secret value never appears. For GPUI, `crates/gpui/tests/primitives.rs` shows both the
in-process functions and `agent::handle(request, cx)` with JSON.
