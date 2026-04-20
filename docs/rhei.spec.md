# Rhei Plan Language Specification

This document defines the formal grammar and semantics of the Rhei Plan language, a structured subset of Markdown for hierarchical task management.

## Overview

The Rhei Plan language is a **context-sensitive subset of Markdown** designed for:

- AI agent memory
- AI agent coordination
- Human oversight of work progress

## Quick Start

### 1. Write a plan

Create a `.rhei.md` file. At minimum, a plan needs a title, a `## Tasks` section, and at least one task with a `**State:**` field:

```markdown
# Rhei: Add user avatars

## Tasks

### Task 1: Add avatar column to users table
**State:** pending

Add a nullable `avatar_url` column to the `users` table and generate a migration.

### Task 2: Build upload endpoint
**State:** pending
**Prior:** Task 1

Create a POST /api/users/:id/avatar endpoint that accepts an image, stores it in S3, and writes the URL to the new column.

### Task 3: Display avatars in the UI
**State:** pending
**Prior:** Task 2

Render the avatar in the profile header and comment list. Fall back to initials when no avatar is set.
```

`**Prior:**` declares dependencies — Task 2 cannot be claimed until Task 1 is in a terminal state as defined by the active state machine (`final: true`; in the built-in `rhei` machine, `completed` and `cancelled`). Tasks without `**Prior:**` are immediately dependency-ready.

### 2. Validate the plan

```bash
rhei validate plan.rhei.md
```

The CLI checks syntax, state validity, dependency integrity, and that the dependency graph has no cycles.
When the active state machine declares artifact contracts, runtime commands also
enforce required state inputs and outputs as described in the states and
transitions specifications.

### 3. Have an agent execute it

Tell an agent with the `rhei-plan-worker` skill to work the plan:

```bash
/rhei-plan-worker plan.rhei.md
```

The worker reads the plan, loads the state machine, and enters a loop: claim the next eligible task with `rhei next`, work in that task's current state, use `rhei transition` when the workflow requires an explicit state change (for example `draft` to `pending`), finish with `rhei complete` when the task reaches a terminal outcome, and repeat. It stops when no eligible tasks remain or a gating state such as `human-review` requires a human decision.

The plan file is the single source of truth — multiple agents or humans can read it to see what is done, what is in progress, and what is blocked.

