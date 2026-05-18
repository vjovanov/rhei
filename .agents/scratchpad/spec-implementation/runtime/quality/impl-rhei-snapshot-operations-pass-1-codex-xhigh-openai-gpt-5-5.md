# Quality Review pass 1 - target codex[xhigh]:openai:gpt-5.5

- Q-continue-deferred: `rhei snapshot continue` is exposed but cannot ever continue from a supported snapshot.
  - Severity: high
  - File: crates/rhei-cli/src/main.rs
  - Detail: The command resolves the snapshot and checks the profile, but then unconditionally returns `unsupported-snapshot-session: interactive snapshot continuation transport is deferred until phase 6`. It never acquires and holds the run lock, never spawns the interactive continuation profile, never honors `--no-capture`, and never writes the operator sibling generation with `produced_by: operator` and `parent_ref`. This violates the implemented CLI surface for a user-visible command.
  - Evidence: Spec lines 103-175 require `rhei snapshot continue <ref>` to start an interactive preloaded agent session, hold the same `.rhei/run.lock`, optionally capture the resulting transcript as an operator generation, and leave `current` unchanged. The implementation at `crates/rhei-cli/src/main.rs:9736-9775` only checks whether the lock is currently held, resolves the ref, warns on timeout, checks profile shape, discards `no_capture`, and always returns the deferred error.
  - Suggested fix: Implement the phase-6 continuation path behind the existing command: acquire `HeldRunLock` for the whole session, construct the interactive transport from `InteractiveContinuationProfile`, preload/resume the source transcript, and write the operator generation through the same atomic snapshot writer unless `--no-capture` is set.

- Q-from-snapshot-noop: `rhei run --from-snapshot` does not resolve or enforce the override contract.
  - Severity: high
  - File: crates/rhei-cli/src/main.rs
  - Detail: The run override hook only checks that the target state declares `snapshot.inherit`; it ignores the supplied reference, `--task`, `--target`, `--override-inherit`, the authored name/from/state/visit/generation/target constraints, and native compatibility. The agent then runs without any snapshot preload, so an accepted `--from-snapshot` invocation silently behaves like a cold run.
  - Evidence: Spec lines 177-197 require `--from-snapshot` to override the concrete inherited source only after authored constraints are applied, reject incompatible or `compat: none` sources unless `--override-inherit` is present, and reject ambiguous override contexts with candidates. The implementation at `crates/rhei-cli/src/main.rs:12442-12473` validates only missing `snapshot.inherit`, then assigns all override inputs to `_` and returns `Ok(())`.
  - Suggested fix: Move the shared snapshot reference resolver into the run path, resolve exactly one source for the current task/target invocation, validate it against the target state's `snapshot.inherit` contract unless `--override-inherit` is present, perform native compatibility checks, and stage the selected snapshot before spawning the agent.

- Q-redactor-unused: `snapshots.redactor` is parsed but never executed for snapshot writes.
  - Severity: medium
  - File: crates/rhei-cli/src/main.rs
  - Detail: The settings schema accepts `snapshots.redactor` and `redactor_env`, but there is no redactor process implementation or call site in the snapshot/runtime paths. When snapshot emission is wired, transcripts will be written without the configured privacy boundary, and failures/timeouts from the redactor cannot abort writes as specified.
  - Evidence: Spec lines 247-272 require Rhei to execute the configured redactor on every transcript before cache write, with controlled cwd/env, finite timeout, stdout replacement, stderr diagnostics, and abort-on-failure semantics. Repository search shows `redactor` only in settings parsing/merging at `crates/rhei-cli/src/main.rs:6135-6165` and `crates/rhei-cli/src/main.rs:6185-6189`; there is no execution hook.
  - Suggested fix: Add a redaction helper used by the atomic snapshot writer before sha256/manifest calculation, with the specified cwd, minimal environment plus `redactor_env`, timeout/kill behavior, stderr logging, and write abort on nonzero exit or timeout.

- Q-current-fallback: Omitted generation references silently select newest when `current` is absent.
  - Severity: medium
  - File: crates/rhei-cli/src/main.rs
  - Detail: The shared resolver is supposed to resolve omitted generations to `current` or report ambiguity. Instead, `select_current_records` sorts each identity by descending generation and falls back to the newest generation if no record has `is_current`. A missing or corrupt `current` pointer can therefore make `show` or `continue` inspect a generation that is not actually current, hiding cache corruption and applying a command-specific tie-breaker.
  - Evidence: Spec lines 118-123 say omitted generations for `continue` resolve to `current`, and lines 24-27 require unresolved positional ambiguity to be an error rather than using tie-breakers. The implementation at `crates/rhei-cli/src/main.rs:9307-9312` calls `select_current_records` when no generation is provided; `select_current_records` at `crates/rhei-cli/src/main.rs:9445-9460` uses `.find(|record| record.is_current).or_else(|| group.into_iter().next())`.
  - Suggested fix: When a generation is omitted, require a valid `current` record for each matched identity. If no current pointer resolves for an identity with matching generations, return a clear cache-integrity/ambiguous-reference error and let operators retry with `/g<N>` or repair the pointer.
