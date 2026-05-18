# Quality Fix Log pass 1

## Accepted Fixes Applied

- Q-from-snapshot-contract
  - Files edited:
    - `crates/rhei-cli/src/main_parts/snapshot_runtime_2.rs`
    - `crates/rhei-cli/src/main_parts/tests_7.rs`
  - Summary: `rhei run --from-snapshot` now rejects `compat: none` unless `--override-inherit` is set, and validates non-bypassed override records against the authored inherit name, `from`, select state/target/visit/generation, default current generation, and native compatibility contract. Added focused preload tests for name/state/target/generation mismatches, `compat: none`, and `--override-inherit` bypass.

- Q-redactor-unused
  - Files edited:
    - `crates/rhei-cli/src/main_parts/settings_1.rs`
    - `crates/rhei-cli/src/main_parts/snapshots_1.rs`
    - `crates/rhei-cli/src/main_parts/snapshot_runtime_1.rs`
    - `crates/rhei-cli/src/main_parts/tests_7.rs`
  - Summary: snapshot emission now passes merged settings and workspace root into the atomic generation writer, runs configured redactors before transcript hashing/writing with closed environment plus `redactor_env` allow-list, captures stdout as replacement transcript bytes, and aborts writes on nonzero exit or timeout without adding manifest redactor metadata. Added tests for redacted bytes/hash and failing-redactor abort.

- Q-current-fallback
  - Files edited:
    - `crates/rhei-cli/src/main_parts/snapshots_3.rs`
    - `crates/rhei-cli/src/main_parts/tests_6.rs`
  - Summary: omitted-generation snapshot refs now require a valid `current` pointer for each matched identity and return a cache-integrity error instructing operators to retry with `/g<N>` or repair `current` when no pointer exists. Existing parser tests were updated to create current pointers where current resolution is expected, and a focused missing-current resolver test was added.

## Checks Run

- `cargo fmt --all`
  - Result: passed.
- `cargo fmt --all -- --check`
  - Result: passed.
- `cargo build -p rhei-cli --bin rhei`
  - Result: passed.
- `cargo test -p rhei-cli --bin rhei snapshot_preload`
  - Result: blocked before snapshot tests compiled by an existing delimiter error in `crates/rhei-cli/src/main_parts/tests_1.rs:493-499`.
- `cargo test -p rhei-cli --bin rhei snapshot_ref_parser`
  - Result: blocked by the same existing `tests_1.rs` delimiter error.
- `cargo test -p rhei-cli --bin rhei snapshot`
  - Result: blocked by the same existing `tests_1.rs` delimiter error.

## Intentionally Left Unfixed

- Q-continue-deferred remains deferred as specified in the fix plan.
- The unrelated `tests_1.rs` delimiter error was not fixed because it is outside the accepted pass-1 quality-fix scope.

## Next State

- `visit_count` for `quality-fix` is 1 and configured visits are 2, so this task should transition back to `quality-review` after this invocation.
