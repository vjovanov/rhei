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

cargo publish -p rhei-cli-output
cargo publish -p rhei-cli-validator
cargo publish -p rhei-api-napi

cargo publish -p rhei-cli
```

The crates.io package names are conflict-free. The Rust import names remain
`rhei_core`, `rhei_validator`, `rhei_output`, and `rhei_tui`.

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
