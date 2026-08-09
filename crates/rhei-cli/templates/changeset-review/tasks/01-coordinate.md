### Task coordinate: Coordinate review of {{change_ref}}
**State:** split

Resolve `{{change_ref}}` (PR, branch, commit SHA, commit range, or diff
file) to a concrete set of changed files. Write an architectural overview
of the change, then split it into logical parts for focused review. For
all repository inspection, first resolve the Git toplevel from this
scratchpad workspace and use that repository root rather than the
workspace directory itself. For
each part, append a `review-<slug>` task to `tasks/` with `**State:** review`
and `**Prior:** Task coordinate`. Append one `aggregate` task with
`**State:** aggregate-reviews` and `**Prior:**` listing every
`review-<slug>` task you created. The aggregate task will validate review
correctness, compare fix proposals, let the smart target decide
discrepancies, and apply accepted fixes. When the overview and parts manifest
are written and all follow-up tasks are appended, transition to `completed`.
