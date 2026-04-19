# Rhei States Specification

This document defines the default states configuration for tasks in the Rhei plan compiler. The authoritative machine-readable form lives in [states.yaml](states.yaml); the writer-skill mirror is [default-states.md](../../skills/rhei-plan-writer/references/default-states.md).

The state-machine schema also permits an optional top-level `personality` string. The default Rhei machine does not set one, but custom machines may use it to inject role framing into `rhei next`.

## States

| State | Description | Initial | Final |
|-------|-------------|---------|-------|
| `draft` | Task is still being shaped; description not ready for execution | Yes | No |
| `pending` | Task ready to start once prerequisites are `completed` | No | No |
| `in-progress` | Task actively being implemented | No | No |
| `agent-review` | A separate reviewing agent inspects the result | No | No |
| `agent-review-fix` | Implementing agent applies reviewer findings, no scope change | No | No |
| `human-review` | Work paused pending human inspection; no autonomous exit | No | No |
| `completed` | Task finished successfully; immutable | No | Yes |
| `cancelled` | Task no longer needed; skip entirely | No | Yes |

## Transitions

See [states.yaml](states.yaml) for the enforced transition table. Summary:

- `draft` → `pending` | `cancelled`
- `pending` → `in-progress` | `cancelled`
- `in-progress` → `agent-review` | `human-review` | `cancelled`
- `agent-review` → `agent-review-fix` (fail) | `human-review` (pass, gated) | `completed` (pass, ungated)
- `agent-review-fix` → `agent-review` | `cancelled`
- `human-review` → `in-progress` | `completed` | `cancelled`

Any transition not listed in `states.yaml` is forbidden.

## Related Documentation

- [Plan Language Specification](../rhei.spec.md) - Formal grammar and semantic constraints
- [Transitions Specification](rhei-transitions.spec.md) - Formal state transition system, callbacks, and YAML schema
- [How Rhei Is Used](rhei-usage.spec.md) - Roles, coordination patterns, and agent workflows
- [Plan Language Usage Guide](rhei-authoring.spec.md) - Practical authoring patterns and walkthroughs
- [Transition Callback Examples](rhei-callbacks.spec.md) - Callback implementations across languages
- [State Machine Writer](rhei-state-machine-writer.spec.md) - Designing custom state machines from project specs and teams