For programmatic execution with `rhei run`, see [Pattern 6: Programmatic State Transitions](specs/rhei-usage.spec.md#pattern-6-programmatic-state-transitions).

## Plan Formats

A Rhei plan can be authored as either a **Single-File Plan** or a **Directory Workspace**.

### Single-File Plan (1 Agent, or low concurrency)

The single-file format is a fixed hierarchical structure:

| Component | Heading Level | Format | Required |
|-----------|---------------|--------|----------|
| Rhei Title | H1 (`#`) | `# Rhei: <title>` | Yes |
| States Declaration | — | `**States:** <state-machine-name>` | No (defaults to `rhei`) |
| Content Sections | H2 (`##`) | `## <section-name>` | No |
| Tasks Section | H2 (`##`) | `## Tasks` | Yes |
| Task | H3 (`###`) | `### Task <id>: <title>` | Yes (at least one) |
| Subtask | H4 (`####`) | `#### Subtask <n>.<m>: <title>` | No |

When present, the `**States:**` field must be the first non-empty line after the `# Rhei:` title. Its value is the `name` of the state machine defined in the associated states configuration (see [States Specification](specs/rhei-states.spec.md)). For a single-file plan, the CLI resolves that configuration from a sibling `states.yaml`. For a directory workspace, it resolves from `<workspace>/states.yaml`. The resolved YAML file's `name` must match the declared `**States:**` value. `--state-machine <path>` overrides this automatic lookup. When the field is omitted, the plan uses the built-in `rhei` state machine.

### Directory Workspace (Agent Teams, High Concurrency)

To prevent Git merge conflicts when multiple agents or humans work in parallel across disparate branches, a Rhei plan can be structured as a directory. This functions similarly to distributed issue trackers.

A Directory Workspace consists of:

1. **`index.rhei.md`**: The root configuration. Contains the `Rhei Title`, `States Declaration`, and any `Content Sections`. It does **not** contain a `## Tasks` section.
2. **`tasks/` directory**: A folder containing arbitrary `.md` files.
3. **Workspace Task Files**: Files within `tasks/` that contain one or more `Task` definitions (starting directly with `### Task <id>:`). They do not require the `# Rhei:` header.

In a Directory Workspace, all tasks are parsed and merged into a single global task graph at runtime. Dependency validation (`**Prior:**`) resolves globally across all files in the `tasks/` directory.

To prevent creation collisions in highly distributed swarms, letter-prefixed
`IDENTIFIER` values rather than sequential `NUMBER` task ids are strongly
recommended for Directory Workspaces. Because the grammar requires an
`IDENTIFIER` to start with a letter, distributed ids should use forms such as
`task-550e8400-e29b-41d4-a716-446655440000` rather than a bare UUID or hash.

### Directory Workspace Metadata

YAML frontmatter for a Directory Workspace belongs in `index.rhei.md`. Workspace
task files start directly with task definitions and must not introduce
independent frontmatter blocks, so the workspace has exactly one authoritative
`metadata.tasks.<id>` map.

Runtime-managed metadata that is defined in the transitions specification, such
as `metadata.tasks.<id>.stateVisits.<state-name>`, is therefore read from and
written to the frontmatter in `index.rhei.md`, keyed by the global task id.

Persistence ownership is normative:

- Markdown task fields remain the source of truth for `**State:**`,
  `**Prior:**`, `**Assignee:**`, and `> **Result:**`. In a Directory
  Workspace, those fields are persisted in the workspace task file that
  contains the task.
- YAML frontmatter under `metadata.tasks.<id>.*` stores auxiliary per-task
  metadata only, such as `stateVisits` counters and custom callback data. It
  must not become a second source of truth for state, dependencies, assignee,
  or result links.
- Runtimes may project the current markdown assignee into callback-facing APIs
  such as `task.metadata.assignee` for convenience, but that value remains a
  view over the markdown `**Assignee:**` line rather than a separately
  persisted frontmatter field.

This keeps task descriptions, `**Assignee:**` changes, and `> **Result:**`
blocks localized to task files, which preserves most of the concurrency benefit
of the workspace format. However, features that persist data through
frontmatter-backed task metadata still serialize through `index.rhei.md`, so
metadata-heavy workflows reintroduce a narrow shared-write hotspot until a
workspace-local metadata format is specified.

## Grammar (EBNF)

```ebnf
(* ============================================== *)
(* DOCUMENT STRUCTURE                             *)
(* ============================================== *)

rhei_document   = rhei_header, { blank_line },
                  [ states_field, { blank_line } ],
                  [ frontmatter, { blank_line } ],
                  { content_section },
                  tasks_section ;

rhei_header     = "# Rhei: ", title, NEWLINE ;

states_field    = "**States:** ", state_machine_name, NEWLINE ;

state_machine_name = title ;

content_section = "## ", section_title, NEWLINE, { markdown_block } ;

tasks_section   = "## Tasks", NEWLINE, { blank_line }, task, { task } ;

(* ============================================== *)
(* DIRECTORY WORKSPACE STRUCTURE                  *)
(* ============================================== *)

workspace_index = rhei_header, { blank_line },
                  [ states_field, { blank_line } ],
                  [ frontmatter, { blank_line } ],
                  { content_section } ;

workspace_task_file = [ { blank_line } ], task, { task } ;


(* ============================================== *)
(* YAML FRONTMATTER (optional metadata)           *)
(* ============================================== *)

(* YAML frontmatter stores custom task metadata such as retryCount,
   stateVisits, priority, and callback data. Core task fields such as state,
   prior, assignee, and result links remain in markdown. It appears between two
   `---` fences and is parsed as YAML, not Markdown. See the Transitions
   Specification for the metadata access contract. *)
frontmatter     = "---", NEWLINE,
                  { yaml_line },
                  "---", NEWLINE ;

yaml_line       = ? any line that is not exactly "---" ?, NEWLINE ;


(* ============================================== *)
(* TASK DEFINITION                                *)
(* ============================================== *)

task            = task_header, NEWLINE, metadata, { task_markdown_block },
                  [ result_block ], { subtask } ;

task_header     = "### Task ", task_id, ": ", title ;

task_id         = NUMBER | IDENTIFIER ;


(* ============================================== *)
(* SUBTASK DEFINITION                             *)
(* ============================================== *)

(* Subtasks are only permitted under tasks with a numeric task_id.
   They are checklist items and do not carry task metadata such as state. *)
subtask         = subtask_header, NEWLINE, { markdown_block } ;

subtask_header  = "#### Subtask ", NUMBER, ".", NUMBER, ": ", title ;


(* ============================================== *)
(* METADATA FIELDS                                *)
(* ============================================== *)

(* State field is mandatory and must appear first.
   Assignee is optional; the `complete` command strips it on completion. *)
metadata        = state_field, [ prior_field ], [ assignee_field ] ;

assignee_field  = "**Assignee:** ", title, NEWLINE ;

(* Result block links to the outcome of a completed task. It is inserted
   by the `complete` command after task content and before subtasks.
   The link text is the task id itself, and the target is always
   runtime/results/<task-id>.md. *)
result_block    = "> **Result:** ", "[", task_id, "](", result_path, ")", NEWLINE ;

result_path     = "runtime/results/", task_id, ".md" ;

state_field     = "**State:** ", state_value, NEWLINE ;

prior_field     = "**Prior:** ", task_ref_list, NEWLINE ;

task_ref_list   = task_ref, { ", ", task_ref } ;

task_ref        = "Task ", task_id ;

(* State values have two rendered forms:
   - bare form for canonical names that match IDENTIFIER exactly
   - backticked form for any other canonical state name, including names
     with spaces or punctuation such as `human review`, `qa/review`, or
     `security.review`
   Counted-loop states may append `-<n>` to the rendered value to make later
   visits visible in markdown. The `-1` suffix is omitted. Exact-match
   resolution against loaded state names happens before any `-<n>` suffix is
   interpreted as a counted visit. Inside backticks, `\\` escapes a
   backslash and `\`` escapes a literal backtick. *)
