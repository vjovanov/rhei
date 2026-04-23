# Rhei: Changeset Review — PR#42
**States:** changeset-review

## Overview
Multi-target review with a human-gated fix fanout.

The change under review (`PR#42`) can be any of:

- a PR URL or PR number
- a branch name
- a commit SHA or range `base..head`
- a path to a `.diff` / `.patch` file

The coordinator resolves the reference to a concrete set of changed files
before splitting.

The instantiated workspace is a review scratchpad. Agents must resolve the
repository root via Git and inspect repository files from that root, not
from this workspace directory.

Flow:

1. The coordinator task resolves `PR#42` and writes an
   architectural overview (intent, subsystems touched, new contracts,
   cross-cutting concerns, risks), then splits the change into logical
   parts. It spawns one `review-<slug>` task per part and one
   `aggregate` task whose `**Prior:**` waits on every part review.
2. Each part is reviewed independently by every configured target:
   - `claude-code[yolo]:anthropic:claude-opus-4-7`
   - `codex[yolo]:openai:gpt-5.4`

   Each reviewer must organize findings into these focus subsections:
   - `performance`
   - `security`
   - `concurrency`
3. The aggregator (`claude-code[yolo]:anthropic:claude-opus-4-7`) deduplicates findings across
   parts, targets, and focus areas, then writes a scored issue list.
4. A human edits the issue list in place to mark each issue `[fix]` or
   `[skip]` and fill in an approach for fixes.
5. The aggregator task transitions to `fix-spawn`, which appends one fix
   task per `[fix]` entry. Each fix runs on `claude-code[yolo]:anthropic:claude-opus-4-7`.

## Notes

- The workspace is "living": the coordinator and the aggregator both add
  task files under `tasks/` during the run. `rhei reset` clears state but
  does not delete dynamically appended task files.
- Instantiate the workspace inside the repository under review, ideally
  under `.agents/scratchpad/`, so `git rev-parse --show-toplevel` from the
  workspace resolves the project root deterministically.
- If the human closes `human-review` with nothing marked `[fix]`, the
  aggregator transitions straight to `completed` and no fix tasks are
  spawned.