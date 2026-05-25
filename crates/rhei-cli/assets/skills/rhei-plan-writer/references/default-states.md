# Default Rhei State Machine

These are the default states used by Rhei plans when no project-specific state machine is declared (i.e., when `**States:**` is omitted, or when the project's `docs/states.yaml` mirrors this set). This mirrors the built-in `rhei` machine (version 3.0).

Every task node starts in `draft` (the profile's `initial`). Agents claim a task with `rhei next` — which sets `**Assignee:**` without changing the state — then work in the current state, call `rhei transition` to advance, and call `rhei complete` to finalize with a result message.

Each state carries an instruction for executing agents:

- `draft` — Task requires analysis before execution. Pick up when all `**Prior:**` tasks are in a terminal state. Analyze the project and write a concrete description of what should be done. Transition to `pending` when the description is finalized.
- `pending` — Ready for implementation. An `**Assignee:**` on a pending task means work is actively in progress. Implement the task and any child task nodes, logging progress per child task. When implementation is complete and self-tested, either call `rhei complete` to finish directly or transition to `agent-review` when a separate review pass is required.
- `agent-review` — A separate reviewing agent inspects the result against the task description, child task nodes, and repository conventions. On pass, transition to `human-review` (or `completed` if no human gate is required). On fail, transition to `agent-review-fix` with concrete findings.
- `agent-review-fix` — The implementing agent addresses reviewer findings only — no scope expansion. Transition back to `agent-review` after applying fixes.
- `human-review` (gating) — Stop all agent work on this task. Wait for a human to approve, request changes, or edit the plan. Do not transition out of this state autonomously.
- `completed` (final) — Treat as immutable. Do not re-open, re-run, or modify unless the user explicitly requests it.
- `cancelled` (final) — Skip entirely. Do not execute, review, or reference as a prerequisite for new work.

## Declared transitions

- `draft` → `pending`, `cancelled`
- `pending` → `agent-review`, `human-review`, `completed`, `cancelled`
- `agent-review` → `agent-review-fix`, `human-review`, `completed`
- `agent-review-fix` → `agent-review`, `cancelled`
- `human-review` → `pending`, `completed`, `cancelled`

No other transitions are legal.

## Profiles and node policy

The default machine defines a single profile `default-rhei` with `initial: draft` and an `allowed` set covering every non-terminal and terminal state above. Both `node_policy.root` and `node_policy.default` resolve to that profile, so every node kind (including the implicit `rhei` root) shares the same flow.
