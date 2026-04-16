---
name: rhei-plan-worker
description: Execute tasks in a Rhei Plan markdown document without an orchestrator. Takes one required argument `<plan>` — the path to a `.rhei.md` file (e.g. `rhei-plan-worker docs/my-plan.rhei.md`). Use when the user asks to work, implement, advance, drive, or make progress on a specific `.rhei.md` plan — the worker self-selects the next eligible task, advances its state, implements it, logs subtask progress, and respects human-review gates.
argument-hint: <plan>
---

# Rhei Plan Worker (Unorchestrated)

Pick up a Rhei Plan and make progress on it without any external scheduler. The worker is driven by the plan itself: the state machine defines what is legal, `**Prior:**` edges define what is ready, and the state instructions define what to do.

Do not repurpose this skill for plan authoring — use `rhei-plan-writer` for that. Here, structural edits to the plan are limited to advancing `**State:**` values, logging subtask progress, and recording review findings.

## Parameters

The skill takes a single required parameter:

- `<plan>` — path to the `.rhei.md` file to work on.

Invoke as `rhei-plan-worker <plan>` (e.g., `rhei-plan-worker docs/markdown-plan-compiler.md`). If the caller does not supply `<plan>`, ask for it and stop — do not guess, do not scan the working directory, do not pick from multiple candidates. The plan path is the worker's only source of truth for which plan to drive.

All other inputs (state machine, task selection, transitions) are derived from the plan file and the state machine it declares.

## Operating Loop

Execute this loop until no eligible task remains or a human gate stops you:

1. **Open the plan at `<plan>`.** If the path does not exist or is not a Rhei Plan, stop and report.
2. **Load the state machine.** Run `rhei states <plan>` to read allowed states, their agent instructions, and the transition graph for the machine the plan declares via `**States:**`. Add `--json` for structured data. Fall back to the YAML referenced by `**States:**` (or `docs/states.yaml`, or [default-states.md](../rhei-plan-writer/references/default-states.md)) if the CLI is unavailable.
3. **Read the plan.** Prefer `rhei render <plan> --format json --pretty` for structured access. Read the raw markdown too — you will edit it in place for progress logging.
4. **Select the next task.** Apply the rules in *Task Selection* below.
5. **Claim the task.** Run `rhei transition <plan> --task <id> --from pending --to in-progress`. If the command fails with a conflict (another agent already claimed the task), go back to step 3 — re-read the plan and re-select.
6. **Execute the task.** Follow the state's `instructions` field verbatim. Implement subtasks in order, logging per subtask (see *Progress Logging*).
7. **Advance the state** when the current state's exit condition is met. Run `rhei transition <plan> --task <id> --from in-progress --to agent-review` (or the appropriate target state). If the transition fails, re-read the plan and diagnose.
8. **Stop at terminal or gating states.** `completed` and `cancelled` are final. `human-review` halts the worker — do not transition out of it autonomously.
9. **Loop.** Go to step 4 until nothing is eligible.

Never skip validation. A failed `rhei validate` run means the last edit is wrong — fix it before moving on.

## Task Selection

A task is **eligible** when all of these hold:

