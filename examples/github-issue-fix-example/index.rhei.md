# Rhei: GitHub Issue Fix Example
**States:** github-issue-fix

## Overview

This workspace fixes one GitHub issue from `vjovanov/rhei`: `1234`.

The first task creates or reuses an isolated worktree from `/tmp`,
fetches the issue, discovers repository instructions and grounding configuration,
records a spec-fit artifact, and writes exactly one follow-up task. The follow-up
task starts in proposal approval inspection, local proposal generation, or
GitHub handoff according to the recorded verdict and publication mode.
Compatible externally published issues recover or publish a content-addressed
proposal and require an authorized exact GitHub approval before implementation.
`no-pr` uses a local proposal and human gate with zero GitHub writes. Approved
work proceeds through validation, focused review/fix cycles, and optional PR
publication; blocked, incompatible, unclear, or attempt-exhausted work produces
a local handoff.

## Source

| Field | Value |
|---|---|
| Repository | `vjovanov/rhei` |
| Issue | `1234` |
| Source checkout | `/tmp` |
| Work subdirectory | `.` |
| Worktree root | `runtime/worktrees` |
| Base branch | `main` |
| Branch prefix | `rhei` |
| Publication mode | `no-pr` |
| Rhei GitHub actor | `rhei[bot]` |
| Proposal attempt limit | `3` |
| PR push remote | `<infer>` |
| PR head owner | `<infer>` |
| PR labels | `["rhei"]` |

## Validation Commands


- Use validation commands discovered from the target repository's `AGENTS.md`.



