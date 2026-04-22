# Rhei: Human Review Content Refresh

## Context
Refresh customer-facing content while preserving explicit human checkpoints in the plan.

## Notes
This example demonstrates:
- escaped state values with spaces
- numeric task dependencies
- subtask body content captured by the parser
- realistic states from the default states file

## Tasks

### Task 1: Audit existing content
**State:** completed

Review the current landing page, pricing page, and help center copy.

#### Task 1.1: Snapshot current messaging
**State:** completed
Capture current headlines and calls to action for comparison.

### Task 2: Prepare revised copy
**State:** human-review
**Prior:** Task 1

Draft updated messaging and leave open questions for stakeholders.

#### Task 2.1: Draft homepage hero
**State:** completed
Propose updated headline, supporting text, and primary call to action.

#### Task 2.2: Summarize reviewer questions
**State:** completed
List questions for product and legal review in a short bullet list.

### Task 3: Apply approved edits
**State:** agent-review
**Prior:** Task 2

Update the content files after review feedback is resolved.

#### Task 3.1: Patch markdown sources
**State:** agent-review
Apply approved copy updates to the documentation repository.

### Task 4: Final sign-off
**State:** pending
**Prior:** Task 3

Coordinate final checks before publication.

#### Task 4.1: Confirm launch checklist
**State:** pending
Verify approvals, screenshots, and rollback notes are present.
