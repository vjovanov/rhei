# Rhei: {{plan_title}}
**States:** github-issue-fix

## Overview

This workspace fixes one GitHub issue from `{{repo}}`: `{{issue}}`.

The first task creates or reuses an isolated worktree from `{{repo_checkout}}`,
fetches the issue, discovers repository instructions and grounding configuration,
records a spec-fit artifact, and writes exactly one follow-up task. The follow-up
task starts in implementation, human review, or GitHub handoff according to the
recorded verdict. Compatible issues proceed through validation, review/fix
cycles with separate requirements, spec, implementation, and validation reviews,
and PR publication; blocked, incompatible, or unclear issues stop for a human
gate or GitHub handoff instead of producing a speculative implementation PR.

## Source

| Field | Value |
|---|---|
| Repository | `{{repo}}` |
| Issue | `{{issue}}` |
| Source checkout | `{{repo_checkout}}` |
| Work subdirectory | `{{work_subdir}}` |
| Worktree root | `{{worktree_root}}` |
| Base branch | `{{base_branch}}` |
| Branch prefix | `{{branch_prefix}}` |
| Require human spec review | `{{require_human_spec_review}}` |
| Publication mode | `{{publication_mode}}` |
| PR push remote | `{% if pr_push_remote %}{{pr_push_remote}}{% else %}<infer>{% endif %}` |
| PR head owner | `{% if pr_head_owner %}{{pr_head_owner}}{% else %}<infer>{% endif %}` |
| PR labels | `{{pr_labels}}` |

## Validation Commands

{% if validation_commands %}
{% for command in validation_commands %}
- `{{ command }}`
{% endfor %}
{% else %}
- Use validation commands discovered from the target repository's `AGENTS.md`.
{% endif %}

{% if extra_context %}
## Extra Context

{{ extra_context | trim }}
{% endif %}
