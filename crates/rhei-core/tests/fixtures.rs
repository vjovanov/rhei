pub const VALID_PLAN: &str = r#"# Rhei: Release Automation Rollout

## Overview
Coordinate the staged rollout of release automation across environments.

## Requirements
- Preserve audit logs
- Support human checkpoints

## Tasks

### Task 1: Define pipeline contracts
**State:** completed

Document inputs, outputs, and rollback hooks.

#### Subtask 1.1: Capture deployment events
**State:** completed
List all event types emitted by the deployment system.

#### Subtask 1.2: Record rollback contract
**State:** completed
```yaml
rollback:
  enabled: true
```

### Task 2: Bootstrap environments
**State:** in-progress
**Prior:** Task 1

Prepare staging and production credentials.

#### Subtask 2.1: Provision staging secrets
**State:** in-progress
Create and store staging credentials.

### Task 3: Roll out release bot
**State:** pending
**Prior:** Task 1, Task 2

Enable the release bot after environment bootstrap succeeds.

#### Subtask 3.1: Dry run in staging
**State:** pending
Run the bot in dry-run mode against staging.
"#;

pub const INVALID_PLAN: &str = r#"# Rhei: Broken Rollout Plan

## Tasks

### Task 1: Ship release bot
**State:** blocked
**Prior:** Task deploy

#### Subtask 2.1: Wrong subtask parent
**State:** pending
This subtask intentionally mismatches its parent task number.

### Task deploy: Deploy release bot
**State:** blocked
**Prior:** Task 1
"#;

pub const INVALID_FIXTURE_MISSING_RHEI_HEADER: &str = r#"## Tasks

### Task 1: Missing rhei header
**State:** pending
"#;

pub const INVALID_FIXTURE_MALFORMED_RHEI_HEADER: &str = r#"#Rhei: Missing required space

## Tasks

### Task 1: Primary task
**State:** pending
"#;

pub const INVALID_FIXTURE_MISSING_TASKS_SECTION: &str = r#"# Rhei: Missing tasks section
"#;

pub const INVALID_FIXTURE_EMPTY_TASKS_SECTION: &str = r#"# Rhei: Empty tasks section

## Tasks
"#;

pub const INVALID_FIXTURE_MALFORMED_TASK_HEADING: &str = r#"# Rhei: Malformed task heading

## Tasks

### Tak 1: Broken keyword
**State:** pending
"#;

pub const INVALID_FIXTURE_MALFORMED_SUBTASK_HEADING: &str = r#"# Rhei: Malformed subtask heading

## Tasks

### Task 1: Parent task
**State:** pending

#### Subtask 1: Missing decimal component
**State:** pending
"#;

pub const INVALID_FIXTURE_MISSING_TASK_TITLE: &str = r#"# Rhei: Missing task title

## Tasks

### Task 1:
**State:** pending
"#;

pub const INVALID_FIXTURE_MISSING_SUBTASK_TITLE: &str = r#"# Rhei: Missing subtask title

## Tasks

### Task 1: Parent task
**State:** pending

#### Subtask 1.1:
**State:** pending
"#;

pub const INVALID_FIXTURE_MALFORMED_STATE_METADATA: &str = r#"# Rhei: Malformed state metadata

## Tasks

### Task 1: Metadata near miss
**State** pending
"#;

pub const INVALID_FIXTURE_MALFORMED_PRIOR_METADATA: &str = r#"# Rhei: Malformed prior metadata

## Tasks

### Task 1: Metadata near miss
**State:** pending
**Prior** Task 2
"#;

pub const INVALID_FIXTURE_METADATA_OUTSIDE_TASK: &str = r#"# Rhei: Metadata outside task

**State:** pending

## Tasks

### Task 1: Actual task
**State:** pending
"#;

pub const INVALID_FIXTURE_LATE_METADATA_AFTER_CONTENT: &str = r#"# Rhei: Late metadata after content

## Tasks

### Task 1: Content before metadata
**State:** pending
Task body content appears before late metadata.
**Prior:** Task 2

### Task 2: Dependency target
**State:** completed
"#;

pub const INVALID_FIXTURE_MISSING_STATE: &str = r#"# Rhei: Missing state

## Tasks

### Task 1: No state metadata
"#;

pub const INVALID_FIXTURE_PRIOR_BEFORE_STATE: &str = r#"# Rhei: Prior before state

## Tasks

### Task 1: Wrong metadata order
**Prior:** Task 2
**State:** pending

### Task 2: Dependency target
**State:** completed
"#;

pub const INVALID_FIXTURE_MISSING_DEPENDENCY: &str = r#"# Rhei: Missing dependency

## Tasks

### Task 1: Depends on unknown task
**State:** pending
**Prior:** Task 99
"#;

pub const INVALID_FIXTURE_INVALID_STATE: &str = r#"# Rhei: Invalid state

## Tasks

### Task 1: Uses unsupported state
**State:** blocked
"#;

pub const INVALID_FIXTURE_SUBTASK_PARENT_MISMATCH: &str = r#"# Rhei: Subtask parent mismatch

## Tasks

### Task 2: Parent task
**State:** pending

#### Subtask 1.1: Wrong parent prefix
**State:** pending
"#;

pub const INVALID_FIXTURE_CIRCULAR_DEPENDENCY: &str = r#"# Rhei: Circular dependency

## Tasks

### Task 1: First task
**State:** pending
**Prior:** Task 2

### Task 2: Second task
**State:** pending
**Prior:** Task 1
"#;

pub const TEST_STATE_MACHINE: &str = r#"name: integration-test-machine
version: 1
states:
  pending:
    description: Task not yet started
  in-progress:
    description: Task currently being worked on
  completed:
    description: Task finished successfully
"#;
