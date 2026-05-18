# Rhei: Spec Implementation
**States:** spec-implementation

---
metadata:
  tasks:
    impl-rhei-agents:
      stateVisits:
        quality-review: 2
        quality-aggregate: 2
        quality-fix: 2
    impl-rhei-run:
      stateVisits:
        quality-review: 2
        quality-aggregate: 2
        quality-fix: 1
    impl-rhei-snapshot-operations:
      stateVisits:
        quality-review: 2
        quality-aggregate: 2
        quality-fix: 1
    impl-rhei-snapshots:
      stateVisits:
        quality-review: 1
        quality-aggregate: 1
        quality-fix: 1
---

## What this workspace does

Implements one or more specifications end-to-end. Every spec goes through:

1. **Implementation** by `codex[xhigh]:openai:gpt-5.5`.
2. **Completeness audit** — `codex[xhigh]:openai:gpt-5.5` checks for
   missing coverage, merges the findings, and closes the gaps.
3. **Quality review/fix loop** — `2` cycles per spec:
   - `codex[xhigh]:openai:gpt-5.5` reviews,
   - `codex[xhigh]:openai:gpt-5.5` writes a fix plan,
   - `codex[xhigh]:openai:gpt-5.5` applies the accepted fixes.

After every per-spec pipeline completes, a **shared end-to-end coverage loop**
runs once across all implemented specs: `codex[xhigh]:openai:gpt-5.5`
writes tests (targeting the mock agent `mock` by default), then re-runs
the standard suite and audits the new tests. The loop runs for `2`
cycles.

## Input mode

Exactly one of these must be set when instantiating:

- `spec_path` — single-spec mode. One spec file gets one per-spec task.
- `spec_ref` — multi-spec mode. A reference (PR / branch / commit range /
  diff file) whose changed `*.spec.md` files each get their own per-spec task.
  All per-spec tasks share one e2e loop at the end.

This instantiation has:

- `spec_path` = `(empty)`
- `spec_ref`  = `PR#2`

The coordinator task verifies the XOR at the start of the run and fails fast
if both or neither are set.

## Configuration

| Role | Target |
|---|---|
| Implementer (per spec) | `codex[xhigh]:openai:gpt-5.5` |
| Reviewers (per spec) | `codex[xhigh]:openai:gpt-5.5` |
| Smart target (coordinator, aggregate, fix) | `codex[xhigh]:openai:gpt-5.5` |
| E2E writer (shared loop) | `codex[xhigh]:openai:gpt-5.5` |
| E2E verifier (shared loop) | `codex[xhigh]:openai:gpt-5.5` |

Quality loop cycles per spec: **2** &nbsp;·&nbsp; E2E loop cycles: **2**

## E2E test policy

Every newly-added e2e test MUST target the mock agent (`mock`),
which returns canned outputs the test controls. The standard suite stays
fast, deterministic, and offline.

Tests that exercise real agent operations are reserved for a small
release-only subset, marked with `release-only` so CI can exclude
them from the default test command and include them only in release builds. The rule of thumb is one
happy-path test per distinct real-agent integration — the verifier flags
growth beyond that.

## Where work happens

This workspace is a **scratchpad**. Every state resolves the repository root
with `git rev-parse --show-toplevel` and applies code edits in the repository
checkout. Runtime artifacts (`runtime/...`) and dynamic per-spec task files
(`tasks/...`) stay under this workspace.

## Notes

- The workspace is "living": the coordinator appends per-spec implementation
  task files and the shared e2e-aggregate task file under `tasks/` during
  the run. `rhei reset` clears state but does not delete dynamically
  appended task files.
- Instantiate inside the repository being worked on, ideally under
  `.agents/scratchpad/`, so `git rev-parse --show-toplevel` from the
  workspace resolves the project root deterministically.
