# github-issue-fix

Fix one GitHub issue through a spec-aware, reviewable workflow. The template
creates an isolated worktree, fetches the issue, discovers target-repository
instructions such as `AGENTS.md` and grund configuration, records a spec-fit
verdict, and then routes the issue to proposal approval or a local GitHub
handoff. Vague or underspecified issues never receive a speculative
implementation. Approved fixes pass through validation, focused review cycles,
and optional PR publication.

Issue titles, bodies, comments, attachments, linked content, and reproduction
instructions are treated as untrusted evidence during intake. The intake agent
must not execute issue-supplied commands, follow arbitrary issue-supplied URLs,
access secrets, or make external GitHub writes. This is prompt-level
defense-in-depth; users should still isolate the configured agent when issues
may be actively hostile.

## Proposal approval contract

Compatible issues receive an implementation proposal before code changes begin.
The proposal names the accepted issue scope, the repository rules and
specification points that constrain it, the intended file and behavior changes,
the validation strategy, material risks, and any known gaps. Proposal prose is
canonicalized and hashed with SHA-256; the first 16 lowercase hexadecimal
characters are the proposal ID. Changing the substantive proposal content
therefore creates a new ID.

For `draft` and `ready` publication modes, Rhei publishes proposals as issue
comments owned by the configured Rhei actor. A supported comment contains
exactly one `<!-- rhei-proposal:v1 id=<proposal-id> attempt=<n> -->` marker.
Only marked comments authored by that actor participate in routing. The latest
such comment is the current proposal; an approval or rejection for any older ID
is stale. GitHub comments, including rejection feedback, remain untrusted
evidence and never become agent instructions.

The only accepted decisions are an exact first line of either:

```text
/rhei approve <proposal-id>
/rhei reject <proposal-id>
```

Reject commands may be followed by free-form feedback on later lines. Commands
with prefixes, suffixes, alternate whitespace, or a stale ID are ignored. At
decision time the command author must currently have GitHub `write`, `maintain`,
or `admin` permission on the configured repository. Outside collaborators and
users with `read` or `triage` permission cannot decide. The configured Rhei
actor may approve or reject its own proposal when it has one of the qualifying
repository permissions.

Publication is idempotent: a proposal marker is checked before posting, so a
retry never duplicates the comment. Rhei then applies the single pre-existing
`rhei:awaiting-approval` label; it never creates labels. Approval removes the
label immediately before implementation. Rejection removes it while a revision
is prepared and reapplies it only after the revised proposal is posted.
Pending, malformed, stale, and unauthorized decisions leave the label
unchanged. Missing labels, permission failures, and malformed GitHub responses
produce a durable blocker and never silently start implementation. Partial
failures are safe to retry.

GitHub comments are the cross-run source of truth. A fresh instantiation
reconstructs the latest proposal and its valid decision from issue metadata,
allowing an approved proposal to proceed without reposting or replanning.
Runtime artifacts are durable audit evidence, not a prerequisite for rerun
recovery. Rejections create a revised proposal while attempts remain; the
default limit is three total proposals, including the initial attempt. Once
exhausted, the workflow produces the existing local GitHub handoff.

