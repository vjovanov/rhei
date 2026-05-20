# RM-rhei-roadmap: Roadmap

This roadmap is sequenced against the project outcomes. §GOAL-rhei-outcomes

## Release Checklist for 0.1.0

This checklist is for the `0.1.0` release line.

### Preflight

Run from the repository root:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings -W clippy::all
cargo build --workspace --all-targets
cargo test --workspace --all-targets --no-fail-fast
cargo doc --workspace --no-deps
```

Confirm the CLI reports the intended release version:

```bash
cargo run -p rhei-cli -- version
```

### Source Release

1. Ensure the working tree contains only intentional release changes.
2. Confirm `CHANGELOG.md` has an entry for the release.
3. Commit the release preparation.
4. Tag the commit:

```bash
git tag -a v0.1.0 -m "Rhei 0.1.0"
git push origin v0.1.0
```

5. Create a GitHub release using the `CHANGELOG.md` entry as release notes.

### Crates.io Dry Runs

Before publishing anything, dry-run the crates that do not depend on other
unpublished Rhei crates:

```bash
cargo publish --dry-run -p rhei-plan-core
cargo publish --dry-run -p rhei-cli-tui
```

After `rhei-plan-core` is published, dry-run and publish the direct dependents:

```bash
cargo publish --dry-run -p rhei-agent-core
cargo publish --dry-run -p rhei-cli-output
cargo publish --dry-run -p rhei-cli-validator
cargo publish --dry-run -p rhei-api-napi
```

After `rhei-cli-output`, `rhei-cli-validator`, and `rhei-cli-tui` are
published, dry-run the CLI:

```bash
cargo publish --dry-run -p rhei-cli
```

### Crates.io Publish Order

Publish in the same dependency order:

```bash
cargo publish -p rhei-plan-core
cargo publish -p rhei-cli-tui

cargo publish -p rhei-agent-core
cargo publish -p rhei-cli-output
cargo publish -p rhei-cli-validator
cargo publish -p rhei-api-napi

cargo publish -p rhei-cli
```

The crates.io package names are conflict-free. The Rust import names remain
`rhei_core`, `rhei_agent_core`, `rhei_validator`, `rhei_output`, and
`rhei_tui`.

### npm Packages

The npm release packages live under `packages/npm/`:

```bash
cd packages/npm/rhei-cli
npm pack --dry-run
npm publish --access public

