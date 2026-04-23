---
name: rhei-plan-writer
description: Create, refactor, and validate Rhei Plan markdown documents for project execution. Use when users ask for implementation plans, task breakdowns, roadmap-to-task conversion, dependency cleanup, or status updates in a strict, structured plan format with task states and prerequisites.
---

# Rhei Plan Writer

Produce a Rhei Plan markdown document that is deterministic, dependency-safe, and ready for execution.

## Output Contract

A Rhei Plan can be authored in two formats:

- **Single-File Plan** — one `.rhei.md` file containing the full task tree. Default for low-concurrency work.
- **Directory Workspace** — an `index.rhei.md` plus a `tasks/` directory of per-file task definitions. Use when multiple agents or humans will work the plan in parallel.

Default to Single-File unless the user asks for high concurrency or merge-conflict safety.

### Single-File Plan

- Emit exactly one H1: `# Rhei: <title>`.
- Optionally emit `**States:** <state-machine-name>` as the first non-empty line after the H1 to declare which state machine the plan follows. Omit to use the built-in `rhei` state machine.
- Optionally emit a YAML frontmatter block (see *Frontmatter*) after the `**States:**` field, before any H2 section.
- Emit zero or more contextual H2 sections before tasks.
- Emit `## Tasks` as the final H2 section.
- Emit at least one task under `## Tasks`.

### Directory Workspace

- Emit `index.rhei.md` at the workspace root: H1, optional `**States:**`, optional frontmatter, optional H2 context sections — **no** `## Tasks` section.
- Emit one or more task files under `tasks/`. Each file begins directly with `### <kind> <id>:` headers and contains no `# Rhei:` header and no independent frontmatter.
- Frontmatter (including `structure`, `metadata.tasks.*`) lives only in `index.rhei.md` — the workspace has exactly one authoritative metadata map.
- Prefer letter-prefixed or name-style IDs (e.g., `task-avatar`, `bug-null-cache`) over sequential numbers. Numeric IDs are safe in Single-File Plans but risk collisions in a distributed workspace.

### Task Block

Use this exact block shape for every task node (root and child):

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
- **Do not author `**Assignee:**` or `> **Result:**` blocks.** Both are runtime-owned: `rhei next` writes `**Assignee:**` when a task is claimed, `rhei complete` removes it and writes `> **Result:** [<id>](runtime/results/<id>.md)`. The plan writer never populates these fields.
- Separate metadata from description with a blank line.
- Emit no other metadata fields.
- Keep descriptions actionable and implementation-oriented.

### Allowed States

