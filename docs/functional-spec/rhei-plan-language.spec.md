# FS-rhei-plan-language: Rhei Plan Language Specification

This document specifies the grammar and semantics of the Rhei Plan language — a structured subset of Markdown for hierarchical task management shared by humans and AI agents.

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

`**Prior:**` declares dependencies — Task 2 cannot be claimed until Task 1 is
in a terminal state as defined by the active state machine (`final: true`; in
the built-in `rhei` machine, `completed` and `cancelled`). Tasks without
`**Prior:**` are immediately dependency-ready.

Tasks may also be decomposed hierarchically when the plan's optional
`structure.maxLevels` setting allows more than one level:

```markdown
# Rhei: Stabilize release
---
structure:
  maxLevels: 3
  nodeKinds: [task, bug]
---

## Tasks

### Task 1: Release readiness
**State:** pending

#### Task 1.1: Verify release notes
**State:** pending

##### Task 1.1.1: Compare changelog and notes
**State:** pending

#### Bug 1.2: Fix null-cache panic
**State:** pending
```

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

The worker reads the plan, loads the state machine, and enters a loop: claim
the next eligible leaf task with `rhei next`, work in that task's current state,
use `rhei transition` when the workflow requires an explicit state change (for
example `draft` to `pending`), finish with `rhei complete` when the task reaches
a terminal outcome, and repeat. This manual worker flow is distinct from
`rhei run` agent mode, where spawned agents do the work of the current state and
the `rhei run` orchestrator performs the transition after the subprocess exits.
The worker stops when no eligible leaf tasks remain or a gating state such as
`human-review` requires a human decision.

The plan file is the single source of truth — multiple agents or humans can read it to see what is done, what is in progress, and what is blocked.

