# Aggregated Completeness Gaps: impl-rhei-snapshot-operations

Spec: `docs/functional-spec/rhei-snapshot-operations.spec.md`

Inputs:

- `runtime/completeness/impl-rhei-snapshot-operations-gaps-codex-xhigh-openai-gpt-5-5.md`

Aggregation rule: retained items marked `missing` or `partial` by any reviewer unless all reviewers marked the same requirement covered with concrete evidence. Where reviewers disagree, the disagreement is preserved for `completeness-fix`.

There was only one reviewer inventory for this task, so there are no cross-reviewer disagreements to preserve.

## Snapshot List

- `partial`: `snapshot list --orphaned` detects task/state/target orphans, but task existence only checks root tasks and does not recurse into child tasks. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:41`, `crates/rhei-cli/src/main.rs:417`, `crates/rhei-cli/src/main.rs:8532`, `crates/rhei-cli/src/main.rs:9389`, `crates/rhei-cli/src/main.rs:9390`, `crates/rhei-cli/src/main.rs:5902`.

## Snapshot Gc

- `partial`: `gc --keep-generations <n>` groups by the required identity, but applies `--older-than` before retention grouping, so newer non-eligible generations are not counted when deciding which older records to delete. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:69`, `crates/rhei-cli/src/main.rs:8453`, `crates/rhei-cli/src/main.rs:9061`, `crates/rhei-cli/src/main.rs:9067`, `crates/rhei-cli/src/main.rs:9074`, `crates/rhei-cli/src/main.rs:9202`.

- `partial`: `gc --orphaned` inherits the child-task orphan detection gap from `snapshot list --orphaned`. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:76`, `crates/rhei-cli/src/main.rs:453`, `crates/rhei-cli/src/main.rs:9064`.

- `partial`: GC active-inherit protection evaluates snapshot inherit selectors for non-terminal root tasks, but does not consider child tasks in active non-terminal states. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:89`, `crates/rhei-cli/src/main.rs:9078`, `crates/rhei-cli/src/main.rs:9115`, `crates/rhei-cli/src/main.rs:9117`, `crates/rhei-cli/src/main.rs:9123`.

## Snapshot Continue

- `missing`: `snapshot continue <ref>` resolves and validates some preconditions, but does not spawn an interactive agent session preloaded with the referenced snapshot. It returns the deferred `unsupported-snapshot-session` error. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:105`, `crates/rhei-cli/src/main.rs:9312`.

- `missing`: successful continuation does not capture the operator-driven transcript as a sibling generation under the same identity with `produced_by: operator`. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:106`, `crates/rhei-cli/src/main.rs:9312`.

- `missing`: because the successful continuation path is absent, the required no-current-advance and no-plan-runtime-mutation behavior is not implemented for successful operator generation writes. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:109`, `crates/rhei-cli/src/main.rs:9312`.

- `partial`: `snapshot list` can filter `produced_by: operator` and full refs can resolve generations, but `snapshot continue` never writes operator transcripts to the cache. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:114`, `crates/rhei-cli/src/main.rs:8622`, `crates/rhei-cli/src/main.rs:8825`, `crates/rhei-cli/src/main.rs:9312`.

- `missing`: `continue --no-capture` is parsed, but the implementation only binds the value and returns the deferred unsupported error; no interactive session runs and no discard path exists. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:136`, `crates/rhei-cli/src/main.rs:477`, `crates/rhei-cli/src/main.rs:9312`.

- `partial`: `continue` checks configured profile session shape and can emit `unsupported-snapshot-session`, but built-in profiles are not resolved through this path when absent from settings, and there is no eventual spawn path. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:140`, `crates/rhei-cli/src/main.rs:9303`, `crates/rhei-cli/src/main.rs:9306`.

- `missing`: the interactive continuation profile does not construct or preserve TTY pass-through behavior; it only checks that interactive/session-layout/resume fields exist. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:143`, `crates/rhei-cli/src/main.rs:9318`.

- `partial`: `continue` checks whether `.rhei/run.lock` is already held, but it does not acquire and hold the lock for the interactive session because no successful session path exists. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:152`, `crates/rhei-cli/src/main.rs:9284`, `crates/rhei-cli/src/main.rs:9469`.

