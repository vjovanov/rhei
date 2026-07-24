# github-issue-fix example

This is the reproducibly rendered, local-only smoke example for the
`github-issue-fix` template. Compatible issues receive a content-addressed
implementation proposal before code changes begin. Because this example uses
`publication_mode=no-pr`, the proposal is rendered locally and stops at the
human gate: it never posts a comment, reads an approval command, changes the
`rhei:awaiting-approval` label, pushes, or opens or updates a PR.

External `draft` and `ready` instantiations instead publish one proposal comment,
apply the pre-existing approval label, and accept only an exact
`/rhei approve <proposal-id>` or `/rhei reject <proposal-id>` first line from a
current repository member with write, maintain, or admin permission, including
the configured publishing actor. A fresh run recovers the latest actor-owned
proposal and decision from GitHub comments. Rejections revise the proposal up
to the configured total attempt limit.

Unclear issues route to a local GitHub handoff instead of implementation.
Approved fixes use focused review cycles separated by requirements,
spec/grund, implementation, and validation review. Aggregate review blockers
route through a bounded repair loop before publication. Focused validation is
the default; broad validation gaps are disclosed for draft publication instead
of blocking by themselves. Validation also produces a compact per-cycle review
brief. Each focused reviewer reads that shared brief plus only its specialist
evidence, while aggregate review alone consumes all four focused findings.

Issue-controlled content is treated as untrusted evidence during intake. The
agent must not execute issue-supplied commands, follow arbitrary links, access
secrets, or make external GitHub writes, and it records suspected prompt
injection as a spec-fit risk.

When the target repository has a spec reference convention, the workflow
requires every added or modified test source file to cite the most-specific
applicable spec point, including helpers, fixtures, and infrastructure-only
test sources. Spec review checks citation compliance, and implementation review
checks that each reference applies to the test file.

Published PR descriptions end with a collapsible `## AI workflow` section that
links to Rhei and records each executed agent step, its resolved model, reasoning
effort, available total/input/cached/output token metrics, aggregate usage,
review-cycle counts, and accounting coverage. An effort unavailable from durable
execution evidence is shown as `not reported`. The active publication step is
explicitly marked as not finalized when its own accounting record is not yet
available.

## Values

| Input | Value |
|---|---|
| `issue` | `1234` |
| `repo` | `vjovanov/rhei` |
| `repo_checkout` | `/tmp` |
| `publication_mode` | `no-pr` |
| `base_branch` | `main` |
| `rhei_actor` | `rhei[bot]` |
| `proposal_attempts` | `3` |
| `implementation_target` | `codex[yolo]:openai:gpt-5.6-sol` |
| `operations_target` | `codex[yolo]:openai:gpt-5.6-luna` |
| `review_target` | `codex[yolo]:openai:gpt-5.6-terra` |
| `aggregate_review_target` | `codex[yolo]:openai:gpt-5.6-sol` |
| `review_passes` | `1` |
| `review_fix_attempts` | `2` |
| `pr_labels` | `["rhei"]` |
| `plan_title` | `GitHub Issue Fix Example` |

The issue number is intentionally just example data; validation checks the
rendered workspace shape and helper behavior, not GitHub reachability.

## Regenerate

```sh
cargo run -p rhei-cli -- instantiate \
  .agents/rhei/templates/github-issue-fix \
  --values .agents/rhei/templates/github-issue-fix/.example-values.yaml \
  --output examples/github-issue-fix-example
```

## Validate

```sh
cargo run -p rhei-cli -- validate examples/github-issue-fix-example
cargo run -p rhei-cli -- run examples/github-issue-fix-example --dry-run
```