state_value     = IDENTIFIER, [ "-", NUMBER ]                    (* pending, review-2 *)
                | "`", quoted_state_text, "`" ;                 (* `human review`, `qa/review`, `human review-2` *)

quoted_state_text = quoted_state_char, { quoted_state_char } ;

quoted_state_char = ESCAPED_BACKSLASH
                  | ESCAPED_BACKTICK
                  | ? any Unicode character except NEWLINE, "`", and "\" ? ;


(* ============================================== *)
(* TERMINALS                                      *)
(* ============================================== *)

title           = { ANY_CHAR - NEWLINE }+ ;

(* Section titles must not equal "Tasks" — that prefix is reserved
   for the tasks_section. *)
section_title   = { ANY_CHAR - NEWLINE }+ - "Tasks" ;

(* A task_markdown_block is ordinary task body content. The `> **Result:** `
   prefix is reserved for the dedicated result_block production so the grammar
   can distinguish result metadata from freeform prose. *)
task_markdown_block = ( blank_line
                      | task_body_line, NEWLINE ) ;

(* A markdown_block is any non-structural content line for use in sections and
   subtasks. Blank lines are markdown_blocks. *)
markdown_block  = ( blank_line
                  | non_structural_line, NEWLINE ) ;

task_body_line  = ? any line that does not match a header production above
                   and does not begin with "> **Result:** " ? ;

non_structural_line = ? any line that does not match a header
                        production above ? ;

blank_line      = NEWLINE ;

NUMBER          = DIGIT, { DIGIT } ;

DIGIT           = "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" ;

IDENTIFIER      = LETTER, { LETTER | DIGIT | "-" | "_" } ;

LETTER          = "a" | "b" | ... | "z" | "A" | "B" | ... | "Z" ;

ANY_CHAR        = ? any Unicode character ? ;

ESCAPED_BACKSLASH = "\\\\" ;

ESCAPED_BACKTICK = "\\`" ;

NEWLINE         = ? line terminator (LF or CRLF) ? ;
```

## Semantic Constraints

Beyond the syntactic rules, the following semantic constraints must be validated:

The grammar tolerates optional blank lines immediately after `## Tasks` in a
single-file plan and at the start of a workspace task file. Implementations
should accept those blank lines in both formats.

Throughout this specification, a *terminal state* means any state marked
`final: true` in the active state machine. In the built-in `rhei` machine, the
terminal states are `completed` and `cancelled`.

