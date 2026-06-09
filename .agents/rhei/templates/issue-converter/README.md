# issue-converter

General GitHub issue queue to executable Rhei workspace converter.

```sh
rhei instantiate issue-converter \
  --set repo=owner/repo \
  --set project_owner=owner \
  --set project_number=1 \
  --set repo_checkout=/path/to/repo \
  --set work_subdir=. \
  --set worktree_root=/path/to/worktrees \
  --set author=octocat \
  --set limit=5 \
  --set candidate_limit=50 \
  --set pr_push_remote=my-fork \
  --set pr_head_owner=octocat \
  --set pr_base_branch=master \
  --set sleep=10m \
  --set max_batches=20 \
  --output issues-octocat
```

Then run the generated workspace by its rhei id under the Panta project root:

```sh
rhei run issues-octocat --parallel 2
```

Run with `--parallel 2` or higher so the converter can keep batching while
generated issue tasks advance in their own worktrees.

The first task fetches at most `candidate_limit` exact issue candidates from the
repository using the configured author, label, state, and search filters, then
verifies each candidate's GitHub Project item and Status directly. It converts
up to `limit` matching Project issue items in Status `Todo`, writes batch
inventory files under `runtime/issues/batches/`, updates
`runtime/issues/conversion.md`, creates or reuses one git worktree per converted
issue, marks converted project items `In Progress`, then appends one independent
issue plan file containing a four-task chain plus a final report task. Vague or
broad issues are converted into a spec-inspection task before implementation
instead of being skipped. If more matching Todo issue candidates remain, Rhei
sleeps for `sleep` and loops back for another bounded batch until the queue is
empty or `max_batches` is reached.

For each issue, the converter generates:

- `issue-<number>-spec`: inspect docs/specs and request `human-review` from the
  task if spec/docs changes or product/spec decisions need approval.
- `issue-<number>-implementation`: implement the smallest approved change in
  the per-issue worktree.
- `issue-<number>-verification`: final local review plus E2E/integration tests.
- `issue-<number>-pr`: push the verified branch and open a PR linked to the
  resolved issue. Configure `pr_push_remote`/`pr_head_owner` when the checkout
  has a known writable fork remote; otherwise the task tries to infer one and
  fails clearly if it cannot.

Those tasks are written together in one file such as
`tasks/02-issue-7850.md`; they are not separate files. The first task in each
issue chain is an independent root task, so multiple converted issues can run as
separate graphs under `rhei run --parallel N`.

Generated work tasks start in `pending`, which is only a queued/generated state.
When picked up by `rhei run`, `pending` advances immediately to `execute`; the
actual task work happens there. Work tasks then use a review-summary router:
`pending -> execute -> agent-review -> review-dispatch -> completed`, or
`review-dispatch -> agent-review-fix -> agent-review` when the latest review
summary says fixes are needed. `agent-review` always writes the latest
`runtime/issues/review-summary/<task-id>.md` with a `**Needs fixes:** yes/no`
line; `review-dispatch` greps that line and routes deterministically. Small,
stylistic, optional, or non-blocking review notes should set the marker to
`**Needs fixes:** no`, allowing the task to complete. `human-review` remains the
human gate for spec/product decisions and returns rework to `execute`.

Useful inputs:

- `repo`: GitHub repository in `owner/name` form.
- `project_owner`: GitHub Projects owner login or `@me`.
- `project_number`: GitHub Projects project number.
- `repo_checkout`: local git checkout root used to create issue worktrees.
- `work_subdir`: subdirectory inside each worktree where implementation commands
  should run; default `.`.
- `worktree_root`: directory where the converter creates per-issue worktrees.
- `todo_status`: project Status value to fetch, default `Todo`.
- `in_progress_status`: project Status value to set after task generation,
  default `In Progress`.
- `author`: optional issue author filter.
- `limit`: maximum issue chains to generate in each batch.
- `candidate_limit`: maximum exact issue-search candidates to inspect in each
  batch before Project status verification.
- `sleep`: delay between bounded batches, such as `30s`, `10m`, or `1h`; use
  `0s` to poll immediately.
- `max_batches`: safety cap for conversion batches.
- `state`: `open`, `closed`, or `all`.
- `labels`: optional comma-separated labels.
- `search`: optional extra GitHub query terms.
- `pr_push_remote`: writable fork remote used by generated PR tasks; optional
  when the task can infer one.
- `pr_head_owner`: GitHub owner/login for the PR head; optional when it can be
  inferred from `pr_push_remote`.
- `pr_base_branch`: target branch for generated PRs, default `master`.
- `conversion_brief`: project-specific rules for what counts as executable.
- `reviewer_agent`: agent that reviews generated task results.

State-machine paths are documented in [states.yaml](states.yaml).

A rendered, runnable instantiation of this template is checked in at
[`examples/issue-converter-example/`](../../../../examples/issue-converter-example).
