# impl-rhei-snapshot-operations Implementation Notes

Spec: `docs/functional-spec/rhei-snapshot-operations.spec.md`

## §FS-rhei-snapshot-operations 1. CLI Surface

- Implemented `rhei snapshot list`, `show`, `gc`, and `continue` command shapes in `crates/rhei-cli/src/main.rs`.
- Added shared snapshot cache loading, manifest readers, listing JSON/text output, and a shared reference resolver in `crates/rhei-cli/src/main.rs`.
- Added focused unit coverage in `crates/rhei-cli/src/main.rs` for command parsing, named-vs-auto shorthand precedence, ambiguous references, and GC retention behavior.

## §FS-rhei-snapshot-operations 1.1 Snapshot Reference Parser

- Implemented shared parsing for path refs and shorthand refs:
  `<task>:<name>[:<state>][@<visit>][:<target>][/g<N>]` and `<task>:<state>` auto-emit shorthand.
- Named snapshot matches win over auto shorthand; explicit `_state` remains available.
- Ambiguous refs return an error listing candidate refs and the retry guidance required by the spec.
- Implementation: `parse_snapshot_ref`, `resolve_snapshot_ref`, `ambiguous_snapshot_report` in `crates/rhei-cli/src/main.rs`.

## §FS-rhei-snapshot-operations 1.2 `rhei snapshot list`

- Implemented `--task`, `--name`, `--state`, `--produced-by orchestrator|operator|all`, `--orphaned`, and `--format text|json`.
- Default `produced_by` is `orchestrator`.
- Text output includes the required default columns.
- Orphan filtering compares manifest task/state/target slug against the current plan, state machine, and resolved target set.
- Implementation: `SnapshotCommand::List`, `print_snapshot_list`, `is_snapshot_orphaned`.

## §FS-rhei-snapshot-operations 1.3 `rhei snapshot show <ref>`

- Implemented full manifest rendering and transcript head/tail preview.
- Uses the shared reference resolver and ambiguity behavior.
- Implementation: `SnapshotCommand::Show`, `print_snapshot_show`.

## §FS-rhei-snapshot-operations 1.4 `rhei snapshot gc`

- Implemented `--task`, `--name`, `--older-than`, `--keep-generations`, `--include-operator`, `--orphaned`, `--dry-run`, and `--force`.
- `--keep-generations` retains newest generations per `(task_id, snapshot_name, emitting_state, visit, target_slug)` identity.
- Operator generations are ignored unless `--include-operator` is set.
- Implemented `.rhei/run.lock` interlock inspection and active `snapshot.inherit.select.generation` best-effort protection unless `--force` is passed.
- Implementation: `snapshot_gc_command`, `generations_beyond_keep`, `run_lock_is_held`, `snapshot_generation_protected_by_active_inherit`.

## §FS-rhei-snapshot-operations 1.5 `rhei snapshot continue <ref>`

- Implemented CLI parsing, reference resolution, run-lock refusal, timeout warning, and `unsupported-snapshot-session` pre-spawn checks for missing resume/layout/interactive profile.
- Deferred: actual interactive TTY spawn and operator generation capture. Reason: the current runtime snapshot emission/preload layer is still a no-op stub, built-in profiles do not yet expose proven interactive continuation profiles, and the spec assigns this surface to phase 6.
- Implementation/stub: `snapshot_continue_command`.

## §FS-rhei-snapshot-operations 2. Run Override

- `rhei run --from-snapshot`, `--override-inherit`, `--task`, and `--target` flags were already present in this scratchpad branch; this task preserved them and wired the authored-contract guard.
- Implemented rejection when `--from-snapshot` is used on a target state without `snapshot.inherit`, including when `--override-inherit` is passed.
- Deferred: actual preload override selection/compatibility enforcement. Reason: runtime snapshot preload is still represented by the existing `preload_snapshot_inherit_before_spawn` stub pending the lower-level snapshot implementation.
- Implementation: `SnapshotExecutionFlags`, `RunOptions::snapshot_override_ref`, `preload_snapshot_inherit_before_spawn`.

## §FS-rhei-snapshot-operations 3. Phased Rollout

- Implemented the inspection/maintenance foundation suitable for phase 1.5: settings parse/merge, manifest readers, `list`, `show`, and `gc`.
- Deferred phase 2+ adapter spikes and phase 3+ runtime snapshot writes because this task did not have proven per-agent native session adapters.

## §FS-rhei-snapshot-operations 4. Configuration

- Implemented typed parsing/merge for the top-level `snapshots` settings block, including `cache_dir`, `experimental`, `provider_cache_ttl`, `redactor`, and `redactor_env`.
- Implemented cache directory resolution with default `.rhei/cache/snapshots`.
- Deferred redactor execution. Reason: redaction runs inside the atomic snapshot write window, but snapshot writes are still deferred/stubbed in this branch.
- Implementation: `SnapshotSettings`, `merge_snapshot_settings`, `snapshot_cache_dir`.

## §FS-rhei-snapshot-operations 5. Open Questions

- No code changes required. The unresolved agent-adapter questions remain deferred to the rollout phases named in the spec.

## Supporting Validator Shape

- Added a minimal typed `StateDef.snapshot` shape in `crates/rhei-validator/src/lib.rs` so the CLI can inspect `snapshot.inherit` for run override and GC interlock behavior.
- Deferred full static validation of all snapshot grammar rules to the snapshot grammar/runtime task; this operations task only needs the parsed shape for its surfaces.

## Verification

- `cargo test -p rhei-cli snapshot_ -- --nocapture`
- `cargo clippy -p rhei-cli --all-targets -- -D warnings -W clippy::all`
- `cargo fmt --all -- --check`
- `cargo build --workspace --all-targets`
- `cargo test --workspace --all-targets --no-fail-fast` was run and found one unrelated existing fixture failure: `e2e::run_tests::changeset_review_human_review_state_is_gating_in_shipped_workflows` reports that `examples/changeset-review-example/states.yaml` is missing a `human-review` state. Snapshot-focused tests passed.
