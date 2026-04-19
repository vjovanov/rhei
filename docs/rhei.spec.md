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

`**Prior:**` declares dependencies — Task 2 cannot start until Task 1 is completed. Tasks without `**Prior:**` are immediately eligible.

### 2. Validate the plan

```bash
rhei validate plan.rhei.md
```

The CLI checks syntax, state validity, dependency integrity, and that the dependency graph has no cycles.

### 3. Have an agent execute it

Tell an agent with the `rhei-plan-worker` skill to work the plan:

```bash
/rhei-plan-worker plan.rhei.md
```

The worker reads the plan, loads the state machine, and enters a loop: pick the next eligible task (all priors completed, state is `pending`), transition it to `in-progress`, implement it, advance through review states, and repeat. It stops when no eligible tasks remain or a `human-review` gate requires a human decision.

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

When present, the `**States:**` field must be the first non-empty line after the `# Rhei:` title. Its value is the `name` of the state machine defined in the associated states configuration (see [States Specification](specs/rhei-states.spec.md)). When omitted, the plan uses the built-in `rhei` state machine.

### Directory Workspace (Agent Teams, High Concurrency)

To prevent Git merge conflicts when multiple agents or humans work in parallel across disparate branches, a Rhei plan can be structured as a directory. This functions similarly to distributed issue trackers.

A Directory Workspace consists of:

1. **`index.rhei.md`**: The root configuration. Contains the `Rhei Title`, `States Declaration`, and any `Content Sections`. It does **not** contain a `## Tasks` section.
2. **`tasks/` directory**: A folder containing arbitrary `.md` files.
3. **Workspace Task Files**: Files within `tasks/` that contain one or more `Task` definitions (starting directly with `### Task <id>:`). They do not require the `# Rhei:` header.

In a Directory Workspace, all tasks are parsed and merged into a single global task graph at runtime. Dependency validation (`**Prior:**`) resolves globally across all files in the `tasks/` directory.

To prevent creation collisions in highly distributed swarms, relying on `IDENTIFIER` (alphanumeric hashes or UUIDs) rather than sequential `NUMBER` for `task_id` is strongly recommended for Directory Workspaces.

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

state_machine_name = IDENTIFIER ;

content_section = "## ", section_title, NEWLINE, { markdown_block } ;

tasks_section   = "## Tasks", NEWLINE, task, { task } ;

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
   priority, assignee, etc. It appears between two `---` fences and
   is parsed as YAML, not Markdown. See the Transitions Specification
   for the metadata access contract. *)
frontmatter     = "---", NEWLINE,
                  { yaml_line },
                  "---", NEWLINE ;

yaml_line       = ? any line that is not exactly "---" ?, NEWLINE ;


(* ============================================== *)
(* TASK DEFINITION                                *)
(* ============================================== *)

task            = task_header, NEWLINE, metadata, { markdown_block },
                  [ result_block ], { subtask } ;

task_header     = "### Task ", task_id, ": ", title ;

task_id         = NUMBER | IDENTIFIER ;


(* ============================================== *)
(* SUBTASK DEFINITION                             *)
(* ============================================== *)

(* Subtasks are only permitted under tasks with a numeric task_id. *)
subtask         = subtask_header, NEWLINE, state_field, { markdown_block } ;

subtask_header  = "#### Subtask ", NUMBER, ".", NUMBER, ": ", title ;


(* ============================================== *)
(* METADATA FIELDS                                *)
(* ============================================== *)

(* State field is mandatory and must appear first.
   Assignee is optional; the `complete` command strips it on completion. *)
metadata        = state_field, [ prior_field ], [ assignee_field ] ;

assignee_field  = "**Assignee:** ", title, NEWLINE ;

(* Result block records the outcome of a completed task. It is inserted
   by the `complete` command after task content and before subtasks.
   It is free-form content rendered as a Markdown blockquote. *)
result_block    = "> **Result:** ", title, NEWLINE,
                  { "> ", non_structural_line, NEWLINE } ;

state_field     = "**State:** ", state_value, NEWLINE ;

prior_field     = "**Prior:** ", task_ref_list, NEWLINE ;

task_ref_list   = task_ref, { ", ", task_ref } ;

task_ref        = "Task ", task_id ;

(* The backtick form is required when the state value contains
   whitespace; it is also accepted (but not required) for single-word
   values. *)
state_value     = IDENTIFIER                           (* single word: pending *)
                | "`", IDENTIFIER, { " ", IDENTIFIER }, "`" ;  (* escaped: `in progress` *)


(* ============================================== *)
(* TERMINALS                                      *)
(* ============================================== *)

title           = { ANY_CHAR - NEWLINE }+ ;

(* Section titles must not equal "Tasks" — that prefix is reserved
   for the tasks_section. *)
section_title   = { ANY_CHAR - NEWLINE }+ - "Tasks" ;

(* A markdown_block is any line that does not introduce a new
   structural element (i.e. does not start with "# ", "## ",
   "### Task ", or "#### Subtask "). Blank lines are markdown_blocks. *)
markdown_block  = ( blank_line
                  | non_structural_line, NEWLINE ) ;

non_structural_line = ? any line that does not match a header
                        production above ? ;

blank_line      = NEWLINE ;

NUMBER          = DIGIT, { DIGIT } ;

