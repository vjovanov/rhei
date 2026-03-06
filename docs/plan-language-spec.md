# Saga Plan Language Specification

This document defines the formal grammar and semantics of the Saga Plan language, a structured subset of Markdown for hierarchical task management.

## Overview

The Saga Plan language is a **context-sensitive subset of Markdown** designed for:
- GitHub/ticket system integration
- AI agent state management
- Human oversight of work progress

## Document Structure

A Saga Plan document has a fixed hierarchical structure:

| Component | Heading Level | Format | Required |
|-----------|---------------|--------|----------|
| Saga Title | H1 (`#`) | `# Saga: <title>` | Yes |
| Content Sections | H2 (`##`) | `## <section-name>` | No |
| Tasks Section | H2 (`##`) | `## Tasks` | Yes |
| Task | H3 (`###`) | `### Task <id>: <title>` | Yes (at least one) |
| Subtask | H4 (`####`) | `#### Subtask <n>.<m>: <title>` | No |

## Grammar (EBNF)

```ebnf
(* ============================================== *)
(* DOCUMENT STRUCTURE                             *)
(* ============================================== *)

saga_document   = saga_header, { content_section }, tasks_section ;

saga_header     = "# Saga: ", title, NEWLINE ;

content_section = "## ", section_title, NEWLINE, { markdown_block } ;

tasks_section   = "## Tasks", NEWLINE, { task } ;


(* ============================================== *)
(* TASK DEFINITION                                *)
(* ============================================== *)

task            = task_header, NEWLINE, metadata, { markdown_block }, { subtask } ;

task_header     = "### Task ", task_id, ": ", title ;

task_id         = NUMBER | IDENTIFIER ;


(* ============================================== *)
(* SUBTASK DEFINITION                             *)
(* ============================================== *)

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

state_value     = IDENTIFIER                           (* single word: pending *)
                | "`", IDENTIFIER, { " ", IDENTIFIER }, "`" ;  (* escaped: `in progress` *)


(* ============================================== *)
(* TERMINALS                                      *)
(* ============================================== *)

title           = { ANY_CHAR - NEWLINE }+ ;

section_title   = { ANY_CHAR - NEWLINE }+ ;

markdown_block  = { ANY_CHAR }, NEWLINE ;

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

All task references in `**Prior:**` fields must resolve to existing tasks in the same document.

```markdown
### Task 2: Implementation
**State:** pending
**Prior:** Task 1, Task 3    ← Task 1 and Task 3 must exist
```

### 2. State Validity

All state values must be defined in the associated state machine configuration. The state machine is loaded from an external YAML file.

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

Subtask numbers must match their parent task:

```markdown
### Task 2: Parent Task
**State:** pending

#### Subtask 2.1: Valid      ← Correct: parent is Task 2
#### Subtask 3.1: Invalid    ← ERROR: parent task number mismatch
```

### 5. Metadata Field Order

The `**State:**` field must always appear before `**Prior:**` when both are present.

## Token Types

For lexer implementation, the following token types are needed:

| Token | Pattern | Example |
|-------|---------|---------|
| `SagaHeader` | `# Saga: .*` | `# Saga: My Project` |
| `SectionHeader` | `## [^T].*` or `## T[^a].*` | `## Overview` |
| `TasksSection` | `## Tasks` | `## Tasks` |
| `TaskHeader` | `### Task <id>: .*` | `### Task 1: Setup` |
| `SubtaskHeader` | `#### Subtask <n>.<m>: .*` | `#### Subtask 1.2: Config` |
| `MetadataState` | `\*\*State:\*\* .*` | `**State:** pending` |
| `MetadataPrior` | `\*\*Prior:\*\* .*` | `**Prior:** Task 1` |
| `Text` | Any other line | Description text |

## AST Node Types

For parser implementation, the following AST structure is recommended:

```rust
struct Saga {
    title: String,
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

The Saga Plan language is **context-sensitive** because:

1. Subtask numbers depend on their parent task context
2. `Prior` references must resolve to existing task definitions
3. State values depend on external state machine configuration

The language cannot be fully described by a context-free grammar alone; semantic analysis is required for complete validation.

## File Extension

The recommended file extension for Saga Plan documents is `.saga.md` or simply `.md` when the context is clear.

## Related Specifications

- [State Machine Specification](state-machine-spec.md) - Defines the state machine configuration format
