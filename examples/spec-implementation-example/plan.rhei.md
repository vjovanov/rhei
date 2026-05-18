# Rhei: rhei-list Spec Implementation Example — docs/functional-spec/rhei-list.spec.md
**States:** spec-implementation

## What this workspace does

Implement the specification at `docs/functional-spec/rhei-list.spec.md` end-to-end and walk it through:

1. **Implementation** by `claude-code[yolo]:anthropic:claude-opus-4-7`.
2. **Completeness audit** — each reviewer independently checks whether the
   implementation covers every normative claim in the spec; `codex[xhigh]:openai:gpt-5.5`
   merges the per-reviewer findings into one gap list; `claude-code[yolo]:anthropic:claude-opus-4-7`
   closes the gaps.
3. **Quality review/fix loop** — `2` cycles. Each cycle:
   - every reviewer reviews the implementation in parallel,
   - `codex[xhigh]:openai:gpt-5.5` writes a fix plan,
   - `codex[xhigh]:openai:gpt-5.5` applies the accepted fixes.
4. **End-to-end coverage loop** — `2` cycles:
   - `claude-code[yolo]:anthropic:claude-opus-4-7` writes / extends e2e tests against the mock agent
     (`mock`),
   - `codex[xhigh]:openai:gpt-5.5` re-runs the standard suite, audits the new tests,
     enforces the mock-agent policy, and lists remaining gaps for the next
     write pass.

## Configuration

| Role | Target |
|---|---|
| Implementer | `claude-code[yolo]:anthropic:claude-opus-4-7` |
| Reviewers (fan-out) | `claude-code[yolo]:anthropic:claude-opus-4-7`, `codex[xhigh]:openai:gpt-5.5` |
| Smart target (aggregate, fix) | `codex[xhigh]:openai:gpt-5.5` |
| E2E writer | `claude-code[yolo]:anthropic:claude-opus-4-7` |
| E2E verifier | `codex[xhigh]:openai:gpt-5.5` |

Quality loop cycles: **2** &nbsp;·&nbsp; E2E loop cycles: **2**

Quality reviewers must address these focus areas:
- `performance`
- `error handling`
- `concurrency`

End-to-end tests live under `e2e`.

## E2E test policy

Every newly-added e2e test MUST target the mock agent (`mock`),
which returns canned outputs the test controls. The standard suite stays
fast, deterministic, and offline.

Tests that exercise real agent operations are reserved for a small
release-only subset, marked with `release-only` so CI can exclude
them from the default test command and include them only in release builds. Those tests live under
`e2e/release-only`. The rule of thumb is one
happy-path test per distinct real-agent integration — the verifier flags
growth beyond that.

## Where work happens

This workspace is a **scratchpad**. Every state resolves the repository root
with `git rev-parse --show-toplevel` and applies code edits in the repository
checkout. Runtime artifacts (`runtime/...`) stay under this workspace.

## Tasks

### Task spec-implementation: Implement docs/functional-spec/rhei-list.spec.md
**State:** implement

Implement the spec at `docs/functional-spec/rhei-list.spec.md` end-to-end and walk it through the
completeness pass, the quality review/fix loop, and the e2e coverage loop
defined in `states.yaml`.