cd ../rhei-api
npm pack --dry-run
npm publish --access public
```

Publish the CLI package first. It is stored in `packages/npm/rhei-cli`, but its
npm package name is `rhei`. The `rhei-api` package depends on the matching
`rhei` npm version.

The npm packages are source-built wrappers: installing them requires Rust
and Cargo, then runs `cargo install rhei-cli --version 0.1.0`.

### PyPI Packages

The PyPI release packages live under `packages/python/`.

```bash
cd packages/python/rhei-cli
python3 -m build
python3 -m twine check dist/*
python3 -m twine upload dist/*

cd ../rhei-api
python3 -m build
python3 -m twine check dist/*
python3 -m twine upload dist/*
```

Publish `rhei-cli` first. The `rhei-api` package depends on
`rhei-cli==0.1.0`.

### Homebrew and GHCR

Do not block the alpha on Homebrew or GHCR. After a tagged GitHub release has
Linux/macOS artifacts, add a tap formula named `rhei` under a project-owned tap.
Add GHCR images only when there is a CI/service entrypoint worth containerizing.

## Completed: CLI Next No-Claim Diagnostics

Status: completed. `rhei next` now distinguishes completed plans, human-gated
tasks, claimed in-flight tasks, mid-workflow tasks that need an explicit
transition, and prerequisite-blocked tasks. Mid-workflow diagnostics include
copy-pasteable `rhei transition` commands for each outgoing transition, while
blocked-prerequisite diagnostics name the first unfinished prior and its state. §FS-rhei-next §FS-rhei-transition-cmd

## Completed: CLI Parse Error Accumulation

Status: completed. `rhei validate` now accumulates recoverable parse errors for
single-file plans and Directory Workspace task files so authors can fix a batch
of markdown mistakes without repeated parse/repair cycles. §FS-rhei-plan-language §FS-rhei-validate

## Planned: CLI UX and Release Polish

Status: planned. This section is the canonical home for useful follow-up work
from the April 2026 PM review and the product-management pre-release pass. The
old notes are historical; this roadmap owns the remaining backlog.

- Make failed `rhei complete` attempts from loop states explain the exact
  blocked transition condition and the currently available next transitions. §FS-rhei-complete §FS-rhei-transitions
- Decide and normalize `rhei transition` result-file behavior: either stop
  writing result files for bare transitions or link/audit them consistently
  with `rhei complete`. §FS-rhei-transition-cmd §FS-rhei-complete
- Improve template discovery and preflight output: list searched paths when no
  templates are found, surface reusable values-file scaffolds in template
  READMEs, and make nested `--list-inputs` defaults copyable. §FS-rhei-templates
- Resolve `type: path` input semantics: keep the current existence check for
  user-supplied paths, decide whether defaults should be checked, and decide
  whether an explicit `--allow-missing-paths` escape hatch belongs in the CLI. §FS-rhei-templates
- Extend JSON error output beyond the current `{ "error": { "message": ... } }`
  envelope with a stable `kind` and optional `path` taxonomy before downstream
  integrations depend on it. §FS-rhei-render §FS-rhei-next
- Clean up small human-output ambiguities: show agent and model as distinct
  fields, reword built-in validation source labels, clarify live template
  variables versus prose in state instructions, and decide whether rendered
  JSON should keep or flatten `metadata.metadata`. §FS-rhei-next §FS-rhei-validate §FS-rhei-states §FS-rhei-render

## Planned: Dashboard and Monitoring Follow-Ups

Status: planned. The first dashboard visualization pass is complete; these
items improve operator diagnosis without changing the execution model.

- Add richer readiness reasons in the dashboard for missing input artifacts and
  human gates. The current dashboard explains unfinished `Prior:` blockers but
  intentionally leaves non-prior causes generic. §FS-rhei-run-tui §FS-rhei-viz
- Add task-opening affordances, state/level filtering or dimming, a dependency
  graph view, and diff visualization against another snapshot or git ref. §FS-rhei-viz

## Planned: Snapshot Adapter and Retention Work

Status: planned. Snapshot v1 intentionally ships a conservative built-in
support boundary; Pi is supported, while other built-in agents require adapter
spikes before Rhei can safely capture and resume their native sessions.

- Resolve built-in adapter spikes for Claude Code, Codex, and Gemini session
  capture/resume surfaces, then update the built-in profile table and runtime
  support boundary. §FS-rhei-snapshot-operations §FS-rhei-snapshots
- Finalize provider cache TTL defaults in shipped settings and keep the
  snapshot specs pointing at that single source of truth. §FS-rhei-snapshot-operations
- Decide whether `snapshot.emit.on: timeout` should be distinct from
  `failure`, whether terminal-task automatic GC should replace TTL-based GC in
  v2, and whether sensitive states need a per-state auto-emit opt-out. §FS-rhei-snapshot-operations §FS-rhei-snapshots
- Add snapshot summarizer helpers, richer retention automation, and redaction
  audit support in a future manifest schema without turning snapshots into
  cross-agent transcript replay. §FS-rhei-snapshot-operations §FS-rhei-snapshots

## Completed: Post-Alpha Snapshot Continuation

Status: completed. Interactive `rhei snapshot continue` drops an operator into
a preloaded agent session and, unless `--no-capture` is passed, captures the
resulting transcript as an operator generation without advancing the snapshot
`current` pointer or mutating plan state. The built-in Pi profile provides the
v1 built-in interactive continuation surface; built-in agents without a proven
Rhei-readable session capture layout fail clearly with
`unsupported-snapshot-session` and can still be replaced by custom
session-capable profiles. §FS-rhei-snapshot-operations §FS-rhei-snapshots

## Completed: Post-Alpha Dashboard Visualization

Status: completed. The browser dashboard that accompanies `rhei run` includes
Gantt, heatmap cube, and Sankey plan views ahead of the operational Tasks,
Slots, Journal, and Links tabs. The dashboard remains the live execution
monitor for slots, task state, journal events, and links while also providing
static plan-shape views without switching tools. §FS-rhei-viz §FS-rhei-run-tui

The TUI surfaces the dashboard as a power-user view when `rhei run` selects the
TUI frontend, while `--dashboard` and `--no-dashboard` remain explicit
overrides in the CLI and completion surface. §FS-rhei-completions §FS-rhei-run
