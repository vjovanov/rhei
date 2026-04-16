# States Specification

This document defines the default states configuration for tasks in the Rhei plan compiler. The authoritative machine-readable form lives in [states.yaml](states.yaml); the writer-skill mirror is [default-states.md](../skills/rhei-plan-writer/references/default-states.md).

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
