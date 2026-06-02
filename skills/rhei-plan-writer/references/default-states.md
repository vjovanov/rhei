# Default Rhei State Machine

These are the default states used by Rhei plans when no project-specific state machine is declared (i.e., when `**States:**` is omitted, or when the project's `docs/states.yaml` mirrors this set). This mirrors the built-in `rhei` machine (version 4.0).

Every task node starts in `pending` (the profile's `initial`). Agents claim a task with `rhei next` — which sets `**Assignee:**` without changing the state — then do the task and call `rhei complete` to finalize with a result message.

Each state carries an instruction for executing agents:

- `pending` — Ready to execute. An `**Assignee:**` on a pending task means work is actively in progress. Do the task.
- `completed` (final) — Treat as immutable. Do not re-open, re-run, or modify unless the user explicitly requests it.

## Declared transitions

- `pending` → `completed`

No other transitions are legal.

## Profiles and node policy

The default machine defines a single profile `default-rhei` with `initial: pending` and an `allowed` set covering the `pending` and `completed` states. Both `node_policy.root` and `node_policy.default` resolve to that profile, so every node kind (including the implicit `rhei` root) shares the same flow.
