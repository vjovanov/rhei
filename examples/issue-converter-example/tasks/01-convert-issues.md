### Task convert-issues: Convert GitHub issues into executable Rhei tasks
**State:** convert

Fetch matching GitHub issues from `octocat/hello-world`, triage them against the conversion
brief, create or reuse one git worktree per matching non-duplicate issue, and
write one issue plan file per converted issue. Each issue plan file contains
spec-inspection, implementation, verification, and PR-opening tasks. The
generated tasks are the plan surface a later `rhei run` executes; broad or
underspecified issues must still become executable spec-inspection plans unless
they are duplicates.

**Conversion brief:**

Convert every matching non-duplicate issue into executable Rhei tasks. If the issue is vague or broad, create a spec-inspection task before implementation instead of skipping it. Only skip exact duplicates, non-issue project items, or inaccessible items that cannot be inspected.
