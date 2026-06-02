# RM-rhei-roadmap: Roadmap

This roadmap is sequenced against the project outcomes. §GOAL-rhei-outcomes

## Release Checklist

The release process is automated through GitHub Actions. The workflow verifies
the requested version, builds multi-platform PGO binaries, publishes crates.io
packages in dependency order when requested, and creates or updates the GitHub
release from `docs/changelog.md`. §FS-rhei-distribution §AR-ci-release

### Preflight

Run from the repository root before preparing a release:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings -W clippy::all
cargo build --workspace --all-targets --locked
cargo test --workspace --all-targets --locked --no-fail-fast
```

Confirm the CLI reports the intended release version:

```bash
cargo run -p rhei-cli -- version
```

Run the manual pre-release workflow to check registry names and build a Linux
PGO release binary:

```bash
gh workflow run pre-release-checks.yml
```

### Version Preparation

Use `scripts/set-release-version.py <version>` to keep the workspace version,
internal crate dependency requirements, npm package versions, and PyPI package
versions aligned. `scripts/prepare_changelog_release.py prepare <version>`
promotes `docs/changelog.md` `Unreleased` into the release section.

Patch and minor release helpers perform those steps automatically after a green
`CI` run on `main`, dry-run the release workflow from a candidate branch, then
fast-forward `main` and dispatch publishing.

### Publishing

Manual publishing is done from the `Release` workflow:

```bash
gh workflow run release.yml \
  -f version=0.1.0 \
  -f publish_crates=true \
  -f create_github_release=true
```

The workflow creates or reuses `vX.Y.Z`, builds release artifacts for Linux
GNU x86_64/aarch64, macOS x86_64/aarch64, and Windows x86_64/aarch64, publishes
the Rust crates, and uploads checksummed binaries to the GitHub release.

### Package Wrappers

The npm and PyPI package wrappers remain source-built wrappers around the
matching `rhei-cli` crate version. Their checked-in version metadata is kept in
sync by `scripts/set-release-version.py`; publishing those wrapper packages
should happen only after the matching `rhei-cli` crate version is available.

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

- Graduate Panta from read-only project loading to project-wide mutation:
  route state, assignee, result, runtime, and artifact rewrites to each owning
  rhei; resolve per-rhei state machines during execution; and replace the
  current mutating-command rejection with scoped project execution. §FS-rhei-panta §AR-rhei-panta
- Validate child-rhei content-section links under Panta: carry a per-section
  link base so a rhei's own content sections resolve against that rhei's
  execution root, not the project root. Today only task-content links are
  checked per rhei; rhei-level content sections are dropped at merge. §FS-rhei-plan-language.3.6 §AR-rhei-panta.5
- Treat a bare rhei as the single rhei of an implicit Panta so every load path
  yields a Panta-rooted graph, matching the target load model. §AR-rhei-panta
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

## Planned: Relocatable Rhei Root and Cross-Worktree Coordination

Status: planned. Today the coordination signal — a task's claim, `**State:**`,
and the transition that rewrites it — lives inside the working-tree task file,
so parallel agents in separate git worktrees can only observe each other's
progress by committing the rhei. This blocks the common multi-agent setup where
users want live coordination but do not want plan state in git history. The work
below decouples the rhei root from the working tree so worktrees on one machine
share live state through the filesystem rather than through commits. §DA-panta-root §AR-rhei-panta §GOAL-rhei-outcomes

- Make the rhei root relocatable with an explicit resolution order: `--rhei-root`
  flag, then `RHEI_ROOT` environment variable, then the shared git common
  directory (`$(git rev-parse --git-common-dir)/rhei/`) when inside a repository,
  then the in-tree default. The common-dir default lets every linked worktree of
  one repository read and write the same runtime state without that state ever
  entering a commit. §AR-rhei-panta §FS-rhei-panta
- Split the authored plan from runtime state: keep task bodies, `**Prior:**`, and
  descriptions as the versioned "what," and move claims, current state, and the
  event log into a side store keyed by node id under the relocatable root. This
  lets users commit the plan while keeping live status uncommitted, instead of
  forcing both into the same file. §FS-rhei-plan-language §AR-rhei-panta
- Confirm the concurrency primitive on a shared store: Directory Workspace
  per-task sharding plus `rhei transition` CAS already make parallel claims safe
  on a shared local filesystem; document that contract and add a lock check where
  filesystem atomicity weakens, such as NFS. §FS-rhei-plan-language §DA-panta-root
- Specify the single-machine versus cross-machine boundary explicitly:
  same-machine worktrees coordinate through the shared filesystem with no
  commits, while coordination across machines without committing requires an
  out-of-band transport — a shared filesystem or a small sync server — the same
  constraint every on-disk tracker faces. §DA-panta-root §GOAL-rhei-outcomes

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
