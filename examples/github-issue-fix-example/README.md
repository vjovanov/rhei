# github-issue-fix example

This is a rendered smoke example for the `github-issue-fix` template.
It includes the issue-adequacy routing behavior: unclear issues should route to
GitHub handoff for a clarification request instead of implementation.
Implemented fixes use focused review cycles separated by requirements,
spec/grund, implementation, and validation review. Aggregate review blockers
route through a bounded repair loop before publication. Focused validation is
the default; broad validation gaps are disclosed for draft publication instead
of blocking by themselves.

When the target repository has a spec reference convention, the workflow also
requires added or changed behavioral tests to cite the most-specific applicable
spec point, checks citation compliance in spec review, and checks behavioral
alignment in implementation review.

Published PR descriptions end with a collapsible `## AI workflow` section that
links to Rhei and records each executed agent step, its resolved model, available
total/input/cached/output token metrics, aggregate usage, review-cycle counts,
and accounting coverage. The active publication step is explicitly marked as
not finalized when its own accounting record is not yet available.

## Values

| Input | Value |
|---|---|
| `issue` | `1234` |
| `repo` | `vjovanov/rhei` |
| `repo_checkout` | `.` |
| `publication_mode` | `no-pr` |
| `base_branch` | `main` |
| `implementation_target` | `codex[yolo]:openai:gpt-5.6-sol` |
| `operations_target` | `codex[yolo]:openai:gpt-5.6-luna` |
| `review_target` | `codex[yolo]:openai:gpt-5.6-terra` |
| `aggregate_review_target` | `codex[yolo]:openai:gpt-5.6-sol` |
| `review_passes` | `1` |
| `review_fix_attempts` | `2` |
| `pr_labels` | `["rhei"]` |
| `plan_title` | `GitHub Issue Fix Example` |

`publication_mode=no-pr` keeps the smoke example local-only if it is ever run:
it must not push, open or update PRs, or post issue comments. The issue number
is intentionally just example data; validation checks the rendered workspace
shape, not GitHub reachability.

## Regenerate

```sh
cargo run -p rhei-cli -- instantiate github-issue-fix 1234 \
  --set repo=vjovanov/rhei \
  --set repo_checkout=. \
  --set publication_mode=no-pr \
  --set base_branch=main \
  --set review_passes=1 \
  --set review_fix_attempts=2 \
  --set 'plan_title=GitHub Issue Fix Example' \
  --output examples/github-issue-fix-example
```

## Validate

```sh
cargo run -p rhei-cli -- validate examples/github-issue-fix-example
cargo run -p rhei-cli -- run examples/github-issue-fix-example --dry-run
```
