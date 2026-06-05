# Changelog

## Unreleased

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