- `missing`: operator generation manifests are never written, so `produced_by: operator` is not recorded by `continue`. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:160`, `crates/rhei-cli/src/main.rs:9312`.

- `missing`: operator generation manifests are never written, so `parent_ref` is not recorded. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:163`, `crates/rhei-cli/src/main.rs:9312`.

- `missing`: operator generation manifests are never written, so clean/nonzero interactive exits are not classified as `completion: success` or `completion: failure`, and the no-`timeout` classification rule is not exercised. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:166`, `crates/rhei-cli/src/main.rs:9312`.

- `missing`: operator sibling generations are not allocated with the atomic-write procedure; runtime emission is a no-op and continuation returns the deferred error. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:170`, `crates/rhei-cli/src/main.rs:11487`, `crates/rhei-cli/src/main.rs:9312`.

- `missing`: operator generation collision-retry and pointer-chain interleaving are not implemented because no operator generation write path exists. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:171`, `crates/rhei-cli/src/main.rs:9312`.

## Run Override

- `missing`: `rhei run --from-snapshot` does not resolve the override reference or override the concrete source snapshot after authored `snapshot.inherit` constraints. The preload hook only checks that `snapshot.inherit` exists and otherwise no-ops. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:179`, `crates/rhei-cli/src/main.rs:11530`, `crates/rhei-cli/src/main.rs:11544`.

- `missing`: without `--override-inherit`, `--from-snapshot` does not check declared name, `from`, selected state, target, visit/generation, `required`, or `compat` constraints. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:182`, `crates/rhei-cli/src/main.rs:11544`.

- `partial`: `--override-inherit` exists and requires `--from-snapshot`, but there are no source-selection or compatibility checks for it to bypass. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:186`, `crates/rhei-cli/src/main.rs:5719`.

- `missing`: without `--override-inherit`, `--from-snapshot` is not rejected for authored `compat: none` or native compatibility failures because compatibility evaluation is not implemented. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:188`, `crates/rhei-cli/src/main.rs:11517`, `crates/rhei-cli/src/main.rs:11544`.

- `partial`: with `--override-inherit`, missing `snapshot.inherit` is still rejected, but source and compatibility checks do not exist to be bypassed. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:190`, `crates/rhei-cli/src/main.rs:11537`.

- `partial`: `rhei run` has selector flags such as `--task` and `--target`, but ambiguous override detection and selector enforcement do not exist because the override is not resolved. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:194`, `crates/rhei-cli/src/main.rs:5723`.

- `missing`: ambiguous `--from-snapshot` overrides do not exit with candidate matches because the preload hook does not call the shared resolver or produce ambiguity candidates. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:196`, `crates/rhei-cli/src/main.rs:11544`.

## Phased Rollout

- `partial`: phase 1 snapshot grammar parsing is minimal; validator structs parse `snapshot.emit` and `snapshot.inherit` shapes, but full grammar validation is not implemented. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:203`, `crates/rhei-validator/src/lib.rs:588`.

- `missing`: phase 1 parse-time errors for unsupported `compat` values are not implemented; `compat` is a plain optional string. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:203`, `crates/rhei-validator/src/lib.rs:611`.

- `missing`: phase 1 parse-time errors for `select.target: all` are not implemented; `target` is a plain optional string. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:203`, `crates/rhei-validator/src/lib.rs:624`.

- `missing`: the phase 1.5 redactor process contract is not implemented; settings parse `redactor`, but no redactor execution path exists. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:204`, `crates/rhei-cli/src/main.rs:6006`.

- `missing`: phase 3 claude-code end-to-end runtime snapshots and integration tests are absent; runtime snapshot writes remain a no-op. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:206`, `crates/rhei-cli/src/main.rs:11487`.

- `missing`: phase 4 pi end-to-end snapshot/fork support is absent; runtime snapshot emission remains a no-op. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:207`, `crates/rhei-cli/src/main.rs:11487`.

