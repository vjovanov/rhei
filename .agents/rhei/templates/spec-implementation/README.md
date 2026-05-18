# spec-implementation

Take a specification and produce a reviewed, fix-cycled, e2e-covered
implementation of it — without you babysitting the loop.

## What it does

You point this template at a spec file. It instantiates a workspace whose
single task walks one agent through implementation, then puts the result
through three structured passes:

1. **Completeness audit** — fan-out across reviewers. "Did we implement
   everything the spec calls out?"
2. **Quality review/fix loop** — fan-out review, smart aggregation, smart
   fix. Repeats N times. "Is what we implemented actually correct?"
3. **E2E coverage loop** — two agents ping-pong: one writes tests against
   the mock agent, the other independently runs and audits them. Repeats M
   times. "Do tests actually exercise the spec through the public
   interface — without us paying for real model calls on every commit?"

Each stage writes its artifacts under `runtime/...` in the workspace so
you can inspect what each agent saw and decided.

## When to use it

- You have a written spec and you want an implementation you can trust
  without doing the whole review loop by hand.
- The work is well-scoped enough that one task can carry it end-to-end (a
  CLI subcommand, an API endpoint, a new state machine, a small subsystem).
  For larger work, split the spec and instantiate this template per piece.
- You're willing to pay for ~`1 + 2N + 2M` agent invocations of the
  configured targets — exact cost depends on `review_passes` and
  `e2e_passes`.

If your need is closer to *audit*, *review*, or *spec drafting*, use the
neighboring templates instead (`spec-implementation-discrepancy-audit`,
`changeset-review`, or `spec-review`).

## Flow at a glance

```
   implement                  (implementation_target)
     |
     v
   completeness-review        (fan-out: review_targets)
     |
     v
   completeness-aggregate     (smart_target)
     |
     v
   completeness-fix           (implementation_target)
     |
     v
   +-> quality-review         (fan-out: review_targets)
   |     |
   |     v
   |   quality-aggregate      (smart_target)
   |     |
   |     v
   |   quality-fix            (smart_target)
   +-- loop x review_passes
         |
         v
   +-> e2e-write              (e2e_writer)
   |     |
   |     v
   |   e2e-verify             (e2e_verifier)
   +-- loop x e2e_passes
         |
         v
       completed
```

The state machine diagram lives at the top of [`states.yaml`](./states.yaml).

## Inputs

The only required input is `spec_path`. Everything else has a sensible default.

| Input | Type | Default | What it does |
|---|---|---|---|
| `spec_path` | string | *(required)* | Path to the spec file (or directory). |
| `spec_title` | string | `Spec Implementation` | Workspace title. |
| `implementation_target` | string | `claude-code[yolo]:anthropic:claude-opus-4-7` | Agent that implements and closes completeness gaps. |
| `review_targets` | string[] | `[claude-code…, codex…]` | Reviewers — used in both completeness and quality reviews. Add more for higher confidence at higher cost. |
| `smart_target` | string | `codex[xhigh]:openai:gpt-5.5` | Aggregates per-reviewer findings, writes the fix plan, applies the fixes. |
| `review_passes` | number | `2` | How many quality review/fix cycles. |
| `focus_areas` | string[] | `[]` | Optional focus sections each quality reviewer must address. |
| `e2e_writer` | string | `claude-code[yolo]:anthropic:claude-opus-4-7` | Agent that adds e2e tests. |
| `e2e_verifier` | string | `codex[xhigh]:openai:gpt-5.5` | Agent that re-runs and audits the e2e suite. Should differ from `e2e_writer`. |
| `e2e_passes` | number | `2` | How many e2e write/verify cycles. |
| `e2e_test_root` | string | empty | Where e2e tests live (e.g., `e2e/`). Empty = let the writer discover. |
| `mock_agent` | string | `mock` | The mock agent selector / test-side name every standard e2e test must target. |
| `release_only_marker` | string | `release-only` | Tag / attribute / filename suffix that marks tests which hit real agent operations and run only on release builds. |
| `release_only_test_root` | string | empty | Optional separate directory for release-only real-agent tests. Empty = co-locate and rely on the marker. |

### E2E test policy (enforced by the template)

Every standard e2e test added by this workflow MUST target the **mock agent**
(`mock_agent`, default `mock`), which returns canned outputs the test
controls. This keeps the suite fast, deterministic, and offline — no real
model calls on every commit.

A small **release-only subset** may invoke real agent operations to verify
that the agent integration still works end-to-end. These tests are marked
with `release_only_marker` (default `release-only`) so the project's CI
runner can:

- exclude them from the default test command, and
- include them only in release builds.

The verifier (`e2e_verifier`) enforces both halves of the policy: it flags
any standard test that calls real-agent code paths, and it flags growth in
the release-only subset beyond ~one happy-path test per distinct real-agent
integration.

The template ships defaults that match a common project convention; override
them per-instantiation when your project uses different names:

```bash
--set mock_agent='MockAgent::canned' \
--set release_only_marker='@RealAgent' \
--set release_only_test_root='e2e/real-agent/'
```

### Common configurations

- **Default (balanced, ~2h–4h of agent time):** just set `spec_path`.
- **Faster, lower-confidence pass:** `--set review_passes=1 --set e2e_passes=1`.
- **Heavier audit:** `--set review_passes=3` and add a third reviewer.
- **Skip e2e entirely:** `--set e2e_passes=1` and accept whatever the
  verifier reports; the workflow does not yet support skipping the loop
  altogether without editing the plan.

## Quick start

```bash
rhei instantiate spec-implementation \
  --set spec_path=docs/functional-spec/my-feature.spec.md \
  --output .agents/scratchpad/spec-implementation/

rhei run .agents/scratchpad/spec-implementation/plan.rhei.md
```

For non-scalar inputs (`review_targets`, `focus_areas`), use a values file:

```bash
rhei instantiate spec-implementation \
  --values my-values.yaml \
  --output .agents/scratchpad/spec-implementation/
```

## Where things land

Each state writes its artifacts under `runtime/...` in the workspace:

| Directory | What's in it |
|---|---|
| `runtime/implement/` | Implementation notes (what was implemented, what was deferred). |
| `runtime/completeness/` | Per-reviewer gap inventories, merged gap list, fix log. |
| `runtime/quality/` | Per-reviewer findings, fix plan, and fix log — one set per pass. |
| `runtime/e2e/` | Write report + verify report — one set per pass. |

The fix-and-edit work itself happens in the repository checkout (resolved
via `git rev-parse --show-toplevel`), not in the workspace.

## What's bundled

- `template.yaml` — input manifest.
- `states.yaml` — the `spec-implementation` state machine (diagram in the
  top comment).
- `plan.rhei.md` — the single-task plan skeleton.
- `settings.json` — adds Codex `high` / `xhigh` modes (the default
  `smart_target` uses `xhigh`) and sets `agent_timeout: 2h` since
  implementation states can run long.

## Example

A pre-rendered example lives at
[`examples/spec-implementation-example/`](../../../../examples/spec-implementation-example/)
and passes `rhei validate` as shipped.
