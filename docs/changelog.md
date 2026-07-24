# Changelog

## Unreleased

- Let `github-issue-fix` generate an initial proposal without a nonexistent
  prior proposal artifact and recover prior proposal evidence during revisions.
- Allow the configured `github-issue-fix` publishing actor to approve or reject
  its own proposal when it has write, maintain, or admin repository permission.
- Make `github-issue-fix` publish a content-addressed AI implementation proposal
  before external code work, require an exact approval from a current
  write/maintain/admin repository member, support bounded rejection revisions
  and fresh-run recovery, and preserve `no-pr` as a zero-write local human gate.
- Include each model's resolved reasoning effort in the `github-issue-fix` PR
  description's `AI workflow` provenance, with an explicit `not reported`
  fallback when durable execution evidence does not expose it.
- Make `github-issue-fix` validation produce a compact per-cycle review brief,
  give each focused reviewer only its specialist evidence, and reserve the full
  four-review context for aggregation so review prompts do not grow cumulatively.
- Make `github-issue-fix` intake treat issue-controlled content as untrusted
  evidence, prohibit issue-supplied commands and external writes, and record
  suspected prompt injection as a spec-fit risk.
- Make `github-issue-fix` require every added or modified test source file,
  including helpers, fixtures, and infrastructure-only tests, to carry the
  most-specific applicable spec reference when the target repository has a
  citation convention.
- Make `github-issue-fix` create and format the PR description only after
  aggregate review is green, instead of requiring an unavailable planned
  description during review.
- Keep `github-issue-fix` handoffs local instead of posting internal blocked
  workflow evidence as GitHub issue comments.
- Route a blocked `github-issue-fix` implementation through a durable handoff
  instead of leaving the workflow waiting on a missing implementation artifact.
- Use GPT-5.6 Terra for implementation work, Luna for focused reviews, and Sol
  for aggregate review in the `github-issue-fix` template.
- Default `github-issue-fix` to one focused review cycle; callers can still
  require additional cycles with `review_passes`.
- Add durable task state history to Flow/dashboard and the `rhei run` TUI,
  including the `state history` surroundings section, prompt-focused inspector
  navigation, a global Machine legend with process-kind styling, and links-only
  shared chrome. PR #48 §FS-rhei-viz.4 §FS-rhei-run-tui.1.5
- Add Codex token accounting from `turn.completed` JSON usage, persisted runtime
  accounting artifacts, and live/run surfaces in the TUI, Flow dashboard, run
  report, and `rhei cost`. PR #44 §FS-rhei-cost-accounting.1
  §FS-rhei-cost-accounting.2 §FS-rhei-cost-accounting.4
- Add a prototype `github-issue-fix` template for routing one GitHub issue
  through worktree setup, repository-rule discovery, spec-fit analysis,
  validation, review, and optional PR publication.
- Make `github-issue-fix` route aggregate review blockers back through a
  deterministic repair loop before publication, with a bounded fix-attempt cap.
- Keep `github-issue-fix` repair cycles from reusing stale review artifacts by
  requiring per-visit validation, review, aggregate, and repair outputs.
- Make `github-issue-fix` use focused issue-specific validation by default and
  disclose expensive broad validation gaps in draft PRs instead of blocking
  publication by themselves.
- Make `github-issue-fix` review dispatch accept Markdown-bulleted readiness
  markers so `- Ready to publish: yes` routes to publication instead of being
  misread as a failed review.
- Make `github-issue-fix` block newly added internal grund citations in public
  user-facing docs unless the target repository explicitly requires them.
- Make `github-issue-fix` keep generated comments before annotation blocks so
  annotations remain directly attached to the declarations they annotate.
- Run program states in the same live `--parallel` worker pool as agent states,
  so a long-running program consumes one slot while other ready independent work
  continues to be scheduled. PR #43 §FS-rhei-run.5 §FS-rhei-programs.6.3