- `missing`: phase 5 codex snapshot resume support is absent; continuation transport is deferred. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:208`, `crates/rhei-cli/src/main.rs:9312`.

- `partial`: phase 6 CLI surfaces for `snapshot continue` and `--from-snapshot` exist, but actual continue transport and run override preload are deferred/no-op. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:209`, `crates/rhei-cli/src/main.rs:463`, `crates/rhei-cli/src/main.rs:5712`, `crates/rhei-cli/src/main.rs:9312`, `crates/rhei-cli/src/main.rs:11544`.

- `partial`: snapshot-free execution is mostly unaffected by the current stubs, but full phase boundaries are not mechanically encoded beyond those stubs. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:212`, `crates/rhei-cli/src/main.rs:11530`, `crates/rhei-cli/src/main.rs:11492`.

- `partial`: settings and list/show/gc exist, but redaction execution and runtime auto-emit are not implemented, so auto-emit does not start after all required prerequisites. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:215`, `crates/rhei-cli/src/main.rs:5998`, `crates/rhei-cli/src/main.rs:8518`, `crates/rhei-cli/src/main.rs:8549`, `crates/rhei-cli/src/main.rs:8554`, `crates/rhei-cli/src/main.rs:11487`.

- `missing`: once an agent can be snapshotted, state-exit auto-emission by default is not implemented; the emit hook is a no-op. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:218`, `crates/rhei-cli/src/main.rs:11487`.

- `partial`: `snapshot continue` has a CLI surface, but it returns the phase-6 deferred error until interactive transport is supported. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:220`, `crates/rhei-cli/src/main.rs:9312`.

## Configuration

- `missing`: `snapshots.provider_cache_ttl` is stored, but no `cache_beneficial` predicate or TTL use exists. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:243`, `crates/rhei-cli/src/main.rs:6004`.

## Redaction Hook

- `missing`: when `snapshots.redactor` is set, Rhei does not execute the named program before writing transcripts to the cache because redactor execution and runtime snapshot writes are absent. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:249`, `crates/rhei-cli/src/main.rs:11487`, `crates/rhei-cli/src/main.rs:6006`.

- `missing`: no redactor subprocess is spawned, so the staged-transcript stdin and redacted stdout contract is absent. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:250`, `crates/rhei-cli/src/main.rs:6006`.

- `missing`: non-zero redactor exits do not abort snapshot writes with a clear error because no redactor process or snapshot write abort path exists. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:252`, `crates/rhei-cli/src/main.rs:11487`.

- `missing`: redaction does not run inside an atomic-write window before sha256 computation because atomic snapshot writes and redaction are absent. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:252`, `crates/rhei-cli/src/main.rs:11487`.

- `missing`: the redactor does not run with cwd set to the plan workspace root because redactor execution is not implemented. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:257`, `crates/rhei-cli/src/main.rs:6042`.

- `partial`: settings include `redactor_env`, but no redactor subprocess or minimal environment construction exists. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:258`, `crates/rhei-cli/src/main.rs:6007`.

- `partial`: projects can parse `redactor_env` settings, but no environment forwarding is executed. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:260`, `crates/rhei-cli/src/main.rs:6007`, `crates/rhei-cli/src/main.rs:6034`.

- `missing`: finite redactor timeout handling, termination, and kill-after-grace behavior are not implemented. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:262`.

- `missing`: redactor stdin/stdout handling and runtime output limits are not implemented. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:265`.

- `missing`: redactor stderr diagnostics capture, truncation, and exclusion from `manifest.json` are not implemented. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:268`.

- `missing`: redactor path, exit status, timeout/truncation outcome, and stderr summary are not logged to the run log. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:270`.

- `missing`: the manifest rule that it does not record whether a redactor ran is not exercised because no redacted snapshot manifest-writing path exists. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:271`, `crates/rhei-cli/src/main.rs:11487`.

- `missing`: redaction opacity via sha256 computed on redacted bytes is not implemented because no redaction or sha256 computation path exists. Evidence: `docs/functional-spec/rhei-snapshot-operations.spec.md:278`, `crates/rhei-cli/src/main.rs:11487`.