DIGIT           = "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" ;

IDENTIFIER      = LETTER, { LETTER | DIGIT | "-" | "_" } ;

LETTER          = "a" | "b" | ... | "z" | "A" | "B" | ... | "Z" ;

ANY_CHAR        = ? any Unicode character ? ;

NEWLINE         = ? line terminator (LF or CRLF) ? ;
```

## Semantic Constraints

Beyond the syntactic rules, the following semantic constraints must be validated:

### 1. Dependency Integrity

All task references in `**Prior:**` fields must resolve to existing tasks in the same document. A `**Prior:**` list must not contain duplicate references and must not reference its own task (self-reference is a 1-cycle).

```markdown
### Task 2: Implementation
**State:** pending
**Prior:** Task 1, Task 3    ← Task 1 and Task 3 must exist
```

### 2. State Validity

All state values must be defined in the associated states configuration. The states definition is loaded from an external YAML file.

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

Subtasks are only permitted under tasks with a numeric `task_id`. The first number of every subtask must equal its parent task's id, and subtask numbers must be unique within their parent:

```markdown
### Task 2: Parent Task
**State:** pending

#### Subtask 2.1: Valid
**State:** pending              ← Correct: parent is Task 2, has required State

#### Subtask 3.1: Invalid
**State:** pending              ← ERROR: parent task number mismatch

#### Subtask 2.1: Duplicate
**State:** pending              ← ERROR: duplicate subtask number under Task 2
```

A task with a named (non-numeric) `task_id` must not declare any subtasks.

### 5. Identifier Uniqueness

Task ids must be unique across the entire plan. In a Single-File Plan, two `### Task <id>:` headers with the same id are an error. In a Directory Workspace, two tasks with the same id across *any* files within the `tasks/` directory are an error.

### 6. Link Integrity

All relative markdown links (`[text](target)`) in content sections, task content, and subtask content must resolve to existing files. Links are resolved relative to the directory containing the plan file (or `index.rhei.md` for Directory Workspaces).

External URLs (`http://`, `https://`, `mailto:`), and fragment-only anchors (`#section`) are not checked. When a link contains a fragment (`file.md#section`), only the file portion is verified.

```markdown
## Overview
See [the spec](specs/language.md) for details.    ← specs/language.md must exist

### Task 1: Setup
**State:** pending

Read [guide](https://example.com/guide)           ← OK: external URL, not checked
See [section](#overview)                           ← OK: fragment-only, not checked
See [missing](docs/nonexistent.md)                 ← ERROR: file does not exist
```

## Token Types

For lexer implementation, the following token types are needed:

| Token | Pattern | Example |
|-------|---------|---------|
| `RheiHeader` | `# Rhei: .*` | `# Rhei: My Project` |
| `MetadataStates` | `\*\*States:\*\* .*` | `**States:** task-states` |
| `TasksSection` | `^## Tasks\s*$` | `## Tasks` |
| `SectionHeader` | `^## .+$` (matched only if `TasksSection` did not match) | `## Overview` |
| `TaskHeader` | `### Task <id>: .*` | `### Task 1: Setup` |
| `SubtaskHeader` | `#### Subtask <n>.<m>: .*` | `#### Subtask 1.2: Config` |
| `MetadataState` | `\*\*State:\*\* .*` | `**State:** pending` |
| `MetadataPrior` | `\*\*Prior:\*\* .*` | `**Prior:** Task 1` |
| `Text` | Any other line | Description text |

## AST Node Types

For parser implementation, the following AST structure is recommended:

```rust
struct Rhei {
    title: String,
    states: String, // state machine name; defaults to "rhei" when omitted
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
    content: String,
    subtasks: Vec<Subtask>,
}

struct Subtask {
    task_number: u32,
    subtask_number: u32,
    title: String,
    state: String,
    content: String,
}

enum TaskId {
    Number(u32),
    Named(String),
}
```

## Language Classification

The Rhei Plan language is **context-sensitive** because:

1. Subtask numbers depend on their parent task context
2. `Prior` references must resolve to existing task definitions
3. State values depend on external states configuration

The language cannot be fully described by a context-free grammar alone; semantic analysis is required for complete validation.

## File Extension

The recommended file extension for Rhei Plan documents is `.rhei.md` or simply `.md` when the context is clear.

## CLI Command Groups

The `rhei` CLI organizes its subcommands into three groups:

| Group | Commands | Purpose |
| --- | --- | --- |
| **Inspection** | `validate`, `render`, `states` | Read-only commands that examine or render a plan without modifying it |
| **Execution** | `transition`, `run`, `next`, `complete` | Commands that mutate the plan file via state transitions |
| **Info** | `version`, `help` | Meta commands about the tool itself |

## Related Specifications

- [How Rhei Is Used](specs/rhei-usage.spec.md) - Roles, coordination patterns, and agent workflows
- [Plan Language Usage Guide](specs/rhei-authoring.spec.md) - Practical authoring patterns and walkthroughs
- [States Specification](specs/rhei-states.spec.md) - Defines the states configuration format
- [Transitions Specification](specs/rhei-transitions.spec.md) - Formal state transition system, callbacks, and YAML schema
- [State Machine Writer](specs/rhei-state-machine-writer.spec.md) - Designing custom state machines from project specs and teams
