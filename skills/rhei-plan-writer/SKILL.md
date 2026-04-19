---
name: rhei-plan-writer
description: Create, refactor, and validate Rhei Plan markdown documents for project execution. Use when users ask for implementation plans, task breakdowns, roadmap-to-task conversion, dependency cleanup, or status updates in a strict, structured plan format with task states and prerequisites.
---

# Rhei Plan Writer

Produce a Rhei Plan markdown document that is deterministic, dependency-safe, and ready for execution.

## Output Contract

- Emit exactly one H1: `# Rhei: <title>`.
- Optionally emit `**States:** <state-machine-name>` as the first non-empty line below the title to declare which state machine the plan follows. Omit the field to use the built-in `rhei` state machine.
- Emit zero or more contextual H2 sections before tasks.
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
- **Every task MUST have a `**State:**` field.** A task without `**State:**` is invalid and will fail validation. This is the single most common authoring mistake — always check for it before finishing.
- Keep `**State:**` as the first metadata line, directly under the task heading — no blank line between the heading and `**State:**`.
- Place `**Prior:**` second when present.
- Omit `**Prior:**` when no prerequisites exist.
- Separate metadata from description with a blank line.
- Emit no other metadata fields.
- Keep descriptions actionable and implementation-oriented.

### Allowed States

Run `rhei states` in the project to discover the allowed state values, their agent instructions, and the declared state transitions for the state machine the plan will follow. Use `rhei states --state-machine <path>` to target a specific YAML file (for example, the one referenced by a plan's `**States:**` line), and `rhei states --json` when a machine-readable form is preferred. Use only state values reported by that command, follow the instructions printed alongside each state, and respect the declared transitions when advancing tasks.

If the `rhei` CLI is not available in the project, fall back to reading the state machine YAML file directly (typically `docs/states.yaml`, or the file referenced by `**States:**`). If the project does not define its own state machine, fall back to the default Rhei state set documented in [default-states.md](references/default-states.md).

For markdown safety, format hyphenated states with backticks:

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
**State:** <state>

<description>
```

Apply these rules:
- **Every subtask MUST have a `**State:**` field.** A subtask without `**State:**` is invalid and will fail validation — the same rule as for tasks.
- Keep `**State:**` as the first line directly under the subtask heading — no blank line between the heading and `**State:**`.
- Default to including subtasks for every task to support implementation logging.
- Skip subtasks only when a task is truly simple, atomic, and does not benefit from further decomposition.
- When skipping subtasks, make the task description explicit enough to act as a single implementation log entry.
- Match `<n>` to the parent task number.
- Increment `<m>` sequentially from `1`.
- Place subtasks directly under the parent task description.

## Planning Workflow

1. Extract deliverables, constraints, and sequencing needs from the request.
2. Decompose the work into independently completable tasks.
3. Assign only real prerequisites to maximize parallel execution.
4. Build a dependency DAG and remove cycles before drafting. Ensure the graph is topologically sortable for execution order.
5. Draft concise context sections only when they improve implementation clarity.
6. Write each task and subtask as concrete implementation instructions.
7. Set initial states correctly:
   - New plan: set all tasks to `pending`.
   - Existing plan update: preserve truthful `completed` and `cancelled` states unless explicitly changed.
8. Run the validation checklist before returning output.
9. **Final scan:** re-read every `### Task` and `#### Subtask` heading in the output and confirm each is immediately followed by a `**State:**` line. If any task or subtask is missing `**State:**`, fix it before returning the plan. This is the most common defect — always perform this check last.

## Validation Checklist

Validate every response against all checks:

- Use one H1 and match `# Rhei: <title>`.
- If present, place `**States:** <state-machine-name>` as the first non-empty line after the H1, before any H2 section.
- Keep `## Tasks` present and last.
- Format every task as `### Task <id>: <title>`.
- Include `**State:**` on every task and every subtask with an allowed value.
- Place `**Prior:**` only after `**State:**` when present.
- Reference only existing tasks in each `**Prior:**` line.
- Keep dependencies acyclic.
- Keep ID style consistent across the document.
- Keep subtask numbering consistent with parent IDs and local order.
- Ensure each task has subtasks unless the task is clearly simple and non-decomposable.
- Emit no metadata fields beyond `**State:**` and `**Prior:**`.
- Keep heading levels strictly H1/H2/H3/H4 for plan title, sections, tasks, subtasks.

## File Extension

Save Rhei Plan documents with the `.rhei.md` extension, or `.md` when the context is clear.

## Editing Existing Rhei Plans

When modifying an existing Rhei Plan:

1. Preserve unchanged sections and task IDs.
2. Append new tasks using the existing ID style.
3. Update dependencies transitively when inserting or deleting tasks.
4. Do not reset `completed` tasks to `pending` unless explicitly requested.
5. Keep `## Tasks` as the final section after edits.

## Missing Information Handling

If required input is missing:

- Ask the user to provide all missing information.
- If the missing information is project-related, the user can instruct you to summon a researcher.

## Task Granularity

Right-sizing tasks is a balancing act across competing constraints:

- **Too large:** the implementing agent exhausts its context window before finishing.
- **Too small:** task-management overhead (transitions, re-reads, cold context) dominates useful work.
- **Right-sized:** a task fits comfortably in one agent session and produces a meaningful, reviewable unit of change. Subtasks should decompose work the agent can reuse context for — shared files, related functions, sequential build steps.

The state machine defines what happens at each stage of a task's lifecycle — read it before deciding granularity. A machine with heavyweight review gates (multi-agent review, human sign-off) justifies larger tasks to amortize that overhead. A lightweight machine (implement → done) allows smaller, more focused tasks. Match task size to the cost of moving through the states.

When a task is simple enough that subtasks would just be a checklist, omit subtasks and use inline TODO lists in the description instead.
