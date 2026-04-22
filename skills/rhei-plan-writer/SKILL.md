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

### Child Task Format

Decompose a task with nested `Task` nodes at a deeper heading level. Child
nodes use the same block shape as roots but with a dotted id that extends
the parent:

```markdown
#### Task <parent>.<child>: <title>
**State:** <state>

<description>
```

Apply these rules:
- **Every child task MUST have a `**State:**` field.** Same rule as for root tasks.
- Keep `**State:**` as the first line directly under the heading — no blank line between the heading and `**State:**`.
- The child id extends the parent id by exactly one new segment, separated by `.`: `1.1`, `1.2.3`, `api.cache`.
- Numeric children increment from `1` within their parent; named children use short identifiers.
- Sibling ids must be unique under the same parent.
- Default to adding child tasks whenever a task benefits from progressive disclosure and per-step logging. Skip them only when the work is clearly atomic.
- When skipping child tasks, make the task description explicit enough to act as a single implementation log entry.
- Heading depth is bounded by the plan's `structure.maxLevels` (default `2`, maximum `4`). H3 is depth 1, H4 is depth 2, H5 is depth 3, H6 is depth 4. A plan that needs more than two levels must declare `structure.maxLevels` in frontmatter.

### Node Kinds

By default the only declared node kind is `task` (rendered `Task` in headings). Plans that mix other kinds (for example bugs) declare them in frontmatter:

```markdown
---
structure:
  maxLevels: 3
  nodeKinds: [task, bug]
---
```

Once declared, a kind's title-cased form may appear as the heading keyword:

```markdown
#### Bug 1.2: Fix null-cache panic
**State:** pending
```

The keyword `rhei` is reserved and must not appear in `nodeKinds`.

## Planning Workflow

1. Extract deliverables, constraints, and sequencing needs from the request.
2. Decompose the work into independently completable tasks.
3. Assign only real prerequisites to maximize parallel execution.
4. Build a dependency DAG and remove cycles before drafting. Ensure the graph is topologically sortable for execution order.
5. Draft concise context sections only when they improve implementation clarity.
6. Write each task and child task as concrete implementation instructions.
7. Set initial states correctly:
   - New plan: set all tasks to `pending`.
   - Existing plan update: preserve truthful `completed` and `cancelled` states unless explicitly changed.
8. Run the validation checklist before returning output.
9. **Final scan:** re-read every `### Task`, `#### Task`, `##### Task`, or `###### Task` heading in the output and confirm each is immediately followed by a `**State:**` line. If any task is missing `**State:**`, fix it before returning the plan. This is the most common defect — always perform this check last.

## Validation Checklist

Validate every response against all checks:

- Use one H1 and match `# Rhei: <title>`.
- If present, place `**States:** <state-machine-name>` as the first non-empty line after the H1, before any H2 section.
- Keep `## Tasks` present and last.
- Format every root task as `### Task <id>: <title>`.
- Format every child task as `#### Task <parent>.<child>: <title>` (and deeper levels at H5/H6 when `structure.maxLevels` permits).
- Include `**State:**` on every task (root or child) with an allowed value.
- Place `**Prior:**` only after `**State:**` when present.
- Reference only existing tasks in each `**Prior:**` line.
- Keep dependencies acyclic.
- Keep ID style consistent across the document.
- Each child id extends its parent id by exactly one segment; sibling ids under the same parent are unique.
- Ensure each task has child tasks unless the task is clearly simple and non-decomposable.
- Emit no metadata fields beyond `**State:**` and `**Prior:**`.
- Heading depth must not exceed the plan's `structure.maxLevels` (default `2`, maximum `4`).

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

## Important: Task Granularity

Right-sizing tasks is a balancing act across competing constraints:

- **Too large:** the implementing agent exhausts its context window before finishing.
- **Too small:** task-management overhead (transitions, re-reads, cold context) dominates useful work.
- **Right-sized:** a task fits comfortably in one agent session and produces a meaningful, reviewable unit of change. Child tasks should decompose work the agent can reuse context for — shared files, related functions, sequential build steps.

The state machine defines what happens at each stage of a task's lifecycle — read it before deciding granularity. A machine with heavyweight review gates (multi-agent review, human sign-off) justifies larger tasks to amortize that overhead. A lightweight machine (implement → done) allows smaller, more focused tasks. Match task size to the cost of moving through the states.

When a task is simple enough that child tasks would just be a checklist, omit them and use inline TODO lists in the description instead.
