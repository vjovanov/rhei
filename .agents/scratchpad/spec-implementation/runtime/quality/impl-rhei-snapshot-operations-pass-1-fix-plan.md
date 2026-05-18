# Quality Fix Plan pass 1

## Accepted

- Q-from-snapshot-contract: `rhei run --from-snapshot` resolves and preloads snapshots, but the override still does not fully enforce the target state's authored `snapshot.inherit` contract.
  - Files: `crates/rhei-cli/src/main_parts/snapshot_runtime_2.rs`, `crates/rhei-cli/src/main_parts/tests_7.rs`
  - Approach: Keep the existing resolver/preload path, but when `--from-snapshot` is present and `--override-inherit` is absent, validate the resolved `SnapshotRecord` against `inherit.name`, `inherit.from`, `inherit.select.state`, `inherit.select.target`, `inherit.select.visit`, `inherit.select.generation`, and `compat`. Reject `compat: none` for override runs unless `--override-inherit` is set instead of silently running cold. Preserve the existing missing-`snapshot.inherit` rejection even with `--override-inherit`.
  - Tests/checks: Add focused unit tests in `crates/rhei-cli/src/main_parts/tests_7.rs` for name/state/target/generation mismatch rejection, `compat: none` rejection without override, and bypass with `--override-inherit`; run `cargo test -p rhei-cli --bin rhei snapshot_preload`.

- Q-redactor-unused: `snapshots.redactor` is parsed, but snapshot writes read and hash the transcript before any redaction hook can run.
  - Files: `crates/rhei-cli/src/main_parts/settings_1.rs`, `crates/rhei-cli/src/main_parts/snapshots_1.rs`, `crates/rhei-cli/src/main_parts/snapshot_runtime_1.rs`, `crates/rhei-cli/src/main_parts/tests_7.rs`
  - Approach: Thread the merged snapshot settings and workspace root into `write_snapshot_generation_atomic`; before computing `transcript_sha256` or writing `transcript.*`, run the configured redactor with cwd set to the workspace root, stdin set to the staged transcript bytes, a closed environment plus explicit `redactor_env` allow-list entries, and a finite timeout/kill path. Use stdout as the replacement transcript. Abort the snapshot write on nonzero exit or timeout and include stderr summary in the error; do not add redactor metadata to `manifest.json`.
  - Tests/checks: Add tests proving a configured redactor changes cached transcript bytes and sha256, and a failing redactor aborts without leaving a generation; run `cargo test -p rhei-cli --bin rhei snapshot`.

- Q-current-fallback: Omitted generation references fall back to the newest generation when an identity has no valid `current` pointer.
  - Files: `crates/rhei-cli/src/main_parts/snapshots_3.rs`, `crates/rhei-cli/src/main_parts/tests_6.rs`
  - Approach: Change omitted-generation resolution to require `record.is_current` for every matched identity. If an identity has generations but no current record, return a clear cache-integrity/ambiguous-reference error that tells the operator to retry with `/g<N>` or repair the `current` pointer; do not select newest as a command-specific tie-breaker.
  - Tests/checks: Add a resolver test that creates multiple generations without `current` and asserts `resolve_snapshot_ref(..., None, None)` errors while `/g<N>` still resolves; run `cargo test -p rhei-cli --bin rhei snapshot_ref_parser`.

## Rejected / Deferred

- Q-continue-deferred: defer
  - Reason: The finding is valid for the full phase-6 surface, but fixing it requires implementing the interactive continuation transport, TTY pass-through, run-lock lifetime management for the whole operator session, and operator transcript capture. That is broader than a pass-1 quality fix and depends on adapter/session-profile work outside this bounded patch.
