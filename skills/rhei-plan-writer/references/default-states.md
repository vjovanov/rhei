# Default Rhei State Machine

These are the default states used by Rhei plans when no project-specific state machine is declared (i.e., when `**States:**` is omitted, or when the project's `docs/states.yaml` mirrors this set).

Each state carries an instruction for executing agents:

- `draft` — When all dependencies are met, you can expand the description of the task according to the current state of the source code. Pick up when all `**Prior:**` tasks are `completed`. Transition to `in-progress` before writing code.
- `pending` — Ready to start the real task.
- `in-progress` — Actively implement the task and its subtasks. Log progress per subtask. When implementation is complete and self-tested, transition to `agent-review`.
- `agent-review` — A separate reviewing agent inspects the result against the task description, subtasks, and repository conventions. On pass, transition to `human-review` (or `completed` if no human gate is required). On fail, transition to `agent-review-fix` with concrete findings.
- `agent-review-fix` — The implementing agent addresses the reviewer's findings only — no scope expansion. After applying fixes, transition back to `agent-review` for re-review.
- `human-review` — Stop all agent work on this task. Wait for a human to approve, request changes, or edit the plan. Do not transition out of this state autonomously.
- `completed` — Treat as immutable. Do not re-open, re-run, or modify unless the user explicitly requests it.
- `cancelled` — Skip entirely. Do not execute, review, or reference as a prerequisite for new work.
