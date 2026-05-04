# Changelog

All notable user-facing changes to Rhei are tracked here.

## 0.1.0-alpha.1 - 2026-05-05

Initial alpha release candidate.

### Added

- Markdown plan parsing for single-file plans and directory workspaces.
- Semantic validation for task state, dependency, hierarchy, terminal-tree, link, and artifact-contract rules.
- CLI commands for validation, rendering, ready-work selection, task completion, explicit transitions, reset, state-machine inspection, template instantiation, skill installation, shell completions, and version reporting.
- YAML state-machine support for transitions, callbacks, program states, agent/tooling profiles, counted review loops, and human gates.
- Terminal and journal support for monitoring parallel `rhei run` execution.
- Renderers for JSON, GitHub-style Markdown, and terminal progress output.
- Built-in templates and examples for release automation, review loops, changeset review, human-intervention workflows, CI healing, and spec/implementation audits.
- Rust library crates for core parsing, validation, output rendering, TUI events, and N-API bindings.

### Known Release Limitation

- crates.io publication uses conflict-free package names: `rhei-cli` for the command, `rhei-api` for the Rust parser API, and `rhei-cli-*` for support crates. The installed binary remains `rhei`, and Rust import names remain stable through explicit library names and dependency aliases.
