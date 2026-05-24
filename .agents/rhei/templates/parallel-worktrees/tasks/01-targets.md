{% for t in targets -%}
### Task {{t.id}}: Apply the batch task in {{t.path}}
**State:** prepare-worktree

**Target path:** `{{t.path}}` — make all edits inside this target's own worktree
(`{{worktree_root}}/{{t.id}}` on branch `{{branch_prefix}}/{{t.id}}`). Do not touch
other targets' paths; they run in parallel in their own worktrees.

The task to apply to `{{t.path}}`:

{{task}}

{% endfor -%}
