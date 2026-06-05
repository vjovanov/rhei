# Rhei: Issue Converter
**States:** issue-converter

## Overview

This workspace converts GitHub project items from `octocat/hello-world` into executable
Rhei task files. The converter fetches at most 50 exact issue
candidate(s) using the configured issue filters, verifies each candidate's
Project item and Status directly, converts at most 5 issue item(s)
whose project Status is `Todo`, creates a Rhei task file, marks each
converted project item `In Progress`, sleeps for `10m`, and
then checks for more matching `Todo` candidates until none remain or
20 batch(es) have run. It records the inventory and conversion
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
| Repository | `octocat/hello-world` |
| Source checkout | `/path/to/hello-world` |
| Work subdirectory | `.` |
| Worktree root | `/path/to/worktrees` |
| Project owner | `octocat` |
| Project number | 1 |
| TODO status | `Todo` |
| In-progress status | `In Progress` |
| Author | `octocat` |
| State | `open` |
| Labels | `<none>` |
| Search | `<none>` |
| Batch limit | 5 |
| Candidate query limit | 50 |
| Sleep between batches | `10m` |
| Max batches | 20 |
| PR push remote | `<infer>` |
| PR head owner | `<infer>` |
| PR base branch | `master` |

## Conversion Brief

Convert every matching non-duplicate issue into executable Rhei tasks. If the issue is vague or broad, create a spec-inspection task before implementation instead of skipping it. Only skip exact duplicates, non-issue project items, or inaccessible items that cannot be inspected.

## Agents

| Role | Agent |
|---|---|
| Converter | `codex[yolo]:openai:gpt-5.5` |
| Issue worker / report | `codex[yolo]:openai:gpt-5.5` |
| Agent reviewer | `codex[yolo]:openai:gpt-5.5` |
