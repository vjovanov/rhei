# impl-rhei-snapshots completeness fix log

Spec: `docs/functional-spec/rhei-snapshots.spec.md`

## Files edited

- `.gitignore`
- `Cargo.lock`
- `crates/rhei-cli/Cargo.toml`
- `crates/rhei-cli/src/main.rs`

## Tests added or updated

- Added `tests::snapshot_emit_writes_auto_and_named_generations`.
- Added `tests::snapshot_preload_resolves_self_current_generation`.

## Gaps closed

- `CONTEXT-001`, `CONTEXT-002`, `CONTEXT-003`: wired runtime snapshot capture into agent exit handling, using the shared cache root and manifest/storage shape for auto and named snapshots.
- `MODEL-001`, `MODEL-002`, `MODEL-003`: snapshot writes now allocate immutable `(task_id, snapshot_name, emitting_state, visit, target_slug, generation)` identities and update `current` without overwriting prior generations.
- `AUTO-001`, `AUTO-003`, `AUTO-004`: auto `_state` snapshots are emitted for supported agent-bearing state exits; named emits follow `snapshot.emit`; unsupported auto sessions skip while unsupported named emits error.
- `YAML-001`, `YAML-003`, `YAML-004`, `YAML-005`, `YAML-006`, `YAML-007`, `YAML-008`, `YAML-009`, `YAML-010`, `YAML-014`, `YAML-016`: runtime emit/preload now applies emit policies, default inherit fields, `compat`, `required`, state/target/visit/generation selectors, and `same` target resolution.
- `LINEAGE-001`, `LINEAGE-002`, `LINEAGE-003`, `LINEAGE-004`: authored inherit now resolves self history, prior visits, nearest ancestor task-id prefixes, latest visits, current/latest generations, and ambiguity/missing fallbacks.
- `TIMING-001`, `FALLBACK-001`: preload runs before spawn and emit runs after agent completion before transition advancement; optional inherit has a single cold-start fallback.
- `COMPAT-001`, `COMPAT-002`, `COMPAT-003`, `COMPAT-004`, `COMPAT-005`, `COMPAT-006`: native compatibility checks agent identity and layout fields, ignores mode, rejects authored timeout preloads, and logs cache-benefit provider/model mismatches.
- `SUBTASK-001`, `SUBTASK-002`, `SUBTASK-004`: successful preloads produce `parent_ref`; ancestor inheritance stays within the workspace cache and creates independent child lineage on the next emit.
- `STORAGE-001`, `STORAGE-002`, `STORAGE-003`, `STORAGE-004`, `SLUG-003`: runtime writes the specified directory layout, canonical identity path segments, relative `current`, raw target selector, and adds `.rhei/cache/` to `.gitignore`.
- `ATOMIC-001`, `ATOMIC-002`, `ATOMIC-003`, `ATOMIC-004`: snapshot writes take an identity lock, stage under `g<N>.tmp-*`, compute SHA-256, rename finalized generations, update `current`, ignore stale temp dirs during allocation, and retry generation allocation on existing destinations.
- `MANIFEST-001`, `MANIFEST-002`, `MANIFEST-003`, `MANIFEST-005`: writer emits full v1 manifests; reader validates required fields, completion/producer combinations, parent shape, and path identity consistency.
- `AGENT-002`, `AGENT-003`, `AGENT-004`, `AGENT-005`, `AGENT-006`, `AGENT-007`: runtime honors session layout/resume capability split, appends `session_dir_flag`, adds the Pi built-in session profile, and preserves unsupported built-in skip/fail behavior.
- `PI-001`: Pi now has built-in native resume/fork/session-dir metadata and participates in the generic runtime snapshot flow.
- `GEMINI-001`, `CLAUDE-001`, `CODEX-001`: unsupported built-ins remain cold/skip/fail according to optional/required and explicit/auto rules.
- `RUN-001`, `RUN-002`, `RUN-003`, `RUN-004`, `RUN-005`, `RUN-006`, `RUN-007`, `RUN-008`: run-time preload and emit now resolve lineage, handle missing/disabled/timeout/incompatible/unsupported cases, classify completion, write auto/named snapshots, and suppress non-agent states.
- `LOOP-001`, `LOOP-002`, `LOOP-003`: visit and target-specific identity fields are used for counted/fanout emissions and per-target preload resolution; poll self-loop suppression remains enforced by the existing call-site structure.
- `VAL-007`, `VAL-014`, `VAL-017`, `VAL-019`, `VAL-021`, `VAL-022`: validation/runtime checks now cover root ancestor inherit on current root tasks, runtime unsupported emit, effective target requirement, timeout selected generations, workspace cache scoping, and orphan warnings.

## Deferred

- `SUBTASK-003`: operator generations from `rhei snapshot continue` remain deferred because the interactive continuation transport is still explicitly unsupported by the current CLI path.
- `AGENT-001`: replacing `CustomAgentProfile.session: serde_json::Value` with a typed validator schema is deferred to avoid a broad settings compatibility migration in this completeness fix.
- `VAL-012`: static nearest-ancestor ambiguity across future task hierarchy states is deferred because the state-machine validator has no full plan hierarchy execution graph; runtime nearest-ancestor ambiguity is handled.
- `VAL-013`: optional cross-agent mismatch warning is deferred because the current validator warning channel is state-machine-local and settings-aware warnings are not yet threaded through every command path.

## Verification

- `cargo fmt --all -- --check`
- `cargo check -p rhei-cli`
- `cargo clippy -p rhei-cli --all-targets -- -D warnings -W clippy::all`
- `cargo test -p rhei-cli snapshot_ --no-fail-fast`
- Attempted full CI chain `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings -W clippy::all && cargo build --workspace --all-targets && cargo test --workspace --all-targets --no-fail-fast`; fmt, clippy, and build passed, but workspace tests failed in pre-existing/non-snapshot e2e cases:
  - `e2e::run_tests::changeset_review_human_review_state_is_gating_in_shipped_workflows` (`examples/changeset-review-example/states.yaml` missing `human-review`).
  - `e2e::next_tests::next_does_not_auto_transition_runnable_initial_states` (fixture resolves model `codex` without a `settings.models.codex` entry).

Next state: `quality-review`.
