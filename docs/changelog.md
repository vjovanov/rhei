# Changelog

## Unreleased

- Make the shell argument to `rhei completions` optional: detect it from
  `$SHELL` when omitted, and on detection failure list the supported shells
  with a copy-pasteable example instead of clap's bare missing-argument
  error. PR #58 §FS-rhei-completions.2

**Breaking: ticket ids are now project-qualified.** Every load yields a
Panta-rooted graph, so a ticket that used to be `1` is now `auth.1` — named
for the rhei it lives in. This changes ids in command output, result artifact
filenames (`runtime/results/auth.1.md`), ledgers, and logs. Plan files are not
rewritten: task headings stay rhei-local, and a plan completed before this
change keeps its rhei-local result links and artifacts, which keep validating.
A single-file rhei must now be named `<id>.rhei.md`, since the file stem is
where its id comes from. PR #45

- Make Panta the default execution model: a bare rhei — a `.rhei.md` file or a
  Directory Workspace — loads as the single rhei of an implicit Panta, so there
  is one loader and one graph shape whether or not an `index.panta.md` exists.
  §AR-rhei-panta.2 §AR-rhei-panta.3
- Mutate project-wide. `rhei run`, `next`, `transition`, `complete`, and `reset`
  operate across the project, routing every state, assignee, result, and runtime
  rewrite back to the owning rhei file. The previous staged boundary — which
  rejected mutating commands on a project — is gone. §FS-rhei-panta.6
- Accept rhei-local shorthand for CLI ticket targets: `rhei complete 1` resolves
  when exactly one in-scope rhei has that ticket, and names the qualified
  candidates when more than one does. §FS-rhei-panta.6
- Add `--rhei <id>` (repeatable) to narrow `run`, `next`, `reset`, and `list` to
  named rheis. It selects candidates without narrowing where their priors
  resolve, so a candidate may still be blocked by a prior outside the scope —
  and the no-work diagnostic now names that prior as out of scope instead of
  reporting out-of-scope work. `run` and `reset` report their resolved scope
  before acting; a one-rhei project has no fan-out to report and stays quiet.
  §FS-rhei-panta.6 §FS-rhei-panta.6.1 §FS-rhei-panta.6.4 §FS-rhei-run.2.5
  §FS-rhei-next.2.2 §FS-rhei-list.2 §FS-rhei-reset.1.1
- Scope a narrowed `rhei reset` to everything keyed by an in-scope ticket —
  result file, logs, declared artifact-contract paths, snapshot sessions,
  worktree refs, accounting captures, and its lines in the transition ledger —
  instead of results and logs alone, and report the run-scoped output it
  deliberately keeps. A stale declared output could otherwise satisfy a
  required input on the next run. §FS-rhei-reset.2.1 §FS-rhei-panta.6.4
- Validate result links as a pair: link text and target must describe the same
  ticket, both qualified or both rhei-local. §FS-rhei-panta.6.3
  §FS-rhei-plan-language.3.8
- Warn when `rhei viz` is pointed at a Panta project: it is not Panta-aware, so
  the page is one disconnected plan per `*.rhei.md`, not the merged project
  graph. §FS-rhei-viz.7.3
- Unify subprocess ids: `RHEI_TASK_ID` is the project-qualified ticket id for
  agents, programs, *and* transition callbacks (callbacks previously received
  the rhei-local id). `RHEI_TASK_ID_LOCAL` and the `{task_id_local}` /
  `{rhei_id}` template variables carry the rhei-local form for scripts and
  instructions that edit or grep the plan file, and the callback context JSON
  gains `task.localId`. §FS-rhei-panta.6 §AR-rhei-panta.3

  Two limits ship with this change, both tracked on the roadmap: one state
  machine still governs a whole project — a rhei declaring a machine different
  from the project default is a load error — and `rhei viz` is not yet
  Panta-aware.

  **Upgrading a pre-qualification workspace.** Nothing is rewritten for you;
  these are the sharp edges and what to do about them:

  - *Plan filenames.* A single-file rhei must be `<id>.rhei.md`, where `<id>`
    starts with a letter and uses only letters, digits, `_`, or `-`. Rename
    files like `My Plan.rhei.md` or `2026-roadmap.rhei.md`; the load error
    suggests a legal name. The same rule applies to Directory Workspace
    directory names.
  - *Scripts and JSON consumers.* Command output, `rhei next` JSON
    (`task_id`), `rhei list --json` (`id`, `prior`, `parent`), `{task_id}`,
    and `RHEI_TASK_ID` all carry qualified ids now. `rhei list --json`'s
    `depth` counts within the owning rhei (a top-level ticket is `1`).
    Scripts that match on heading ids should switch to `RHEI_TASK_ID_LOCAL`.
  - *Mid-flight runtime artifacts.* Artifacts produced under rhei-local names
    (`runtime/results/1.md`, `runtime/worktree-refs/1.yaml`, ledger lines
    `1 pending@…`, `runtime/accounting/tasks/1.json`) are not read, cleaned,
    or migrated — a narrowed reset only matches qualified keys. When a
    required input exists only under its pre-qualification name, the
    missing-artifact error names that file and the rename that fixes it.
    Finish or reset in-flight tickets before upgrading if you want a clean
    ledger; a full `rhei reset` still clears the whole `runtime/` tree.
  - *Snapshot caches.* `.rhei/cache/snapshots/` is keyed by ticket id, so
    caches produced before this change no longer resolve: `rhei snapshot
    list --orphaned` shows them and `rhei snapshot gc` prunes them.
  - *Completed plans.* Legacy rhei-local result links keep validating and are
    left alone; only re-completing a ticket refreshes its link to the
    qualified form. §FS-rhei-panta.6.3
- Add durable task state history to Flow/dashboard and the `rhei run` TUI,
  including the `state history` surroundings section, prompt-focused inspector
  navigation, a global Machine legend with process-kind styling, and links-only
  shared chrome. PR #48 §FS-rhei-viz.4 §FS-rhei-run-tui.1.5
- Add Codex token accounting from `turn.completed` JSON usage, persisted runtime
  accounting artifacts, and live/run surfaces in the TUI, Flow dashboard, run
  report, and `rhei cost`. PR #44 §FS-rhei-cost-accounting.1
  §FS-rhei-cost-accounting.2 §FS-rhei-cost-accounting.4
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
