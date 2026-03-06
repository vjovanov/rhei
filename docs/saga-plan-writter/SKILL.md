---
name: saga-plan-writter
description: Create, refactor, and validate Saga Plan markdown documents for project execution. Use when users ask for implementation plans, task breakdowns, roadmap-to-task conversion, dependency cleanup, or status updates in a strict, structured plan format with task states and prerequisites.
---

# Saga Plan Writter

Produce a Saga Plan markdown document that is deterministic, dependency-safe, and ready for execution.

## Output Contract

- Emit exactly one H1: `# Saga: <title>`.
- Emit exactly two H2 sections before tasks, in this order: `## Context`, then `## Goals`.
- Emit `## Tasks` as the final H2 section.
- Emit at least one task under `## Tasks`.

### Task Format

Use this exact block shape:

```markdown
### Task <id>: <title>
**State:** <state>
**Prior:** Task <id>, Task <id>

<description>
```

Apply these rules:
- Keep `**State:**` as the first metadata line.
- Place `**Prior:**` second when present.
- Omit `**Prior:**` when no prerequisites exist.
- Emit no other metadata fields.
- Keep descriptions actionable and implementation-oriented.

### Allowed States

Use only these state values:
- `pending`
- `in-progress`
- `blocked`
- `completed`
- `cancelled`

For markdown safety, format `in-progress` as:

```markdown
**State:** `in-progress`
```

Backticks are acceptable for all state values when they improve consistency and readability.

### ID Policy

- Choose exactly one ID style per document.
- Numeric style: `1`, `2`, `3`, ...
- Named style: `setup`, `review`, `api`, ...
- Prefer numeric IDs unless the plan is small and conceptual.
- Do not mix styles in one document.

### Subtask Format

Use subtasks only with numeric task IDs.

```markdown
#### Subtask <n>.<m>: <title>
<description>
```

Apply these rules:
- Prefer subtasks for non-trivial tasks to support implementation logging.
- Subtasks may be skipped for simple, atomic tasks that are clear without decomposition.
- When skipping subtasks, make the task description explicit enough to act as a single implementation log entry.
- Match `<n>` to the parent task number.
- Increment `<m>` sequentially from `1`.
- Place subtasks directly under the parent task description.

## Planning Workflow

1. Extract deliverables, constraints, and sequencing needs from the request.
2. Decompose the work into independently completable tasks.
3. Assign only real prerequisites to maximize parallel execution.
4. Build a dependency DAG and remove cycles before drafting. Ensure the graph is topologically sortable for execution order. In numeric-ID plans, a task must not depend on a higher-numbered future task (for example, Task 2 cannot depend on Task 3).
5. Draft concise `## Context` and `## Goals` sections before `## Tasks`.
6. Write each task and subtask as concrete implementation instructions.
7. Set initial states correctly:
   - New plan: set all tasks to `pending`.
   - Existing plan update: preserve truthful `completed` and `cancelled` states unless explicitly changed.
8. Run the validation checklist before returning output.

## Validation Checklist

Validate every response against all checks:

- Use one H1 and match `# Saga: <title>`.
- Keep `## Context` and `## Goals` present, in that order, before `## Tasks`.
- Keep `## Tasks` present and last.
- Format every task as `### Task <id>: <title>`.
- Include `**State:**` on every task with an allowed value.
- Place `**Prior:**` only after `**State:**` when present.
- Reference only existing tasks in each `**Prior:**` line.
- Keep dependencies acyclic.
- Keep ID style consistent across the document.
- Keep subtask numbering consistent with parent IDs and local order.
- Ensure non-trivial tasks are decomposed into subtasks; allow simple tasks without subtasks.
- Emit no metadata fields beyond `**State:**` and `**Prior:**`.
- Keep heading levels strictly H1/H2/H3/H4 for saga, sections, tasks, subtasks.

## Editing Existing Saga Plans

When modifying an existing saga plan:

1. Preserve unchanged sections and task IDs.
2. Append new tasks using the existing ID style.
3. Update dependencies transitively when inserting or deleting tasks.
4. Do not reset `completed` tasks to `pending` unless explicitly requested.
5. Ensure `## Context` and `## Goals` exist before `## Tasks`; fold equivalent legacy sections into them when needed.
6. Keep `## Tasks` as the final section after edits.

## Missing Information Handling

If required input is missing:

- Ask the user to provide all missing information.
- If the missing information is project-related, the user can instruct you to summon a researcher.
