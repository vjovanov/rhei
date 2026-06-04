### Task convert-issues: Convert GitHub issues into executable Rhei tasks
**State:** convert

Fetch matching GitHub issues from `{{repo}}`, triage them against the conversion
brief, create or reuse one git worktree per matching non-duplicate issue, and
write one issue plan file per converted issue. Each issue plan file contains
spec-inspection, implementation, verification, and PR-opening tasks. The
generated tasks are the plan surface a later `rhei run` executes; broad or
underspecified issues must still become executable spec-inspection plans unless
they are duplicates.
§FS-rhei-templates.7.1

**Conversion brief:**

{{ conversion_brief | trim }}
