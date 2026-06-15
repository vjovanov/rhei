# github-issue-fix example

This is a rendered smoke example for the `github-issue-fix` template.
It includes the issue-adequacy routing behavior: unclear issues should route to
GitHub handoff for a clarification request instead of implementation.
Implemented fixes use two focused review cycles. Each cycle separates
requirements, spec/grund, implementation, and validation review before the
aggregate PR-readiness decision.

## Values

| Input | Value |
|---|---|
| `issue` | `1234` |
| `repo` | `vjovanov/rhei` |
| `repo_checkout` | `.` |
| `publication_mode` | `no-pr` |
| `base_branch` | `main` |
| `review_passes` | `2` |
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
  --set 'plan_title=GitHub Issue Fix Example' \
  --output examples/github-issue-fix-example
```

## Validate

```sh
cargo run -p rhei-cli -- validate examples/github-issue-fix-example
cargo run -p rhei-cli -- run examples/github-issue-fix-example --dry-run
```
