# Rhei Plan Language Specification

This document defines the formal grammar and semantics of the Rhei Plan language, a structured subset of Markdown for hierarchical task management.

## Overview

The Rhei Plan language is a **context-sensitive subset of Markdown** designed for:

- AI agent memory and plan management
- Human oversight of work progress
- AI agent orchestration
- Ticket system interaction

## Document Structure

A Rhei Plan document has a fixed hierarchical structure:

| Component | Heading Level | Format | Required |
|-----------|---------------|--------|----------|
| Rhei Title | H1 (`#`) | `# Rhei: <title>` | Yes |
| States Declaration | — | `**States:** <state-machine-name>` | No (defaults to `rhei`) |
| Content Sections | H2 (`##`) | `## <section-name>` | No |
| Tasks Section | H2 (`##`) | `## Tasks` | Yes |
| Task | H3 (`###`) | `### Task <id>: <title>` | Yes (at least one) |
| Subtask | H4 (`####`) | `#### Subtask <n>.<m>: <title>` | No |

When present, the `**States:**` field must be the first non-empty line after the `# Rhei:` title. Its value is the `name` of the state machine defined in the associated states configuration (see [States Specification](states-spec.md)). When omitted, the plan uses the built-in `rhei` state machine.

## Grammar (EBNF)

```ebnf
(* ============================================== *)
(* DOCUMENT STRUCTURE                             *)
(* ============================================== *)

rhei_document   = rhei_header, { blank_line },
                  [ states_field, { blank_line } ],
                  { content_section },
                  tasks_section ;

rhei_header     = "# Rhei: ", title, NEWLINE ;

states_field    = "**States:** ", state_machine_name, NEWLINE ;

state_machine_name = IDENTIFIER ;

content_section = "## ", section_title, NEWLINE, { markdown_block } ;

tasks_section   = "## Tasks", NEWLINE, task, { task } ;


(* ============================================== *)
(* TASK DEFINITION                                *)
(* ============================================== *)

task            = task_header, NEWLINE, metadata, { markdown_block }, { subtask } ;

task_header     = "### Task ", task_id, ": ", title ;

task_id         = NUMBER | IDENTIFIER ;


(* ============================================== *)
(* SUBTASK DEFINITION                             *)
(* ============================================== *)

(* Subtasks are only permitted under tasks with a numeric task_id. *)
subtask         = subtask_header, NEWLINE, { markdown_block } ;

subtask_header  = "#### Subtask ", NUMBER, ".", NUMBER, ": ", title ;


(* ============================================== *)
(* METADATA FIELDS                                *)
(* ============================================== *)

(* State field is mandatory and must appear first *)
metadata        = state_field, [ prior_field ] ;

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

#### Subtask 2.1: Valid      ← Correct: parent is Task 2
#### Subtask 3.1: Invalid    ← ERROR: parent task number mismatch
#### Subtask 2.1: Duplicate  ← ERROR: duplicate subtask number under Task 2
```

A task with a named (non-numeric) `task_id` must not declare any subtasks.

### 5. Identifier Uniqueness

Task ids must be unique across the document. Two `### Task <id>:` headers with the same id (numeric or named) are an error.

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

## Related Specifications

- [How Rhei Is Used](how-rhei-is-used.md) - Roles, coordination patterns, and agent workflows
- [Plan Language Usage Guide](plan-language-usage.md) - Practical authoring patterns and walkthroughs
- [States Specification](states-spec.md) - Defines the states configuration format
