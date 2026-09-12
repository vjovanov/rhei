//! The plan and state-machine text every harness in this directory starts
//! from: one four-state machine and the four task shapes — a chain, a fork,
//! three independents, and a parent with subtasks — that most e2e cases only
//! need a copy of. They are fixture data rather than harness code, which is the
//! seam they were lifted out of `mod.rs` along.

// ---------------------------------------------------------------------------
// State machine
// ---------------------------------------------------------------------------

pub const STATE_MACHINE: &str = r#"name: integration-test
version: 1
states:
  draft:
    initial: true
    description: Analysis phase
    instructions: |
      Analyze the task and write a description. Transition to pending once done.
  pending:
    description: Ready for work
    instructions: |
      Implement the task. Transition to completed when finished.
  completed:
    final: true
    description: Done
  cancelled:
    final: true
    description: Abandoned
transitions:
  - from: draft
    to: pending
  - from: pending
    to: completed
  - from: "*"
    to: cancelled
"#;

// ---------------------------------------------------------------------------
// Plan templates (all tasks start in draft)
// ---------------------------------------------------------------------------

pub const LINEAR_PLAN: &str = r#"# Rhei: Linear Chain

## Tasks

### Task 1: First step
**State:** draft

### Task 2: Second step
**State:** draft
**Prior:** Task 1

### Task 3: Third step
**State:** draft
**Prior:** Task 2
"#;

pub const PARALLEL_PLAN: &str = r#"# Rhei: Parallel Branches

## Tasks

### Task 1: Root
**State:** draft

### Task 2: Branch A
**State:** draft
**Prior:** Task 1

### Task 3: Branch B
**State:** draft
**Prior:** Task 1
"#;

pub const INDEPENDENT_PLAN: &str = r#"# Rhei: Independent Tasks

## Tasks

### Task 1: Alpha
**State:** draft

### Task 2: Beta
**State:** draft

### Task 3: Gamma
**State:** draft
"#;

pub const SUBTASK_PLAN: &str = r#"# Rhei: Subtask Test

## Tasks

### Task 1: Parent task
**State:** draft
Some task content here.

#### Task 1.1: First subtask
**State:** draft
Subtask one content.

#### Task 1.2: Second subtask
**State:** draft
Subtask two content.
"#;