For programmatic execution with `rhei run`, see [Pattern 6: Programmatic State Transitions](rhei-usage.spec.md#38-pattern-6-programmatic-state-transitions).



## 1. Plan Formats

A Rhei plan can be authored as either a **Single-File Plan** or a **Directory Workspace**.

### 1.1. Single-File Plan (1 Agent, or low concurrency)

The single-file format is a hierarchical structure:

| Component | Heading Level | Format | Required |
|-----------|---------------|--------|----------|
| Rhei Title | H1 (`#`) | `# Rhei: <title>` | Yes |
| States Declaration | — | `**States:** <state-machine-name>` | No (defaults to `rhei`) |
| Content Sections | H2 (`##`) | `## <section-name>` | No |
| Tasks Section | H2 (`##`) | `## Tasks` | Yes |
| Root Node | H3 (`###`) | `### <kind> <id>: <title>` | Yes (at least one) |
| Child Node | H4-H6 (`####`-`######`) | `<heading> <kind> <id>: <title>` | No |

When present, the `**States:**` field must be the first non-empty line after
the `# Rhei:` title. Its value is the `name` of the state machine defined in the
associated states configuration (see [States Specification](rhei-states.spec.md)).
State-machine resolution is defined in
[State Machine Resolution](#13-state-machine-resolution).

When frontmatter omits a `structure` block, the default structure is:

```yaml
structure:
  maxLevels: 2
  nodeKinds: [task]
```

This establishes the default hierarchical structure for the current language
revision. See [ADR 0002](../adr/0002-hierarchical-task-nodes.md) for the rationale
behind the nested task-node model and for migration rules from pre-revision
plans.

### 1.2. Directory Workspace (Agent Teams, High Concurrency)

To prevent Git merge conflicts when multiple agents or humans work in parallel across disparate branches, a Rhei plan can be structured as a directory. This functions similarly to distributed issue trackers.

A Directory Workspace consists of:

1. **`index.rhei.md`**: The root configuration. Contains the `Rhei Title`, `States Declaration`, and any `Content Sections`. It does **not** contain a `## Tasks` section.
2. **`tasks/` directory**: A folder containing workspace task `.md` files.
3. **Workspace Task Files**: Files within `tasks/` that contain one or more
   node definitions (starting directly with `### <kind> <id>:`). They do not
   require the `# Rhei:` header.

Task-file discovery is recursive and deterministic. Implementations must load
non-hidden files matching `tasks/**/*.md`, where neither the file name nor any
directory segment under `tasks/` starts with `.`. Paths are ordered by their
normalized workspace-relative path using `/` separators and case-sensitive
lexicographic comparison. A discovered task file must parse as
`workspace_task_file` and contain at least one root task node; empty Markdown
files and prose-only Markdown files under `tasks/` are invalid task files.

In a Directory Workspace, all tasks are parsed and merged into a single global
task graph at runtime. Dependency validation (`**Prior:**`) resolves globally
across all discovered task files under `tasks/`. The merged plan order is the
discovered file order, then each file's authored preorder task order. Commands
that scan "in plan order" use this merged order for scheduling and rendering.

To prevent creation collisions in highly distributed swarms, letter-prefixed
`IDENTIFIER` values rather than sequential `NUMBER` task ids are strongly
recommended for Directory Workspaces. Because the grammar requires an
`IDENTIFIER` to start with a letter, distributed ids should use forms such as
`task-550e8400-e29b-41d4-a716-446655440000` rather than a bare UUID or hash.

### 1.3. State Machine Resolution

State-machine resolution is normative for all commands:

1. `--state-machine <path>` loads the specified YAML file and overrides
   automatic lookup. If the plan declares `**States:**`, the loaded file's
   `name` must match that value; if the field is omitted, the loaded file's
   `name` becomes the active state-machine name for this invocation.
2. When `**States:**` is omitted and no override is supplied, the plan uses the
   built-in `rhei` state machine. Sibling or workspace `states.yaml` files are
   ignored in this case.
3. When `**States:** rhei` is declared and no override is supplied, a matching
   auto-discovered `states.yaml` named `rhei` may be used; otherwise the plan
   falls back to the built-in `rhei` state machine.
4. When a non-`rhei` `**States:** <name>` is declared and no override is
   supplied, the CLI resolves the file from a sibling `states.yaml` for a
   single-file plan or from `<workspace>/states.yaml` for a Directory Workspace.
   That file must exist and its `name` must match `<name>`.
5. A declared non-`rhei` state machine without a matching auto-discovered file
   is a validation error; it never falls back to the built-in machine.

### 1.4. Directory Workspace Metadata

YAML frontmatter for a Directory Workspace belongs in `index.rhei.md`. Workspace
task files start directly with task definitions and must not introduce
independent frontmatter blocks, so the workspace has exactly one authoritative
`metadata.tasks.<id>` map.

Runtime-managed metadata that is defined in the transitions specification, such
as `metadata.tasks.<id>.stateVisits.<state-name>`, is therefore read from and
written to the frontmatter in `index.rhei.md`, keyed by the global task id.

Persistence ownership is normative:

- Markdown task fields remain the source of truth for `**State:**`,
  `**Prior:**`, `**Assignee:**`, and `> **Result:**`. In a
  Directory Workspace, those fields are persisted in the workspace task file
  that contains the task.
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

## 2. Grammar (EBNF)

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

(* ============================================== *)
(* DIRECTORY WORKSPACE STRUCTURE                  *)
(* ============================================== *)

workspace_index = rhei_header, { blank_line },
                  [ states_field, { blank_line } ],
                  [ frontmatter, { blank_line } ],
                  { content_section } ;

workspace_task_file = [ { blank_line } ], task_level_1, { task_level_1 } ;


(* ============================================== *)
(* YAML FRONTMATTER (optional metadata)           *)
(* ============================================== *)

(* YAML frontmatter stores custom task metadata such as retryCount,
   stateVisits, priority, callback data, and optional plan structure settings.
   Core task fields such as state, prior, assignee, and result links remain in
   markdown. It appears between two `---` fences and is parsed as YAML, not
   Markdown. See the Transitions Specification for the metadata access
   contract. *)
frontmatter     = "---", NEWLINE,
                  { yaml_line },
                  "---", NEWLINE ;

yaml_line       = ? any line that is not exactly "---" ?, NEWLINE ;


(* ============================================== *)
(* TASK TREE                                      *)
(* ============================================== *)

tasks_section   = "## Tasks", NEWLINE, { blank_line },
                  task_level_1, { task_level_1 } ;

task_level_1    = node_header_level_1, NEWLINE, metadata, { task_markdown_block },
                  [ result_block ], { task_level_2 } ;

task_level_2    = node_header_level_2, NEWLINE, metadata, { task_markdown_block },
                  [ result_block ], { task_level_3 } ;

task_level_3    = node_header_level_3, NEWLINE, metadata, { task_markdown_block },
                  [ result_block ], { task_level_4 } ;

task_level_4    = node_header_level_4, NEWLINE, metadata, { task_markdown_block },
                  [ result_block ] ;

node_header_level_1 = "### ", node_kind_keyword, " ", task_id, ": ", title ;
node_header_level_2 = "#### ", node_kind_keyword, " ", task_id, ": ", title ;
node_header_level_3 = "##### ", node_kind_keyword, " ", task_id, ": ", title ;
node_header_level_4 = "###### ", node_kind_keyword, " ", task_id, ": ", title ;

node_kind_keyword = IDENTIFIER ;

task_id         = task_id_segment, { ".", task_id_segment } ;

task_id_segment = NUMBER | IDENTIFIER ;


(* ============================================== *)
(* METADATA FIELDS                                *)
(* ============================================== *)

(* State field is mandatory and must appear first.
   Prior and Assignee are optional; the `complete` command strips the assignee
   on completion. *)
metadata        = state_field, [ prior_field ], [ assignee_field ] ;

assignee_field  = "**Assignee:** ", title, NEWLINE ;

(* Result block links a terminal task to its runtime result/audit file.
   It is inserted by the `complete` command after task content and before child
   tasks. The link text is the task id itself, and the target is always
   runtime/results/<task-id>.md. *)
result_block    = "> **Result:** ", "[", task_id, "](", result_path, ")", NEWLINE ;

result_path     = "runtime/results/", task_id, ".md" ;

state_field     = "**State:** ", state_value, NEWLINE ;

prior_field     = "**Prior:** ", task_ref_list, NEWLINE ;

task_ref_list   = task_ref, { ", ", task_ref } ;

task_ref        = node_kind_keyword, " ", task_id ;

(* State values have two rendered forms:
   - bare form for canonical names that match IDENTIFIER exactly
   - backticked form for any other canonical state name, including names
     with spaces or punctuation such as `human review`, `qa/review`, or
     `security.review`
   States with a declared `visits` budget may append `-<n>` (where n >= 2)
   to the rendered value to make later visits visible in markdown. A `-1`
   suffix is never rendered — the first visit is the unsuffixed base name.
   Exact-match resolution against loaded state names happens before any
   `-<n>` suffix is interpreted as a counted visit. Inside backticks, `\\`
   escapes a backslash and `\`` escapes a literal backtick. *)
state_value     = IDENTIFIER, [ "-", VISIT_NUMBER ]              (* pending, review-2 *)
                | "`", quoted_state_text, "`" ;                 (* `human review`, `qa/review`, `human review-2` *)

(* VISIT_NUMBER encodes a counted-visit suffix. It must be >= 2 so that the
   unsuffixed base name is the only valid rendering of visit 1. No leading
   zeros. *)
VISIT_NUMBER    = LEADING_2_9, { DIGIT }
                | "1", DIGIT, { DIGIT } ;

LEADING_2_9     = "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" ;

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

(* A markdown_block is any non-structural content line for use in sections.
   Blank lines are markdown_blocks. *)
markdown_block  = ( blank_line
                  | non_structural_line, NEWLINE ) ;

task_body_line  = ? any line that does not match a header production above
                   and does not begin with "> **Result:** " ? ;

non_structural_line = ? any line that does not match a header
                        production above ? ;

blank_line      = NEWLINE ;

NUMBER          = "0" | NONZERO_DIGIT, { DIGIT } ;

DIGIT           = "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" ;

NONZERO_DIGIT   = "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" ;

IDENTIFIER      = LETTER, { LETTER | DIGIT | "-" | "_" } ;

LETTER          = "a" | "b" | ... | "z" | "A" | "B" | ... | "Z" ;

ANY_CHAR        = ? any Unicode character ? ;

ESCAPED_BACKSLASH = "\\\\" ;

ESCAPED_BACKTICK = "\\`" ;

NEWLINE         = ? line terminator (LF or CRLF) ? ;
```

## 3. Semantic Constraints

Beyond the syntactic rules, the following semantic constraints must be validated:

The grammar tolerates optional blank lines immediately after `## Tasks` in a
single-file plan and at the start of a workspace task file. Implementations
should accept those blank lines in both formats.

Throughout this specification, a *task node* means any authored node entry at
heading level `###`, `####`, `#####`, or `######` whose keyword is declared in
`structure.nodeKinds` (or the default `[task]` when omitted). Root task nodes
live directly under `## Tasks`; deeper task nodes are children of the nearest
shallower task node above them.

Throughout this specification, a *terminal state* means any state marked
`final: true` in the active state machine. In the built-in `rhei` machine, the
terminal states are `completed` and `cancelled`.

Throughout this specification, a *leaf task node* means a task node with no
child task nodes. Only leaf task nodes are claimable by `rhei next`. Non-leaf
task nodes are structural rollups and result anchors for their descendants; they
must be excluded from manual claim selection so a parent and child are never
claimed at the same time. `rhei run` may still execute a non-leaf root task when
the active state machine models the parent as the workflow owner for its child
nodes. Non-leaf nodes may be moved to a terminal state only after all
descendants are terminal.

Dependency readiness requires successful terminal dependencies: a task is ready
with respect to `**Prior:**` only when every referenced dependency is in a
terminal state whose normalized state name is not `cancelled`. State-machine
`instructions` text is descriptive guidance for agents and must not narrow or
override this readiness rule unless the machine introduces a separate normative
field for that purpose.

When frontmatter defines a `structure` map, the following keys are meaningful
to this specification:

- `structure.maxLevels` — maximum allowed task depth, counted from level 1 at
  `### <kind> ...`. Valid values are `1` through `4`. If omitted, the default
  is `2`.
- `structure.nodeKinds` — allowed node-kind keywords for task nodes. Values
  must be unique `IDENTIFIER`s. Parsing and validation normalize them
  case-insensitively. If omitted, the default is `[task]`.

`rhei` is a reserved node-kind name. It denotes the plan itself and is
always the kind of the single root node at level 0. `rhei` must not appear
in `structure.nodeKinds`, and it must not appear as a key under
`node_policy.by_type` in the active state machine. A non-root node must
never use the `rhei` kind.

### Plan Root Model

The level-0 `rhei` root node is virtual. It is not authored in markdown, has no
`**State:**`, `**Prior:**`, `**Assignee:**`, or `> **Result:**` line, and is not
persisted as a task node in a plan file or workspace task file. Runtime commands
must not claim, transition, complete, cancel, or reset the virtual root.

`node_policy.root` validates and names the state-machine profile for tools that
model the plan root internally, but it does not create a persisted or executable
node. UI-level plan status, including any `plan_state` shown by visualization
tools, is derived from authored task nodes rather than from a root state field.

The heading keyword is the node kind. By convention, authored headings render
that keyword in Title Case (`task` -> `Task`, `bug` -> `Bug`), but semantic
matching is case-insensitive.

Each node's state policy — which state it starts in and which states it may
ever hold — is resolved from the active state machine's `profiles` and
`node_policy` blocks. The root (always `rhei`) resolves through
`node_policy.root`; all other nodes resolve through `node_policy.overrides`,
then `node_policy.by_type[<kind>]`, then `node_policy.default`. See the
[States Specification](rhei-states.spec.md#9-node-policy) for the full
resolution order and validation rules.

### 3.1. Dependency Integrity

All task references in `**Prior:**` fields must resolve to existing task nodes
in the same logical plan: in a Single-File Plan that means the same document,
and in a Directory Workspace that means the merged workspace graph across all
task files under `tasks/`. A `**Prior:**` list must not contain duplicate
references, must not reference its own task (self-reference is a 1-cycle), and
must not reference any ancestor of the task. A child task cannot list its
parent as `**Prior:**`; if generated follow-up work must wait for a completed
parent task, author that follow-up as a top-level sibling with `**Prior:**`
pointing at the completed task.

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

When a dependency reference includes a node kind, that kind must match the
declared kind of the referenced node.

Invalid child dependency example:

```markdown
### Task fetch-prs: Fetch pull requests
**State:** completed

#### Task fetch-prs.ci-failure-5227: Triage CI failure
**State:** pending
**Prior:** Task fetch-prs    ← ERROR: child cannot depend on its parent
```

### 3.2. State Validity

All state values must be defined in the associated states configuration resolved
by the state-machine resolution rules in
[State Machine Resolution](#13-state-machine-resolution).

In addition, each authored `**State:**` must be a member of the node's
resolved profile's `allowed` set, as determined by the active state
machine's `node_policy`. A state that is defined in the global `states`
block but excluded from the node's resolved profile is a validation error
on that node.

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

### 3.3. Acyclic Dependencies

The task dependency graph must be a Directed Acyclic Graph (DAG). Circular dependencies are forbidden:

```markdown
### Task 1: First
**State:** pending
**Prior:** Task 2    ← ERROR: creates cycle

### Task 2: Second
**State:** pending
**Prior:** Task 1    ← ERROR: creates cycle
```

### 3.4. Hierarchical Task Consistency

Task nodes form a tree. Authored heading depth and task-id path depth must
match:

- a `### <kind> ...` node must use a one-segment id such as `1` or `api`
- a `#### <kind> ...` node must use a two-segment id such as `1.1` or `api.cache`
- a `##### <kind> ...` node must use a three-segment id
- a `###### <kind> ...` node must use a four-segment id

Child task ids must extend their parent id by exactly one segment, and sibling
task ids must be unique within the same parent.

```markdown
### Task 2: Parent Task
**State:** pending

#### Task 2.1: Valid child
**State:** pending

#### Task 3.1: Invalid child
**State:** pending
                         ← ERROR: child id does not extend parent id `2`

#### Task 2.1: Duplicate sibling
**State:** pending
                         ← ERROR: duplicate child id under Task 2
```

Mixed numeric and named path segments are valid so long as the full child id
extends the parent id by one segment:

```markdown
### Task api: Build API
**State:** pending

#### Task api.cache: Add cache layer
**State:** pending

##### Task api.cache.1: Verify cache key behavior
**State:** pending
```

Task id segments are canonical. A numeric segment must be `0` or a decimal
integer with no leading zeroes, and it must fit in the unsigned 32-bit range
`0..=4294967295`. `Task 1` and `Task 01` are not two distinct ids:
`Task 01` is invalid syntax because of the leading zero. `Task 4294967296` is
semantically invalid because the numeric segment is out of range. Result paths,
dependency references, rendered ids, and uniqueness checks all use the canonical
task id written in the plan.

Task depth must not exceed `structure.maxLevels`.

### 3.5. Identifier Uniqueness

Task ids must be unique across the entire plan. In a Single-File Plan, two
task nodes with the same id are an error. In a Directory Workspace, two task
nodes with the same id across *any* files within the `tasks/` directory are an
error.

### 3.6. Link Integrity

All relative markdown links (`[text](target)`) in content sections and task
node content must resolve to existing files. In a Single-File Plan, links
resolve relative to the directory containing the plan file. In a Directory
Workspace, they resolve relative to the workspace root, meaning the directory
containing `index.rhei.md`, even when the link appears in a nested file under
`tasks/`.

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

### 3.7. Node Kind Validity

The heading keyword for each task node must be listed in `structure.nodeKinds`.
When `structure.nodeKinds` is omitted, only `Task` is valid.

Example:

```markdown
---
structure:
  nodeKinds: [task, bug]
---

## Tasks

### Task 1: Stabilize release
**State:** pending

#### Bug 1.1: Fix crash in cache warmer
**State:** pending        ← Valid

#### Spike 1.2: Unknown category
**State:** pending        ← ERROR: `spike` is not a declared node kind
```

### 3.8. Result Block Consistency

When a task contains a `> **Result:**` block, that block must describe the
enclosing task itself:

- The link text must equal the enclosing task's `task_id`.
- The target path must be exactly `runtime/results/<task-id>.md` using that
  same id.

A result block is optional syntax, but it has a lifecycle invariant:

- A non-terminal task must not contain a result block.
- A terminal task may contain one valid result block. Validation does not
  require every terminal task to have one, because terminal states can be
  reached by commands other than `rhei complete` and by imported plans.
- `rhei complete` must create or preserve exactly one valid result block for a
  successful non-cancelled terminal completion.
- `rhei transition` may append audit entries to the result file, but it never
  adds a result block to the task body.
- `rhei reset` removes result blocks along with other runtime completion
  artifacts.

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

### 3.9. Terminal Tree Coherence

A task node in a terminal state must not contain any non-terminal descendants.

```markdown
### Task 2: Parent
**State:** completed

#### Task 2.1: Still open
**State:** pending        ← ERROR: terminal parent with non-terminal child
```

### 3.10. State Artifact Contracts

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
alone. When a transition also declares `on_leave` or `on_enter` callbacks,
those callbacks are optional per edge: an omitted callback is treated as
implicit success.

This section is canonical for artifact enforcement order across commands:

| Command | Enforced artifacts | Ordering |
|---------|--------------------|----------|
| `rhei next` | Current-state `inputs` only | Before a task is claimable, resolve the current state's inputs. Required inputs must exist; optional inputs are resolved and exposed but missing optional inputs do not block. Claim mode re-checks under the file lock immediately before writing `**Assignee:**`. `next --peek` does not check outputs, run callbacks, write state, or write result files; claim mode may auto-advance non-runnable initial states as defined in the next-command spec. |
| `rhei transition` | Source-state `outputs`; target-state `inputs` | After the compare-and-swap guard and edge validation, execute `on_leave` unless skipped. Then check required source outputs, resolve target inputs for the final target state, and reject before the state write if any required artifact is missing. Optional target inputs are skipped for blocking but still resolved. Write the target state, execute `on_enter` unless skipped, append the transition audit entry to the result file, then atomically persist the task file. |
| `rhei complete` | Source-state `outputs`; completion-target `inputs` | Select the non-cancelled terminal completion target first. Then use the same transition artifact order as `rhei transition`: `on_leave`, source outputs, target inputs, state write, `on_enter`. After the transition succeeds, append the result-file entry, remove `**Assignee:**`, add or preserve the result block, and atomically persist the task file. |
| `rhei run` | Current-state `inputs`; source-state `outputs` for successful work; target-state `inputs` | Before spawning work, the ready-set scan checks current-state required inputs and skips missing optional inputs for blocking. After a subprocess exits `0`, required source outputs are part of the completion condition; if any are missing, no transition fires and the task stays in its current state. Non-zero, timeout, and tooling-failure routes select error transitions as specified by `rhei run` and do not require normal source outputs. For any selected target transition, required target inputs are checked before the state write and optional target inputs do not block. A successful-work transition also re-checks source outputs after `on_leave` and before the state write. Result-file writes, if any, happen only after the transition succeeds. |

For every declared input, required or optional, implementations resolve the path
and expose the path and existence flag to templates, agents, and programs.
`optional: true` is valid only on inputs. Outputs are always required when
declared, and a missing output blocks the transition out of the source state
except for the `rhei run` failure, timeout, and tooling-unavailable routes
described above.

Examples:

```markdown
# Single-file plan at /repo/plans/release.rhei.md
Artifact path: runtime/reviews/release.md
Resolves to: /repo/plans/runtime/reviews/release.md

# Directory workspace at /repo/release/index.rhei.md
Artifact path: runtime/reviews/release.md
Resolves to: /repo/release/runtime/reviews/release.md
```

## 4. Token Types

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
| `NodeHeader` | `^(###|####|#####|######) <kind> <id>: .*` | `#### Bug 1.2: Config` |
| `MetadataState` | `\*\*State:\*\* .*` | `**State:** pending` |
| `MetadataPrior` | `\*\*Prior:\*\* .*` | `**Prior:** Bug 1.2` |
| `MetadataAssignee` | `\*\*Assignee:\*\* .*` | `**Assignee:** alice` |
| `ResultBlock` | `^> \*\*Result:\*\* \[[^]]+\]\([^)]+\)\s*$` | `> **Result:** [task-1](runtime/results/task-1.md)` |
| `Text` | Any other line | Description text |

## 5. AST Node Types

This section is also illustrative and non-normative. It shows one viable shape
for a parser AST, but it is not a complete or exclusive contract.

For parser implementation, the following AST structure is recommended:

```rust
struct Rhei {
    title: String,
    states: String, // state machine name; defaults to "rhei" when omitted
    frontmatter: Option<YamlValue>,
    content_sections: Vec<ContentSection>,
    tasks: Vec<TaskNode>,
}

struct ContentSection {
    title: String,
    content: String,
}

struct TaskNode {
    id: TaskId,
    title: String,
    state: String,
    kind: String,
    prior: Vec<TaskId>,
    assignee: Option<String>,
    content: String,
    result: Option<ResultLink>,
    children: Vec<TaskNode>,
}

struct TaskId {
    segments: Vec<TaskIdSegment>,
}

enum TaskIdSegment {
    // Numeric segments are canonical: no leading zeros and within u32 range.
    Number(u32),
    Named(String),
}

struct ResultLink {
    task_id: TaskId,
    path: String,
}
```

## 6. Language Classification

The Rhei Plan language is **context-sensitive** because:

1. Child task ids must align with parent task ids and heading depth
2. `Prior` references must resolve to existing task definitions
3. Node-kind keywords depend on plan-level `structure` configuration
4. State values depend on external states configuration
5. State artifact contracts depend on external workspace files at execution time

The language cannot be fully described by a context-free grammar alone; semantic analysis is required for complete validation.

## 7. File Extension

The recommended file extension for Rhei Plan documents is `.rhei.md` or simply `.md` when the context is clear.

## 8. CLI Command Groups

The `rhei` CLI help currently organizes its subcommands into five groups:

| Group | Commands | Purpose |
| --- | --- | --- |
| **Inspection** | `validate`, `render`, `states`, `list` | Read-only commands that examine or render a plan without modifying it |
| **Templates** | `templates`, `instantiate` | Discover and instantiate reusable plan and workspace templates |
| **Execution** | `transition`, `run`, `snapshot`, `next`, `complete`, `reset` | Commands that mutate the plan file or workspace state, or operate on execution runtime artifacts |
| **Setup** | `install-skills`, `completions` | Install packaged Rhei skills and generate shell completion scripts |
| **Info** | `version`, `help` | Meta commands about the tool itself |

`rhei viz` remains specified as a visualization behavior surface (§FS-rhei-viz),
but it is not currently declared as a CLI variant or listed in the generated
help. Until the command is reintroduced, current CLI command groups must exclude
`viz` and treat the visualization spec as non-current implementation guidance.

## Related Specifications

- [How Rhei Is Used](rhei-usage.spec.md) - Roles, coordination patterns, and agent workflows
- [Plan Language Usage Guide](rhei-authoring.spec.md) - Practical authoring patterns and walkthroughs
- [States Specification](rhei-states.spec.md) - Defines the states configuration format
- [Transitions Specification](rhei-transitions.spec.md) - Formal state transition system, callbacks, and YAML schema
- [Validate Command](rhei-validate.spec.md) - `rhei validate` semantic checks
- [Render Command](rhei-render.spec.md) - `rhei render` output formats
- [States Command](rhei-states-cmd.spec.md) - `rhei states` state-machine inspection
- [List Command](rhei-list.spec.md) - `rhei list` filter set and output format
- [Next Command](rhei-next.spec.md) - `rhei next` behavioral contract, including `--peek` mode
- [Transition Command](rhei-transition-cmd.spec.md) - `rhei transition` compare-and-swap contract
- [Complete Command](rhei-complete.spec.md) - `rhei complete` behavioral contract
- [Run Command](rhei-run.spec.md) - `rhei run` execution loop under orchestrator authority
- [Reset Command](rhei-reset.spec.md) - `rhei reset` behavior for restoring initial state
- [Completions Command](rhei-completions.spec.md) - `rhei completions` shell completion generation
- [Version Command](rhei-version.spec.md) - `rhei version` component version output
- [State Machine Writer](rhei-state-machine-writer.spec.md) - Designing custom state machines from project specs and teams
