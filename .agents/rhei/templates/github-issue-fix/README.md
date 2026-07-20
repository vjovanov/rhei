# github-issue-fix

Fix one GitHub issue through a spec-aware, reviewable workflow. The template
creates an isolated worktree, fetches the issue, discovers target-repository
instructions such as `AGENTS.md` and grund configuration, records a spec-fit
verdict, and then routes the issue to implementation, human review, or GitHub
handoff. Vague or underspecified issues route to a GitHub clarification handoff
instead of a speculative implementation. Implemented fixes pass through
validation, focused review cycles, and optional PR publication.

## Inputs

| Name | Type | Default | Description |
|---|---|---|---|
| `issue` | string | required | GitHub issue number or URL. |
| `repo` | string | required | GitHub repository in `owner/name` form. |
| `repo_checkout` | path | required | Local checkout used as the source for the issue worktree. |
| `work_subdir` | string | `.` | Subdirectory inside the worktree where implementation commands run. |
| `worktree_root` | string | `runtime/worktrees` | Directory where the issue worktree is created. |
| `base_branch` | string | `main` | Base branch for the issue branch and PR. |
| `branch_prefix` | string | `rhei` | Prefix for the issue branch. |
| `require_human_spec_review` | boolean | `true` | Whether compatible issues still stop for human review before implementation. |
| `publication_mode` | string | `draft` | `no-pr` for local artifacts only, `draft`, or `ready`. |
| `pr_push_remote` | string | empty | Writable git remote for pushing the issue branch. |
| `pr_head_owner` | string | empty | GitHub owner/login for PR heads. |
| `pr_labels` | array<string> | `rhei` | Labels to apply to the PR when they already exist on the target repository. |
| `validation_commands` | array<string> | empty | Explicit validation commands that must run; otherwise validation defaults to focused issue-specific checks plus cheap targeted repo checks. |
| `implementation_target` | string | `codex[yolo]:openai:gpt-5.6-sol` | Agent for intake, implementation, validation fixes, and review repairs. |
| `operations_target` | string | `codex[yolo]:openai:gpt-5.6-luna` | Agent for procedural GitHub handoffs and publication records. |
| `review_target` | string | `codex[yolo]:openai:gpt-5.6-terra` | Agent for focused requirements, spec, implementation, and validation reviews. |
| `aggregate_review_target` | string | `codex[yolo]:openai:gpt-5.6-sol` | Agent that combines focused review results into a publication-readiness decision. |
| `review_passes` | number | `1` | Minimum number of focused review cycles before publication; override it for additional clean review cycles. |
| `review_fix_attempts` | number | `2` | Additional review/fix cycles allowed when aggregate review finds blocking issues. |
| `plan_title` | string | `GitHub Issue Fix` | Rendered workspace title. |
| `extra_context` | string | empty | Extra project-specific guidance. |

## State Paths

| Path | States |
|---|---|
| Intake | `issue-intake -> completed` after writing artifacts and one follow-up task. |
| Compatible issue | `implement-fix -> implementation-dispatch -> validate-fix -> requirements-review -> spec-review -> implementation-review -> validation-review -> aggregate-review -> review-dispatch -> address-review -> validate-fix -> ... -> publish-pr -> completed` |
| Exhausted review repair | `review-dispatch -> record-blocked-publication -> completed` |
| Human gate | `human-review -> implement-fix` or `human-review -> github-handoff` or `human-review -> cancelled` |
| Blocked or unclear issue | `github-handoff -> completed` locally, without issue comments or PR publication. |

The state-machine diagram is documented at the top of `states.yaml`.

## Flow

1. `issue-intake` creates or reuses a branch and worktree for the issue.
2. It fetches the GitHub issue and writes a durable snapshot.
3. It reads applicable repository instructions, nested `AGENTS.md` files, and
   grund configuration when present.
4. It writes an adequacy/spec-fit verdict and routing note. Issues without
   enough detail to name the likely change and validation path are routed to
   a local handoff for clarification.
5. It creates one follow-up task in `implement-fix`, `human-review`, or
   `github-handoff`.
6. Implementation writes a durable `ready` or `blocked` result. Ready fixes are
   validated; blocked implementations route to GitHub handoff without review or
   publication. Ready fixes are then reviewed through separate requirements,
   spec/grund, implementation-quality, and validation-readiness reviews. An
   aggregate review turns those focused findings into one PR-readiness decision.
   Validation defaults to focused checks for the changed behavior plus cheap
   targeted repo checks. Expensive full suites, exact CI matrices, full builds,
   and documentation renders are recorded as validation gaps unless explicitly
   configured or required by the change.
7. `review-dispatch` reads the aggregate review's machine-readable readiness
   markers. Ready fixes publish only after the required `review_passes`. Draft
   PRs may publish with disclosed broad validation gaps when requirements,
   spec/grund, implementation, and focused validation are clean. Not-ready fixes
   route back through `address-review` while `review_fix_attempts` remain. When
   attempts are exhausted, `record-blocked-publication` records a blocked local
   result instead of pushing an unsafe PR.
8. Publication follows `publication_mode`. `no-pr` performs no external GitHub
   writes. Published PRs apply configured labels such as `rhei` only when those
   labels already exist on the target repository; the workflow does not create
   labels. Published PR descriptions are written in a user-facing format with
   `## What changed`, `## Why`, `## Example` when meaningful, `## Implementation
   summary`, and `## Validation`. The workflow avoids internal review sections
   such as spec-fit summaries, review readiness, or validation-gap bookkeeping
   in the PR body itself.

## Usage

```sh
rhei instantiate github-issue-fix 1234 \
  --set repo=owner/repo \
  --set repo_checkout=/path/to/repo \
  --set base_branch=main \
  --set publication_mode=draft \
  --output .agents/scratchpad/issue-1234

rhei run .agents/scratchpad/issue-1234
```

For a first trial, use `publication_mode=no-pr` so the workflow produces only
local artifacts. It will not push, open or update a PR, or post issue comments:

```sh
rhei instantiate github-issue-fix 1234 \
  --set repo=owner/repo \
  --set repo_checkout=/path/to/repo \
  --set publication_mode=no-pr \
  --output .agents/scratchpad/issue-1234-local
```

A rendered smoke example lives at
`examples/github-issue-fix-example/`.

To require additional clean review cycles before publication, pass
`--set review_passes=<count>` when instantiating the template.