- Its `**State:**` is the state the machine treats as "ready to start" (typically `pending`; check the machine's transitions for the one that leads into `in-progress`).
- Every task listed in `**Prior:**` is in a terminal-success state (typically `completed`). `cancelled` priors also unblock — treat the dependency as satisfied.
- No ancestor task is in `human-review` or any other state whose instructions forbid downstream work.

Selection policy when multiple tasks are eligible:

1. Prefer the task with the fewest remaining descendants (clears the graph faster).
2. Break ties by task ID order as written in the plan.
3. Never pick a task in `draft` — its description is not finalized. If the user asks for draft work, surface it and wait for promotion to `pending`.

If a task is already in `in-progress` or `agent-review` (e.g., a prior session was interrupted), resume it before starting anything new.

## State Transitions

All state transitions **must** go through the `rhei transition` command:

```bash
rhei transition <plan> --task <id> --from <current-state> --to <target-state>
```

Do not edit `**State:**` fields in the markdown by hand. The CLI command provides:

- **File locking** — prevents concurrent writes from corrupting the plan.
- **Compare-and-swap** — the `--from` flag ensures the task is still in the expected state. If another agent already transitioned it, the command fails with a conflict error.
- **Transition validation** — illegal transitions are rejected before any write occurs.

On conflict, re-read the plan (step 3 of the operating loop) and re-select a task. Do not retry the same transition blindly.

Typical transitions for the default Rhei machine:

- `pending` → `in-progress` — when you start implementation.
- `in-progress` → `agent-review` — when implementation is complete and self-tested.
- `agent-review` → `agent-review-fix` — on review failure; record findings first.
- `agent-review` → `human-review` — when a human gate is required.
- `agent-review` → `completed` — when no human gate is required.
- `agent-review-fix` → `agent-review` — after applying reviewer findings.
- `human-review` → ... — worker does not perform these; a human does.

If the loaded machine differs, trust it over this list.

## Progress Logging

Log implementation progress by appending to the subtask body — do not invent new metadata fields.

- One short paragraph per subtask, written as you complete it (not in a batch at the end).
- State the concrete change: files touched, functions added, commands run to verify.
- Do not restate the subtask title; extend the description.
- For tasks without subtasks, append a single paragraph to the task description.

When a task re-enters `in-progress` from `agent-review`, append a new paragraph describing the rework rather than rewriting history.

## Agent Review

When a task enters `agent-review`, the reviewer is a *different* mental mode, not a different person. The reviewer:

- Reads the task description, subtasks, and the diff actually produced.
- Checks repository conventions (lint, format, test commands listed in [AGENTS.md](../../AGENTS.md) or the project's equivalent).
- Records concrete findings as a new paragraph in the task body, prefixed with `Review:` — one bullet per finding.
- Transitions to `in-progress` (rework), `human-review` (needs human), or `completed` (pass).

Never approve a task whose tests or build fail.

## Editing Discipline

State transitions and progress logging are separate concerns:

- **State transitions** — always use `rhei transition`. Never edit `**State:**` fields in the markdown directly.
- **Progress logging** — edit the markdown directly to append subtask progress (see *Progress Logging*). After every direct edit, run `rhei validate <plan>` to confirm the file is still well-formed.

General rules:

- Preserve IDs, titles, and `**Prior:**` edges. Structural changes belong to the plan writer, not the worker.
- Do not reorder tasks. Do not delete completed or cancelled tasks.
- Do not reformat unrelated sections.

## Stopping Conditions

Stop the loop and report to the user when any of these is true:

- No eligible task remains (everything is `completed`, `cancelled`, `draft`, or blocked on `human-review`).
- A task reaches `human-review` — stop *that* task but keep working on independent branches of the DAG.
- `rhei validate` fails after an edit you cannot explain — stop and show the user.
- The task requires information or access the worker does not have (e.g., credentials, external decisions) — stop and ask.

When stopping, print a short summary: which tasks advanced, which task is blocked and why, and what the next human action is.

## Unorchestrated Mode Notes

"Unorchestrated" means **no** external process tells the worker what to do next. Consequences:

- The worker reads the plan on every iteration; treat the markdown file as the single source of truth.
- Do not cache state across iterations beyond the current conversation.
- Multiple workers can safely operate on the same plan. The `rhei transition` command's compare-and-swap semantics prevent two workers from claiming the same task — the loser gets a conflict error and re-selects.
- Do not batch multiple task transitions into one command — one task, one transition, then re-read.

## Missing Information Handling

If the plan path, state machine, or a task description is ambiguous or missing required detail:

- Ask the user before editing.
- Never invent prerequisites, states, or transitions to unblock selection.
- If a task description is too thin to implement, surface it and ask for clarification — do not silently expand scope.