Dependency readiness is defined by that terminal-state rule alone: a task is
ready with respect to `**Prior:**` only when every referenced dependency is in
a terminal state. State-machine `instructions` text is descriptive guidance for
agents and must not narrow or override this readiness rule unless the machine
introduces a separate normative field for that purpose.

### 1. Dependency Integrity

All task references in `**Prior:**` fields must resolve to existing tasks in the same logical plan: in a Single-File Plan that means the same document, and in a Directory Workspace that means the merged workspace graph across all task files under `tasks/`. A `**Prior:**` list must not contain duplicate references and must not reference its own task (self-reference is a 1-cycle).

```markdown
### Task 2: Implementation
**State:** pending
**Prior:** Task 1, Task 3    ← Task 1 and Task 3 must exist
```

Directory Workspace example:

```markdown
# tasks/backend.md
### Task api: Build API
**State:** pending

# tasks/frontend.md
### Task ui: Build UI
**State:** pending
**Prior:** Task api    ← Valid: resolves across the merged workspace graph
```

### 2. State Validity

All state values must be defined in the associated states configuration. By default, that configuration is loaded from the plan's auto-discovered `states.yaml` when `**States:**` is declared, or from the built-in `rhei` state machine when it is omitted. `--state-machine <path>` may override the auto-discovered file.

State-name rendering is normative across the plan and state-machine specs:

- A canonical state name that matches `IDENTIFIER` exactly may be written bare:
  `**State:** pending`.
- Any canonical state name containing whitespace or any character outside
  `IDENTIFIER` must be written in backticks, using the backticked forms shown
  in the examples below.
- Inside the backticked form, `\\` encodes a literal backslash and `\`` encodes
  a literal backtick.
- When related specs describe artifact or transition state names as arbitrary
  YAML strings, this markdown encoding is the corresponding representation in a
  plan file.

When a state machine state declares `visits: <n>`, the authored markdown may
encode later counted visits directly in `**State:**` using a `-<visit>` suffix:

```markdown
**State:** review      ← first visit (implicit)
**State:** review-2    ← second visit
**State:** `human review-3`  ← third visit for a spaced state name
```

The canonical state machine state is the unsuffixed base name (`review`,
`human review`). The suffix is only valid for states that declare `visits`, must
be greater than `1`, and must not exceed the declared visit budget.

Parsing rule for ambiguous names:

1. Remove optional surrounding backticks and first try to match the rendered
   value exactly against a loaded state name.
2. Only if no exact match exists may an implementation interpret a trailing
   `-<digits>` suffix as a counted visit.
3. In that case, the unsuffixed base name must itself exactly match a loaded
   state name, and the parsed visit count becomes the suffix value.

This means a machine that defines a literal state named `review-2` treats
`**State:** review-2` as that canonical state on visit 1. It is only parsed as
"state `review`, visit 2" when no literal `review-2` state exists.

Additional examples:

```markdown
**State:** pending                 ← canonical state `pending`
**State:** `human review`          ← canonical state `human review`
**State:** `qa/review`             ← canonical state `qa/review`
**State:** `security.review-2`     ← canonical state `security.review`, visit 2
**State:** `review-2`              ← either literal state `review-2` or, if absent, state `review` visit 2
```

### 3. Acyclic Dependencies

The task dependency graph must be a Directed Acyclic Graph (DAG). Circular dependencies are forbidden:

```markdown
### Task 1: First
**State:** pending
**Prior:** Task 2    ← ERROR: creates cycle

### Task 2: Second
**State:** pending
**Prior:** Task 1    ← ERROR: creates cycle
```

### 4. Subtask Numbering Consistency

Subtasks are only permitted under tasks with a numeric `task_id`. The first number of every subtask must equal its parent task's id, and subtask numbers must be unique within their parent. Subtasks do not carry independent `**State:**` metadata; they are lightweight checklist items nested under the parent task:

```markdown
### Task 2: Parent Task
**State:** pending

#### Subtask 2.1: Valid
Document the database migration steps.    ← Correct: parent is Task 2

#### Subtask 3.1: Invalid
Document the rollback path.               ← ERROR: parent task number mismatch

#### Subtask 2.1: Duplicate
Add verification notes.                   ← ERROR: duplicate subtask number under Task 2
```

