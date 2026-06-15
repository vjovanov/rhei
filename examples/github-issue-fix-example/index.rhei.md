# Rhei: GitHub Issue Fix Example
**States:** github-issue-fix

## Overview

This workspace fixes one GitHub issue from `vjovanov/rhei`: `1234`.

The first task creates or reuses an isolated worktree from `/home/jovan/Work/rhei/.`,
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
| Repository | `vjovanov/rhei` |
| Issue | `1234` |
| Source checkout | `/home/jovan/Work/rhei/.` |
| Work subdirectory | `.` |
| Worktree root | `runtime/worktrees` |
| Base branch | `main` |
| Branch prefix | `rhei` |
| Require human spec review | `true` |
| Publication mode | `no-pr` |
| PR push remote | `<infer>` |
| PR head owner | `<infer>` |
| PR labels | `["rhei"]` |

## Validation Commands


- Use validation commands discovered from the target repository's `AGENTS.md`.



