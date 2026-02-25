# changeset-review

Review a code change (PR, branch, commit, commit range, or diff file) by
splitting it into logical parts, running a multi-target review over each
part, scoring findings, gating on a human, and fanning out targeted fix
tasks.

Instantiate this workspace inside the repository being reviewed. The
instantiated directory is a scratchpad, not the source tree itself, so
agents must resolve the Git toplevel first and inspect or edit files from
there (or from any prepared worktree/fork the workflow creates later).

## Inputs

| Input | Type | Default | Description |
|---|---|---|---|
| `change_ref` | string | *(required)* | PR URL/number, branch, commit SHA, `base..head` range, or `.diff`/`.patch` file path |
| `review_targets` | string[] | `[claude-code[yolo]:anthropic:claude-opus-4-7, codex[yolo]:openai:gpt-5.4]` | Execution targets that independently review each part (one parallel pass per target) |
| `review_focus` | string[] | `[]` | Optional focus subsections each reviewer must address (e.g., `performance`, `security`, `concurrency`). Empty means general review |
| `aggregator_target` | string | `claude-code[yolo]:anthropic:claude-opus-4-7` | Execution target that deduplicates and scores findings |
| `fix_target` | string | `claude-code[yolo]:anthropic:claude-opus-4-7` | Execution target that implements each human-approved fix |
| `fix_prepare` | string | `none` | Optional pre-fix workspace isolation: `none`, `branch`, `worktree`, `fork` |
| `fix_commit` | string | `none` | Optional post-fix commit step: `none`, `commit`, `push`, `pr` |

## State machine

The diagram lives as a comment at the top of [`states.yaml`](./states.yaml).
Per-task paths through the machine:

| Task | Path through the machine |
|---|---|
| coordinator | `split` → `completed` |
| `review-<slug>` (one per part) | `review` → `completed` (fans out across `review_targets`) |
| `aggregate` | `aggregate` → `human-review` → `fix-spawn` → `completed` (or → `completed` if nothing marked `[fix]`) |
| `fix-<issue-id>` (one per `[fix]`) | `[prepare-workspace →] fix [→ commit-fix] → completed` — bracketed stages are present only when `fix_prepare` / `fix_commit` are non-`none` |

## Flow

1. The coordinator resolves `change_ref` with VCS tooling and writes an
   architectural overview that includes a Metadata subsection (PR body
   if applicable, commit messages, intersecting specs) and a
   "User-facing?" call.
2. Based on the overview the coordinator decides which review parts
   to spawn:
    - **code parts** — one per logical slice of the diff
    - `pr-description` — only if the change arrived as a PR
    - `commit-messages` — always, to flag mess / wip / fixup commits
    - `documentation` — only if the change is user-facing
    - `spec` — only if the project has specs intersecting the subsystems
      touched
3. Each part is reviewed once per configured target via `all_targets`.
   If `review_focus` is set, every reviewer must produce a subsection
   per focus area.
4. The aggregator merges per-part / per-target / per-focus findings
   into a scored issue list, then transitions to `human-review`.
5. The human edits the issue list in place — flipping `[ ]` to `[fix]`
   or `[skip]` and filling in `Approach:` — and transitions to
   `fix-spawn`.
6. `fix-spawn` appends one fix task per `[fix]` entry. Each fix runs
   on `fix_target`, optionally in an isolated workspace
   (`fix_prepare`) and optionally followed by a commit step
   (`fix_commit`).

## Instantiate

```bash
rhei instantiate changeset-review \
  --set change_ref=PR#42 \
  --set review_targets='["claude-code[yolo]:anthropic:claude-opus-4-7","codex[yolo]:openai:gpt-5.4"]' \
  --set review_focus='["performance","security","concurrency"]' \
  --output ./.agents/scratchpad/changeset-review/
```

## Example

A pre-rendered example lives at [`examples/changeset-review-example/`](../../../../examples/changeset-review-example/)
and passes `rhei validate` as shipped.
