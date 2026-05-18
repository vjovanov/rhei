# Completeness Fix Log: impl-rhei-snapshot-operations

Spec: `docs/functional-spec/rhei-snapshot-operations.spec.md`

## Fixed

- `snapshot list --orphaned` child-task orphan detection now recurses through the full task tree instead of checking only root tasks.
  - Files edited: `crates/rhei-cli/src/main.rs`
  - Test added: `snapshot_orphan_detection_recurses_into_child_tasks`

- `snapshot gc --orphaned` inherits the recursive child-task orphan detection fix because GC uses the same `is_snapshot_orphaned` helper.
  - Files edited: `crates/rhei-cli/src/main.rs`
  - Test coverage: `snapshot_orphan_detection_recurses_into_child_tasks`

- `snapshot gc --keep-generations <n> --older-than <duration>` now evaluates retention against all records selected by identity filters before applying the age filter to deletion candidates. Newer non-age-eligible records therefore count toward the kept generation budget.
  - Files edited: `crates/rhei-cli/src/main.rs`
  - Test added: `snapshot_gc_keep_generations_counts_newer_records_before_older_than`

- GC active-inherit protection now checks active child tasks as well as root tasks before allowing selected generations to be deleted.
  - Files edited: `crates/rhei-cli/src/main.rs`
  - Test added: `snapshot_active_inherit_protection_recurses_into_child_tasks`

- Phase 1 snapshot grammar validation now rejects unsupported `snapshot.inherit.compat` values, rejects `snapshot.inherit.select.target: all`, rejects unknown keys in `snapshot`, `snapshot.emit`, `snapshot.inherit`, and `snapshot.inherit.select`, and validates the core snapshot grammar fields (`name`, `emit.on`, `inherit.from`, selector visit/generation values, terminal/gating/program exclusions, and `required: true` with `compat: none`).
  - Files edited: `crates/rhei-validator/src/lib.rs`
  - Tests added: `rejects_snapshot_inherit_unsupported_compat`, `rejects_snapshot_inherit_select_target_all`, `rejects_snapshot_unknown_keys_in_closed_objects`

## Deferred

- `snapshot continue <ref>` interactive session spawn, `--no-capture`, operator transcript capture, operator manifest fields (`produced_by`, `parent_ref`, `completion`), atomic operator generation allocation, collision retry, current-pointer non-advance, and full lock hold across the interactive session are deferred. Concrete reason: built-in snapshot-capable `CustomAgentProfile.session` entries intentionally remain unresolved in the spec and code, and there is no implemented interactive continuation transport or native adapter contract to spawn while preserving TTY pass-through.

- `rhei run --from-snapshot` source override resolution, authored inherit constraint checking, native compatibility checks, ambiguity candidates, and `--override-inherit` bypass behavior are deferred. Concrete reason: the runtime snapshot preload module is still a stub (`preload_snapshot_inherit_before_spawn`) and no emitted snapshot source/staging/native compatibility implementation exists for it to call.

- Phase 1.5 redactor execution, phase 3 claude-code end-to-end snapshots, phase 4 pi snapshot/fork support, phase 5 codex resume support, phase 6 interactive transport completion, and state-exit auto-emission are deferred. Concrete reason: the runtime snapshot write path is still intentionally absent (`emit_snapshots_after_transition_selection` is a no-op stub), so there is no transcript staging window or agent session layout integration to attach these behaviors to.

- `snapshots.provider_cache_ttl` use in `cache_beneficial` is deferred. Concrete reason: `cache_beneficial` is evaluated during snapshot preload, and preload resolution/compatibility is not implemented yet.

- Redaction hook subprocess execution, minimal redactor environment forwarding, timeout/kill handling, stdout/stderr limits, run-log diagnostics, sha256-after-redaction opacity, and manifest non-recording behavior are deferred. Concrete reason: these requirements must run inside the atomic snapshot write window before manifest finalization, but the runtime snapshot writer is not implemented in this task state.

## Verification

- `cargo fmt --all`
- `cargo fmt --all -- --check`
- `cargo test -p rhei-cli --bin rhei snapshot`
- `cargo test -p rhei-cli-validator snapshot --lib`
- `cargo test -p rhei-cli-validator --lib`
- `cargo test -p rhei-cli --bin rhei`
- `cargo clippy -p rhei-cli -p rhei-cli-validator --all-targets -- -D warnings -W clippy::all`

Ready for `quality-review`.
