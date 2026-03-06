# Saga: Human Review Content Refresh

## Context
Refresh customer-facing content while preserving explicit human checkpoints in the plan.

## Notes
This example demonstrates:
- escaped state values with spaces
- numeric task dependencies
- subtask body content captured by the parser
- realistic states from the default state machine

## Tasks

### Task 1: Audit existing content
**State:** completed

Review the current landing page, pricing page, and help center copy.

#### Subtask 1.1: Snapshot current messaging
Capture current headlines and calls to action for comparison.

### Task 2: Prepare revised copy
**State:** human-review
**Prior:** Task 1

Draft updated messaging and leave open questions for stakeholders.

#### Subtask 2.1: Draft homepage hero
Propose updated headline, supporting text, and primary call to action.

#### Subtask 2.2: Summarize reviewer questions
List questions for product and legal review in a short bullet list.

### Task 3: Apply approved edits
**State:** agent-review
**Prior:** Task 2

Update the content files after review feedback is resolved.

#### Subtask 3.1: Patch markdown sources
Apply approved copy updates to the documentation repository.

### Task 4: Final sign-off
**State:** in-progress
**Prior:** Task 3

Coordinate final checks before publication.

#### Subtask 4.1: Confirm launch checklist
Verify approvals, screenshots, and rollback notes are present.