Every proposal ends with its actual ID, copy-paste approval and rejection
commands, disclosure that the proposal was AI-generated, the resolved
`provider:model` from the completed proposal-generation invocation record, and
a link to [Rhei](https://github.com/vjovanov/rhei). Missing model evidence is
reported as `not reported`, never guessed. Local handoffs carry the same compact
provenance inside their suggested issue comment.

`publication_mode=no-pr` is strictly local: it generates a proposal artifact
and uses the existing local `human-review` gate, but never reads decisions from
issue comments, posts a proposal, changes a label, pushes, or opens or updates a
PR. `github-handoff` is local-only in every publication mode; a human may choose
to post its provenance-bearing suggested comment.

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
| `publication_mode` | string | `draft` | `no-pr` for local artifacts only, `draft`, or `ready`. |
| `rhei_actor` | string | `rhei[bot]` | GitHub actor that owns proposal comments and may decide when repository-authorized. |
| `proposal_attempts` | number | `3` | Total proposal attempts, including the initial proposal. |
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
| New external proposal | `approval-check -> propose-fix -> publish-proposal -> proposal-pending`. |
| Pending external proposal | `approval-check -> proposal-pending`; no duplicate comment or label mutation. |
| Approved external proposal | `approval-check -> approval-apply -> implement-fix`. |
| Rejected external proposal | `approval-check -> rejection-prepare -> propose-fix -> publish-proposal -> proposal-pending`, or `github-handoff` after exhaustion. |
| Local-only proposal | `propose-fix -> publish-proposal -> human-review -> implement-fix`, with no GitHub writes. |
| Approved implementation | `implement-fix -> implementation-dispatch -> validate-fix -> requirements-review -> spec-review -> implementation-review -> validation-review -> aggregate-review -> review-dispatch -> ... -> publish-pr -> completed`. |
| Exhausted review repair | `review-dispatch -> record-blocked-publication -> completed` |
| Material design divergence | `implementation-dispatch -> propose-fix`, requiring a new proposal ID and approval. |
| Blocked or unclear issue | `github-handoff -> completed` locally, without issue comments or PR publication. |

The state-machine diagram is documented at the top of `states.yaml`.

## Flow

1. `issue-intake` creates or reuses a branch and worktree for the issue.
2. It fetches the GitHub issue as untrusted evidence and writes a durable
   snapshot. Suspected prompt injection is recorded as a risk, never followed
   as agent instruction.
3. It reads applicable repository instructions, nested `AGENTS.md` files, and
   grund configuration when present.
4. It writes an adequacy/spec-fit verdict and routing note. Issues without
   enough detail to name the likely change and validation path are routed to
   a local handoff for clarification.
5. It creates one follow-up task in `approval-check`, `propose-fix`, or
   `github-handoff`. External modes inspect actor-owned proposal markers and
   exact authorized decisions before any design or code change. A missing
   proposal is generated and published once; a pending proposal ends the
   current run. A later fresh run recovers approval or rejection from GitHub.
   `no-pr` renders the same content-addressed proposal locally and stops at the
   local human gate without invoking `gh`.
6. Implementation is bound to the exact approved proposal and writes a durable
   `ready`, `reproposal`, or `blocked` result. A material approach change routes
   through a new proposal ID and approval rather than diverging silently. Ready fixes are
   validated; blocked implementations route to GitHub handoff without review or
   publication. Validation also writes a compact, current-cycle review brief
   containing shared scope, change, rule, and validation evidence without review
   conclusions. Ready fixes are then reviewed through separate requirements,
   spec/grund, implementation-quality, and validation-readiness reviews. Each
   focused reviewer reads the shared brief plus only its authoritative specialist
   evidence, without consuming earlier focused-review conclusions. The aggregate
   review alone reads all four focused findings and turns them into one
   PR-readiness decision.
   When the target repository has a spec citation/reference convention, every
   added or modified test source file must carry the most-specific applicable
   spec reference, including helpers, fixtures, and infrastructure-only test
   sources. Spec review blocks missing or unsuitable references, while
   implementation review checks that each reference is applicable to the test
   file.
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
   summary`, and `## Validation`, followed by a final collapsible `## AI
   workflow` provenance section. That section links to Rhei, lists every
   executed agent step with its resolved model, reasoning effort, and available
   total/input/cached/output token metrics, and places aggregate usage after the
   steps. An effort that is not exposed by durable execution evidence is shown
   as `not reported`, never guessed. The active publication step is marked as
   not finalized when its own token record is not yet available. Other internal
   review details such as spec-fit summaries, review readiness, or
   validation-gap bookkeeping stay out of the PR body.

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
local artifacts. It will not invoke GitHub writes, push, open or update a PR,
post issue comments, or change the approval label:

```sh
rhei instantiate github-issue-fix 1234 \
  --set repo=owner/repo \
  --set repo_checkout=/path/to/repo \
  --set publication_mode=no-pr \
  --output .agents/scratchpad/issue-1234-local
```

A rendered smoke example lives at
`examples/github-issue-fix-example/`.

Before using `draft` or `ready`, create the `rhei:awaiting-approval` label in
the target repository and configure `rhei_actor` to the authenticated publishing
login. Rhei checks that the label exists but never creates it.

After a proposal is posted, copy one command from its footer into a new issue
comment. Approval is the exact first line:

```text
/rhei approve <proposal-id>
```

Rejection uses the exact first line and optional feedback below it:

```text
/rhei reject <proposal-id>
<explain what should change>
```

Start a fresh instantiation after posting the decision. The new run recovers the
current proposal and decision from GitHub comments; it does not require the
previous run's runtime directory.

To require additional clean review cycles before publication, pass
`--set review_passes=<count>` when instantiating the template.

## Regenerating the example

The committed local-only example and its values file are checked for byte-level
drift. Regenerate the rendered files with:

```sh
cargo run -p rhei-cli -- instantiate \
  .agents/rhei/templates/github-issue-fix \
  --values .agents/rhei/templates/github-issue-fix/.example-values.yaml \
  --output examples/github-issue-fix-example
```

Keep the example's hand-written `README.md` and
`instantiation-values.yaml`; the latter must remain byte-identical to the
template's `.example-values.yaml`.