- Add the Flow-style interactive `rhei run` TUI surface with shared Flow, Machine,
  Cost, Journal, and Tasks views; cross-view filtering; task state filtering;
  custom terminal-state readiness; and human-gate liveness for both agent and
  callback runs. PR #42 §FS-rhei-run-tui.1.5 §FS-rhei-run-tui.1.5.2
- Write a durable per-run Markdown report at the end of every `rhei run` to
  `runtime/run-report.md` (latest) and `runtime/run-reports/<timestamp>-<run-id>.md`
  (history): header, outcome strip, attention list, transition ledger, source-order
  task final states, and spawned invocations with relative log links. The non-TTY
  path now prints a greppable `Report:` pointer, and a run that advanced tasks
  without spawning any agent or program is called out so reused-output advances are
  not mistaken for fast work. The report is also written for runs that abort with
  an error mid-execution; a `--dry-run` stays side-effect-free and writes nothing.
  PR #41 §FS-rhei-run-report.1 §FS-rhei-run-report.4
- Add task-level execution overrides with `**Model:**` and `**Target:**`,
  including validation, agent resolution precedence, transition artifact checks,
  and canonical example coverage. PR #40 §FS-rhei-plan-language.3.11
- Print a console-first end-of-run summary when `rhei run` exits on an
  interactive terminal: a result line, a state-distribution bar, run counts, an
  attention list of gated/blocked tasks, and a source-order task tree with
  per-task driver, duration, and final-state markers. Non-TTY output is
  unchanged so scripts and CI keep matching it. PR #39 §FS-rhei-run-report.3
- Detect when an agent-created commit leaves tracked Rhei-owned plan/result
  state uncommitted after `rhei run` applies its orchestrator transition, and
  report a clear error instead of silently reporting durable success. PR #38
- Run agents from checkout roots so repository `AGENTS.md` files and task
  worktrees are visible while Rhei artifacts stay rooted at the plan workspace.
  PR #35
- Fix `rhei run` auto-advance for nested agent tasks after required output
  artifacts are written. PR #33
- Clear stale Flow dashboard running indicators after the live loopback server
  stops answering, so closed runs do not leave browser tabs spinning forever.
  PR #31
- Simplify the built-in state machine to the manual `pending` -> `completed`
  flow, preserve durable manual claims from `rhei next`, and make `rhei run`
  refuse to auto-complete default manual tasks. PR #30
- Clarify the first-run example path, Panta's current read-only project support
  boundary, and runnable example discovery; fix `xtask` example copying for
  fixtures that contain snapshot symlinks. PR #28
- Fix stale template-author guidance, Flow inspector wording, and local Claude
  registration ignore handling after the settings-path and runtime-slot
  changes. PR #26
- Fix Flow running-now and running summary counts to use active runtime slots
  instead of persisted active-like task states. PR #23
- Fix Claude Code live intervention transport by using stream-json stdin with
  verbose print output when `intervene_stdin` is enabled. PR #25
- Move project settings from `.rhei/settings.json` to
  `.agents/rhei/settings.json`, including template instantiation output. PR #22
- Tighten `rhei-template-writer` skill guidance for editing existing templates
  and validating rendered `**Prior:**` metadata. PR #21
- Improve `rhei instantiate` template discovery help by listing templates when
  no template is provided and suggesting close matches for missing named
  templates. PR #20
- Remove the `rhei lsp` language-server product surface. PR #18
- Add product workflow templates and examples for agent discussion,
  analyze-and-dispatch, parallel worktrees, multi-model analysis, and spec
  review. PR #17
- Add live dashboard controls for explicit human-gate transitions. PR #16
- Add GitHub Actions CI, pre-commit hooks, and PGO release automation modeled on
  Grund's release flow. PR #15

## 1. [0.1.0] - 2026-05-21

- Initial alpha release line for the Rhei CLI, Rust crates, npm wrappers, and
  PyPI wrappers.

## 2. Older releases