A task with a named (non-numeric) `task_id` must not declare any subtasks.

### 5. Identifier Uniqueness

Task ids must be unique across the entire plan. In a Single-File Plan, two `### Task <id>:` headers with the same id are an error. In a Directory Workspace, two tasks with the same id across *any* files within the `tasks/` directory are an error.

### 6. Link Integrity

All relative markdown links (`[text](target)`) in content sections, task
content, and subtask content must resolve to existing files. In a Single-File
Plan, links resolve relative to the directory containing the plan file. In a
Directory Workspace, they resolve relative to the workspace root, meaning the
directory containing `index.rhei.md`, even when the link appears in a nested
file under `tasks/`.

External URLs (`http://`, `https://`, `mailto:`), and fragment-only anchors (`#section`) are not checked. When a link contains a fragment (`file.md#section`), only the file portion is verified.

For Directory Workspaces, implementations must not resolve `./` or `../`
against the physical path of the task file that contains the link. This keeps
links stable when tasks move between files or when task files are nested under
`tasks/`.

For workspace-relative validation, implementations must join the relative
target to the workspace root and then normalize `.` and `..` path segments. If
the normalized path escapes the workspace root, the link is invalid.

```markdown
## Overview
See [the spec](specs/language.md) for details.    ← specs/language.md must exist

### Task 1: Setup
**State:** pending

Read [guide](https://example.com/guide)           ← OK: external URL, not checked
See [section](#overview)                           ← OK: fragment-only, not checked
See [missing](docs/nonexistent.md)                 ← ERROR: file does not exist
```

Directory Workspace example with a nested task file:

```markdown
# <workspace>/tasks/backend/api.md
### Task api: Build API
**State:** pending

See [guide](./docs/guide.md)        ← resolves to <workspace>/docs/guide.md
See [shared](shared.md)             ← resolves to <workspace>/shared.md
See [protocol](specs/http.md#post)  ← checks <workspace>/specs/http.md only
See [notes](#api-notes)             ← fragment-only anchor, not checked
```

### 7. Result Block Consistency

When a task contains a `> **Result:**` block, that block must describe the
enclosing task itself:

- The link text must equal the enclosing task's `task_id`.
- The target path must be exactly `runtime/results/<task-id>.md` using that
  same id.

Example:

```markdown
### Task api: Build API
**State:** completed
> **Result:** [api](runtime/results/api.md)    ← Valid

### Task ui: Build UI
**State:** completed
> **Result:** [api](runtime/results/api.md)    ← ERROR: references a different task id
```

`result_block` is validated by this task-local rule rather than by the general
link-integrity check above. The file may be created later by runtime commands
such as `rhei complete`.

### 8. State Artifact Contracts

The active state machine may declare required file `inputs` and `outputs` for a
state. These contracts are part of execution semantics, not markdown syntax:

- Entering a state may require one or more input files to already exist.
- Leaving a state may require one or more output files to have been written.
- Artifact paths are resolved relative to the plan root (single-file plan) or
  workspace root (directory workspace).

This section is normative for artifact-path resolution across the Rhei spec
set. The execution root is defined as:

- The directory containing the `.rhei.md` plan file for a Single-File Plan.
- The directory containing `index.rhei.md` for a Directory Workspace.

When related specs describe artifact `path` values as "workspace-relative"
templates, they mean relative to this execution root. In other words, the same
artifact template is interpreted relative to the single-file plan directory in
single-file mode and relative to the workspace root in directory-workspace
mode.

Because artifact existence depends on runtime workspace state, this constraint
is enforced by execution commands such as `rhei transition`, `rhei complete`,
`rhei run`, and `rhei next`, rather than by pure syntax validation of markdown
alone.

Examples:

```markdown
# Single-file plan at /repo/plans/release.rhei.md
Artifact path: runtime/reviews/release.md
Resolves to: /repo/plans/runtime/reviews/release.md

# Directory workspace at /repo/release/index.rhei.md
Artifact path: runtime/reviews/release.md
Resolves to: /repo/release/runtime/reviews/release.md
```

## Token Types

This section is illustrative and non-normative. A complete implementation must
support every normative grammar production above, including YAML frontmatter,
`**Assignee:**`, and `> **Result:**` blocks.

