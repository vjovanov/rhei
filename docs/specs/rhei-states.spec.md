# Rhei States Specification

This document defines the default states configuration for tasks in the Rhei plan compiler. The authoritative machine-readable form lives in [states.yaml](states.yaml); the writer-skill mirror is [default-states.md](../../skills/rhei-plan-writer/references/default-states.md).

The state-machine schema also permits these optional model-selection fields for multi-model workflows:

- Top-level `personality: <string>` to inject role framing into `rhei next` (applies to all states that do not override it)
- Per-state `personality: <string>` to override the machine-level personality for that specific state
- Top-level `models: [<model-name>, ...]` to declare the model identifiers available to the machine
- Per-state `all_models: [<model-name>, ...]` to declare the full model set that may execute that state
- Per-state `model: <model-name>` to bind a state to exactly one declared model
- Per-state `iterations: <integer>` to cap counted loop re-entries for that state

When `models` is omitted, the machine behaves as it does today and states are not model-constrained. When `models` is present, a state may either omit both selector fields, set `all_models: [<name>, ...]`, or set `model: <name>`. Setting both `all_models` and `model` on the same state is invalid. `iterations` is orthogonal to model selection and may be used together with either `all_models` or `model`.

## Schema Additions

### Top-level fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `personality` | string | No | Shared role framing for agents working tasks in this machine |
| `models` | string array | No | The complete set of model identifiers available to the machine |

### Per-state fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `personality` | string | No | Overrides the machine-level `personality` for this state; takes precedence in `rhei next` output |
| `iterations` | integer | No | Maximum number of loop-back re-entries permitted for this state before the workflow must take a non-loop exit |
| `all_models` | string array | No | The complete set of declared model identifiers allowed to work this state |
| `model` | string | No | A single model identifier from the machine-level `models` list |

### Validation Rules

- `models`, when present, must be a list of unique non-empty strings.
- `state.model`, when present, must match an entry from the machine-level `models` list.
- `state.all_models`, when present, must be a list of unique non-empty strings drawn from the machine-level `models` list.
- A state must not declare both `all_models: [..]` and `model: <name>`.
- `state.all_models: []` is treated the same as omitting the field.
- `state.iterations`, when present, must be an integer greater than or equal to `1`.

Counted-loop counters are task-instance data, not state-definition data. The state machine declares the cap with `iterations`; runtimes persist the current per-task counts in task metadata.

When a state declares both `all_models` and `iterations`, the engine runs the state once per listed model and each model-specific execution tracks its own iteration budget.

## States

| State | Description | Initial | Final |
|-------|-------------|---------|-------|
| `draft` | Task is still being shaped; description not ready for execution | Yes | No |
| `pending` | Task ready for implementation once prerequisites are `completed` | No | No |
| `agent-review` | A separate reviewing agent inspects the result | No | No |
| `agent-review-fix` | Implementing agent applies reviewer findings, no scope change | No | No |
| `human-review` | Work paused pending human inspection; no autonomous exit | No | No |
| `completed` | Task finished successfully; immutable | No | Yes |
| `cancelled` | Task no longer needed; skip entirely | No | Yes |

## Transitions

See [states.yaml](states.yaml) for the enforced transition table. Summary:

- `draft` → `pending` | `cancelled`
- `pending` → `agent-review` | `human-review` | `completed` | `cancelled`
- `agent-review` → `agent-review-fix` (fail) | `human-review` (pass, gated) | `completed` (pass, ungated)
- `agent-review-fix` → `agent-review` | `cancelled`
- `human-review` → `pending` | `completed` | `cancelled`

Any transition not listed in `states.yaml` is forbidden.

## Related Documentation

- [Plan Language Specification](../rhei.spec.md) - Formal grammar and semantic constraints
- [Transitions Specification](rhei-transitions.spec.md) - Formal state transition system, callbacks, and YAML schema
- [How Rhei Is Used](rhei-usage.spec.md) - Roles, coordination patterns, and agent workflows
- [Plan Language Usage Guide](rhei-authoring.spec.md) - Practical authoring patterns and walkthroughs
- [Transition Callback Examples](rhei-callbacks.spec.md) - Callback implementations across languages
- [State Machine Writer](rhei-state-machine-writer.spec.md) - Designing custom state machines from project specs and teams
