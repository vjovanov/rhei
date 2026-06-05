# Rhei: {{plan_title}}
**States:** issue-converter

## Overview

This workspace converts GitHub project items from `{{repo}}` into executable
Rhei task files. The converter fetches at most {{candidate_limit}} exact issue
candidate(s) using the configured issue filters, verifies each candidate's
Project item and Status directly, converts at most {{limit}} issue item(s)
whose project Status is `{{todo_status}}`, creates a Rhei task file, marks each
converted project item `{{in_progress_status}}`, sleeps for `{{sleep}}`, and
then checks for more matching `{{todo_status}}` candidates until none remain or
{{max_batches}} batch(es) have run. It records the inventory and conversion
rationale, and appends one executable task chain per matching non-duplicate
issue.
For each converted issue, the converter creates or reuses a dedicated git
worktree, then appends one issue plan file containing spec-inspection,
implementation, verification, and PR-opening tasks. Each issue plan is an
independent root graph; code and docs work happens in the per-issue worktree
while the scratchpad stores Rhei plan/runtime artifacts.

## Source

| Field | Value |
|---|---|
| Repository | `{{repo}}` |
| Source checkout | `{{repo_checkout}}` |
| Work subdirectory | `{{work_subdir}}` |
| Worktree root | `{{worktree_root}}` |
| Project owner | `{{project_owner}}` |
| Project number | {{project_number}} |
| TODO status | `{{todo_status}}` |
| In-progress status | `{{in_progress_status}}` |
| Author | `{% if author %}{{author}}{% else %}<any>{% endif %}` |
| State | `{{state}}` |
| Labels | `{% if labels %}{{labels}}{% else %}<none>{% endif %}` |
| Search | `{% if search %}{{search}}{% else %}<none>{% endif %}` |
| Batch limit | {{limit}} |
| Candidate query limit | {{candidate_limit}} |
| Sleep between batches | `{{sleep}}` |
| Max batches | {{max_batches}} |
| PR push remote | `{% if pr_push_remote %}{{pr_push_remote}}{% else %}<infer>{% endif %}` |
| PR head owner | `{% if pr_head_owner %}{{pr_head_owner}}{% else %}<infer>{% endif %}` |
| PR base branch | `{{pr_base_branch}}` |

## Conversion Brief

{{ conversion_brief | trim }}

## Agents

| Role | Agent |
|---|---|
| Converter | `{{converter_agent}}` |
| Issue worker / report | `{{worker_agent}}` |
| Agent reviewer | `{{reviewer_agent}}` |
