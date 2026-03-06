pub const VALID_PLAN: &str = r#"# Saga: Release Automation Rollout

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
List all event types emitted by the deployment system.

#### Subtask 1.2: Record rollback contract
```yaml
rollback:
  enabled: true
```

### Task bootstrap_env: Bootstrap environments
**State:** in-progress
**Prior:** Task 1

Prepare staging and production credentials.

#### Subtask 2.1: Provision staging secrets
Create and store staging credentials.

### Task 3: Roll out release bot
**State:** pending
**Prior:** Task 1, Task bootstrap_env

Enable the release bot after environment bootstrap succeeds.

#### Subtask 3.1: Dry run in staging
Run the bot in dry-run mode against staging.
"#;

pub const INVALID_PLAN: &str = r#"# Saga: Broken Rollout Plan

## Tasks

### Task 1: Ship release bot
**Prior:** Task deploy
**State:** blocked

#### Subtask 2.1: Wrong subtask parent
This subtask intentionally mismatches its parent task number.

### Task deploy: Deploy release bot
**State:** blocked
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
  blocked:
    description: Task blocked by dependencies
"#;
