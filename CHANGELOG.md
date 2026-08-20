# Changelog

Released versions of Rhei. Changes that have not shipped in a release yet live
in the `Unreleased` section of [docs/changelog.md](docs/changelog.md), which
release automation promotes into a numbered section here at release time
(§FS-rhei-distribution.5). Read both to see everything on `main`.

## Unreleased

See [docs/changelog.md](docs/changelog.md#unreleased). It currently carries a
**breaking change**: ticket ids are project-qualified (`1` → `auth.1`), which
changes command output, result artifact filenames, ledgers, logs, snapshot
cache keys, and the `RHEI_TASK_ID` seen by callbacks, and requires a
single-file rhei to be named `<id>.rhei.md`. The entry ends with an
"Upgrading a pre-qualification workspace" checklist — read it before upgrading
a workspace with in-flight runs.

## 0.1.0 - 2026-05-19

Initial release.

### Added

- Markdown plan parsing for single-file plans and directory workspaces.
- Semantic validation for task state, dependency, hierarchy, terminal-tree, link, and artifact-contract rules.
- CLI commands for validation, rendering, ready-work selection, task completion, explicit transitions, reset, state-machine inspection, template instantiation, skill installation, shell completions, and version reporting.
- YAML state-machine support for transitions, callbacks, program states, agent/tooling profiles, counted review loops, and human gates.
- Terminal and journal support for monitoring parallel `rhei run` execution.
- Renderers for JSON, GitHub-style Markdown, and terminal progress output.
- Example workspaces for release automation, review loops, changeset review, human-intervention workflows, CI healing, and spec/implementation audits. (Templates were project- and user-scoped in 0.1.0; a built-in library shipped inside the binary landed after it — see the unreleased notes.)
- Rust library crates for core parsing, validation, output rendering, TUI events, and N-API bindings.

### Known Release Limitation

- crates.io publication uses conflict-free package names, because `rhei` and `rhei-core` both belong to an unrelated project on crates.io: `rhei-cli` for the command and `rhei-plan` for the Rust plan-model API. The installed binaries are `rhei` and `rh`, and Rust import names remain stable through explicit library names and dependency aliases.