For lexer implementation, the following token types are a reasonable minimum:

| Token | Pattern | Example |
|-------|---------|---------|
| `RheiHeader` | `# Rhei: .*` | `# Rhei: My Project` |
| `MetadataStates` | `\*\*States:\*\* .*` | `**States:** rhei` |
| `FrontmatterFence` | `^---\s*$` | `---` |
| `FrontmatterYamlLine` | Any line inside frontmatter that is not `---` | `metadata:` |
| `TasksSection` | `^## Tasks\s*$` | `## Tasks` |
| `SectionHeader` | `^## .+$` (matched only if `TasksSection` did not match) | `## Overview` |
| `TaskHeader` | `### Task <id>: .*` | `### Task 1: Setup` |
| `SubtaskHeader` | `#### Subtask <n>.<m>: .*` | `#### Subtask 1.2: Config` |
| `MetadataState` | `\*\*State:\*\* .*` | `**State:** pending` |
| `MetadataPrior` | `\*\*Prior:\*\* .*` | `**Prior:** Task 1` |
| `MetadataAssignee` | `\*\*Assignee:\*\* .*` | `**Assignee:** alice` |
| `ResultBlock` | `^> \*\*Result:\*\* \[[^]]+\]\([^)]+\)\s*$` | `> **Result:** [task-1](runtime/results/task-1.md)` |
| `Text` | Any other line | Description text |

## AST Node Types

This section is also illustrative and non-normative. It shows one viable shape
for a parser AST, but it is not a complete or exclusive contract.

For parser implementation, the following AST structure is recommended:

```rust
struct Rhei {
    title: String,
    states: String, // state machine name; defaults to "rhei" when omitted
    frontmatter: Option<YamlValue>,
    content_sections: Vec<ContentSection>,
    tasks: Vec<Task>,
}

struct ContentSection {
    title: String,
    content: String,
}

struct Task {
    id: TaskId,
    title: String,
    state: String,
    prior: Vec<TaskId>,
    assignee: Option<String>,
    content: String,
    result: Option<ResultLink>,
    subtasks: Vec<Subtask>,
}

struct Subtask {
    task_number: u32,
    subtask_number: u32,
    title: String,
    content: String,
}

enum TaskId {
    Number(u32),
    Named(String),
}

struct ResultLink {
    task_id: TaskId,
    path: String,
}
```

## Language Classification

The Rhei Plan language is **context-sensitive** because:

1. Subtask numbers depend on their parent task context
2. `Prior` references must resolve to existing task definitions
3. State values depend on external states configuration
4. State artifact contracts depend on external workspace files at execution time

The language cannot be fully described by a context-free grammar alone; semantic analysis is required for complete validation.

## File Extension

The recommended file extension for Rhei Plan documents is `.rhei.md` or simply `.md` when the context is clear.

## CLI Command Groups

The `rhei` CLI help currently organizes its subcommands into five groups:

| Group | Commands | Purpose |
| --- | --- | --- |
| **Inspection** | `validate`, `render`, `states` | Read-only commands that examine or render a plan without modifying it |
| **Templates** | `templates`, `instantiate` | Discover and instantiate reusable plan and workspace templates |
| **Execution** | `transition`, `run`, `next`, `complete`, `reset` | Commands that mutate the plan file or workspace state |
| **Setup** | `install-skills` | Install packaged Rhei skills for supported agent environments |
| **Info** | `version`, `help` | Meta commands about the tool itself |

## Related Specifications

- [How Rhei Is Used](specs/rhei-usage.spec.md) - Roles, coordination patterns, and agent workflows
- [Plan Language Usage Guide](specs/rhei-authoring.spec.md) - Practical authoring patterns and walkthroughs
- [States Specification](specs/rhei-states.spec.md) - Defines the states configuration format
- [Transitions Specification](specs/rhei-transitions.spec.md) - Formal state transition system, callbacks, and YAML schema
- [Next Command](specs/rhei-next.spec.md) - `rhei next` behavioral contract, including `--peek` mode
- [Complete Command](specs/rhei-complete.spec.md) - `rhei complete` behavioral contract
- [State Machine Writer](specs/rhei-state-machine-writer.spec.md) - Designing custom state machines from project specs and teams