Run `rhei states` in the project to discover the allowed state values, their agent instructions, and the declared transitions for the state machine the plan will follow. Use `rhei states --state-machine <path>` to target a specific YAML file (for example, the one referenced by a plan's `**States:**` line), and `rhei states --json` when machine-readable output is preferred. Use only state values reported by that command, follow the printed instructions when describing task work, and respect the declared transitions when choosing initial states.

If the `rhei` CLI is unavailable in the project, fall back to reading the state machine YAML file directly (typically `docs/states.yaml`, or the file referenced by `**States:**`). If the project does not define its own state machine, fall back to the default Rhei state set documented in [default-states.md](references/default-states.md).

Each node's initial state comes from the machine's `profiles.<name>.initial` via `node_policy` — **not** from a state-level `initial: true` flag. In the built-in `rhei` machine, the initial state is `draft`. When authoring a new plan under that machine, every task starts in `draft`.

For markdown safety, format state names containing hyphens, spaces, or punctuation with backticks:

```markdown
**State:** `agent-review`
**State:** `human review`
**State:** `security.review-2`
```

Backticks are acceptable for all state values when they improve consistency. Canonical names that match `IDENTIFIER` exactly (e.g. `draft`, `pending`) may be written bare.

For state machines that declare a `visits` budget on a state, a counted-visit suffix (`-<n>`, with `n >= 2`) may appear in the rendered state value for later visits. The plan writer normally does not author these — they accumulate at runtime.

### ID Policy

- Choose exactly one ID style per document.
- Numeric style: `1`, `2`, `3`, ...
- Named style: `setup`, `review`, `api`, ...
- Prefer numeric IDs unless the plan is small and conceptual, or the plan is a Directory Workspace (prefer named IDs there to avoid collisions).
- Do not mix styles in one document.

### Child Task Format

Decompose a task with nested `Task` nodes at a deeper heading level. Child nodes use the same block shape as roots but with a dotted id that extends the parent:

```markdown
#### Task <parent>.<child>: <title>
**State:** <state>

<description>
```

Apply these rules:
- **Every child task MUST have a `**State:**` field.** Same rule as for root tasks.
- Keep `**State:**` as the first line directly under the heading — no blank line between the heading and `**State:**`.
- The child id extends the parent id by exactly one new segment, separated by `.`: `1.1`, `1.2.3`, `api.cache`.
- Numeric children increment from `1` within their parent; named children use short identifiers. Mixed numeric/named segments are allowed as long as depth matches.
- Sibling ids must be unique under the same parent.
- Default to adding child tasks whenever a task benefits from progressive disclosure and per-step logging. Skip them only when the work is clearly atomic.
- When skipping child tasks, make the task description explicit enough to act as a single implementation log entry.
- Heading depth is bounded by the plan's `structure.maxLevels` (default `2`, maximum `4`). H3 is depth 1, H4 is depth 2, H5 is depth 3, H6 is depth 4. A plan that needs more than two levels must declare `structure.maxLevels` in frontmatter.

### Frontmatter

Emit a YAML frontmatter block only when the plan needs non-default `structure` settings. The `structure` block is the only thing a plan writer should author in frontmatter; runtime-managed keys under `metadata.tasks.*` (such as `stateVisits` counters) are written by the CLI and must not be hand-authored.

```markdown
---
structure:
  maxLevels: 3
  nodeKinds: [task, bug]
---
```

- `structure.maxLevels` — maximum task depth, from `1` (`###` only) through `4` (`######` allowed). Default `2`. Required when any child task would exceed depth 2.
- `structure.nodeKinds` — allowed heading keywords. Default `[task]`. Add other kinds (`bug`, `spike`, `epic`, ...) only when the plan actually uses them. The keyword `rhei` is reserved and must never appear in `nodeKinds`.

Once a kind is declared, its title-cased form may appear as the heading keyword:

```markdown
#### Bug 1.2: Fix null-cache panic
**State:** pending
```

## Planning Workflow

1. Extract deliverables, constraints, and sequencing needs from the request.
2. Decompose the work into independently completable tasks.
3. Assign only real prerequisites to maximize parallel execution.
4. Build a dependency DAG and remove cycles before drafting. Ensure the graph is topologically sortable for execution order.
5. Draft concise context sections only when they improve implementation clarity.
6. Write each task and child task as concrete implementation instructions.
7. Set initial states correctly:
   - New plan: set every task to the active machine's profile `initial` (`draft` for the built-in machine).
   - Existing plan update: preserve truthful terminal states (`completed`, `cancelled`) unless explicitly changed, and preserve any `**Assignee:**` / `> **Result:**` blocks that the runtime has written.
8. Run the validation checklist before returning output.
9. **Final scan:** re-read every `### Task`, `#### Task`, `##### Task`, or `###### Task` heading (or other declared kinds) in the output and confirm each is immediately followed by a `**State:**` line. If any task is missing `**State:**`, fix it before returning the plan. This is the most common defect — always perform this check last.

## Validation Checklist

Validate every response against all checks:

- Use one H1 and match `# Rhei: <title>` (Single-File Plan) or `index.rhei.md` (Directory Workspace).
- If present, place `**States:** <state-machine-name>` as the first non-empty line after the H1, before any frontmatter or H2 section.
- If present, place YAML frontmatter between the `**States:**` line and the first H2 section.
- Keep `## Tasks` present and last in Single-File Plans; omit it entirely from `index.rhei.md` in Directory Workspaces.
- Format every root task node as `### <Kind> <id>: <title>`.
- Format every child task node as `#### <Kind> <parent>.<child>: <title>` (and deeper levels at H5/H6 when `structure.maxLevels` permits).
- Include `**State:**` on every task (root or child) with an allowed value from the resolved profile.
- Place `**Prior:**` only after `**State:**` when present.
- Do not author `**Assignee:**` or `> **Result:**` — these are runtime-owned.
- Reference only existing tasks in each `**Prior:**` line. In a Directory Workspace, references resolve across the merged workspace graph.
- Keep dependencies acyclic. A task must not self-reference.
- Keep ID style consistent across the document.
- Each child id extends its parent id by exactly one segment; sibling ids under the same parent are unique.
- Ensure each task has child tasks unless the task is clearly simple and non-decomposable.
- Emit no metadata fields beyond `**State:**` and `**Prior:**`.
- Heading depth must not exceed the plan's `structure.maxLevels` (default `2`, maximum `4`).
- If mixed kinds are used, every heading keyword appears in `structure.nodeKinds`; `rhei` never appears there.

When the CLI is available, run `rhei validate <plan>` after writing — it performs the full grammar, state, dependency, link, and terminal-coherence checks the checklist only approximates.

## File Extension

Save Rhei Plan documents with the `.rhei.md` extension, or `.md` when the context is clear. The Directory Workspace root file is always `index.rhei.md`.

## Editing Existing Rhei Plans

When modifying an existing Rhei Plan:

1. Preserve unchanged sections, task IDs, frontmatter, `**Assignee:**` lines, and `> **Result:**` blocks.
2. Append new tasks using the existing ID style.
3. Update dependencies transitively when inserting or deleting tasks.
4. Do not reset `completed` or `cancelled` tasks unless explicitly requested. These are treated as immutable by the worker.
5. Keep `## Tasks` as the final section after edits (Single-File Plans).
6. Run `rhei validate` after editing.

## Missing Information Handling

If required input is missing:

- Ask the user to provide all missing information.
- If the missing information is project-related, the user can instruct you to summon a researcher.

## Important: Task Granularity

Right-sizing tasks is a balancing act across competing constraints:

- **Too large:** the implementing agent exhausts its context window before finishing.
- **Too small:** task-management overhead (transitions, re-reads, cold context) dominates useful work.
- **Right-sized:** a task fits comfortably in one agent session and produces a meaningful, reviewable unit of change. Child tasks should decompose work the agent can reuse context for — shared files, related functions, sequential build steps.

The state machine defines what happens at each stage of a task's lifecycle — read it before deciding granularity. A machine with heavyweight review gates (multi-agent review, human sign-off, multi-team handoffs) justifies larger tasks to amortize that overhead. A lightweight machine (implement → done) allows smaller, more focused tasks. Match task size to the cost of moving through the states.

When a task is simple enough that child tasks would just be a checklist, omit them and use inline TODO lists in the description instead.
