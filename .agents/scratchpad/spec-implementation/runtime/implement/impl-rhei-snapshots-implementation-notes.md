# impl-rhei-snapshots implementation notes

Spec: `docs/functional-spec/rhei-snapshots.spec.md`

## Implemented

- §FS-rhei-snapshots.1 Goals / §FS-rhei-snapshots.2 Non-Goals
  - Implemented the authored-lineage grammar and validation without adding working-tree or cross-workspace snapshot behavior.
  - Files: `crates/rhei-validator/src/lib.rs`, `crates/rhei-cli/src/main.rs`.

- §FS-rhei-snapshots.3 Core Model / §FS-rhei-snapshots.3.1 Auto-Emitted vs Named Snapshots
  - Snapshot cache records, identities, current-generation selection, list/show/gc/continue inspection, and `_state` reference handling are implemented in the CLI snapshot command surface.
  - Files: `crates/rhei-cli/src/main.rs`.

- §FS-rhei-snapshots.4 State-Machine YAML Grammar
  - `snapshot.emit`, `snapshot.inherit`, and `snapshot.inherit.select` parse as closed objects.
  - Name, enum, boolean, integer selector, `select.target: all`, `select.target: same`, and polling/final/gating/program exclusions are validated.
  - Files: `crates/rhei-validator/src/lib.rs`.

- §FS-rhei-snapshots.4.3 Lineage Resolution / §FS-rhei-snapshots.4.6 Fallback Behavior
  - Static same-machine lineage validation rejects unresolvable and ambiguous authored `snapshot.inherit` references.
  - Fanout sources require `select.target`, and required inheritance rejects statically-known cross-agent sources.
  - Runtime cold-start fallback remains a guarded hook until adapter preload support is available.
  - Files: `crates/rhei-validator/src/lib.rs`, `crates/rhei-cli/src/main.rs`.

- §FS-rhei-snapshots.5 Compatibility Predicates / §FS-rhei-snapshots.9 Agent Transport Integration
  - Settings-aware validation checks that explicit named emit has a supported session layout and required inherit has a supported preload strategy.
  - Built-in unresolved agents remain unsupported for explicit snapshot operations unless a user profile supplies a `session` block.
  - Files: `crates/rhei-cli/src/main.rs`.

- §FS-rhei-snapshots.6 Sub-Task Inheritance
  - Snapshot records carry and display `parent_ref`; operator/cache handling preserves the manifest field.
  - Authored ancestor preload remains deferred with runtime adapter preload.
  - Files: `crates/rhei-cli/src/main.rs`.

- §FS-rhei-snapshots.7 Storage Layout / §FS-rhei-snapshots.7.1 Target Slug / §FS-rhei-snapshots.7.2 Atomic Writes
  - CLI cache readers and GC use the specified identity path shape.
  - Target slug normalization now preserves `.`, `_`, and `-`; fanout slug collisions are rejected.
  - Existing generation helpers allocate generation directories and update `current` with atomic symlink replacement.
  - Files: `crates/rhei-validator/src/lib.rs`, `crates/rhei-cli/src/main.rs`.

- §FS-rhei-snapshots.8 Manifest Schema
  - CLI reads manifest fields for listing/show/gc and validates `completion` against `produced_by`.
  - Files: `crates/rhei-cli/src/main.rs`.

- §FS-rhei-snapshots.10 Runtime Behavior
  - Run-loop call sites exist before spawn for inherit and after transition selection for emit.
  - `--from-snapshot` is rejected unless the target state declares `snapshot.inherit`.
  - Runtime adapter-specific preload/emit remains deferred below.
  - Files: `crates/rhei-cli/src/main.rs`.

- §FS-rhei-snapshots.11 Validation Rules
  - Added direct unit coverage for closed objects, enum validation, selector validation, unresolvable/ambiguous inheritance, fanout source targeting, target slug preservation/collision, settings-level session support, and required-preload validation.
  - Files: `crates/rhei-validator/src/lib.rs`, `crates/rhei-cli/src/main.rs`.

## Deferred

- Full auto-emitted and named snapshot writing from `rhei run` is deferred. The spec requires agent-native transcript discovery, copying, and manifest creation, but built-in Claude Code, Codex, and Gemini session layouts are explicitly unresolved until the adapter spike; implementing a fake transcript source would violate §FS-rhei-snapshots.9.2.

- Native preload/fork staging for authored `snapshot.inherit` is deferred with the same adapter dependency. The run-loop hook validates the authored contract and override gate now, but it does not append resume/fork strategy flags until a supported `AgentSessionProfile` shape is proven.

- `rhei snapshot continue` interactive transport is deferred. The command validates cache references, lock safety, timeout warning, and profile capability, then reports `unsupported-snapshot-session` for the unimplemented interactive transport.

- Ancestor-chain runtime resolution is deferred with preload. Static validation covers state-machine-local ambiguity and fanout requirements; actual parent task walk and successful-preload `parent_ref` assignment need the same runtime preload module.

## Verification

- `cargo test -p rhei-cli-validator --lib`
- `cargo test -p rhei-cli --bin rhei`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings -W clippy::all`
- `cargo build --workspace --all-targets`
- `cargo test --workspace --all-targets --no-fail-fast` was run after the targeted fixes. Snapshot-related unit and integration coverage passed, but two pre-existing/non-snapshot integration failures remained:
  - `e2e::next_tests::next_does_not_auto_transition_runnable_initial_states`: fixture uses `model: codex` without a `settings.models.codex` entry.
  - `e2e::run_tests::changeset_review_human_review_state_is_gating_in_shipped_workflows`: `examples/changeset-review-example/states.yaml` lacks the expected `human-review` state.

Recommended next state: `completeness-review`.
