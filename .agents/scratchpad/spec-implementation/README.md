# spec-implementation

Take one or more specifications and produce reviewed, fix-cycled,
e2e-covered implementations of them — without you babysitting the loop.

## What it does

You point this template at either a single spec file (`spec_path`) or a
git reference (`spec_ref` — PR, branch, commit range, or `.diff` file)
whose changed `*.spec.md` files become the work set. It instantiates a
workspace with a coordinator task that resolves the input and fans out:

1. **Per-spec pipeline** (one task per spec, run in parallel):
   - **Implementation** by Codex.
   - **Completeness audit** — Codex checks coverage. "Did we implement
     everything the spec calls out?"
   - **Quality review/fix loop** — fan-out review, smart aggregation,
     smart fix. Repeats N times. "Is what we implemented actually correct?"
2. **Shared E2E coverage loop** (runs once, after every per-spec task
   completes):
   - Codex writes tests against the mock agent, then independently runs
     and audits them. Repeats M times. "Do tests
     actually exercise the specs through the public interface — without us
     paying for real model calls on every commit?"

Each stage writes its artifacts under `runtime/...` in the workspace so
you can inspect what each agent saw and decided.

## When to use it

- You have a written spec (or several) and you want implementations you
  can trust without running the whole review loop by hand.
- For multi-spec mode, the changed specs are related enough that one
  shared e2e pass over them makes sense. For unrelated specs, instantiate
  separately per spec.
- You're willing to pay for ~`1 + (1 + 2N) × S + 2M` agent invocations
  where S is the spec count, N is `review_passes`, and M is `e2e_passes`.

If your need is closer to *audit*, *review*, or *spec drafting*, use the
neighboring templates (`spec-implementation-discrepancy-audit`,
`changeset-review`, `spec-review`).

## Input mode (XOR)

Exactly one of these must be set:

| Mode | Input | Example |
|---|---|---|
| Single-spec | `spec_path` | `--set spec_path=docs/functional-spec/rhei-list.spec.md` |
| Multi-spec from diff | `spec_ref` | `--set spec_ref=main..HEAD` or `--set spec_ref=PR#42` |

The coordinator state checks the XOR at the start of the run and stops
with a clear error if both or neither are set. (The check happens at run
time, not at instantiation time — instantiation will still produce a
workspace even if the inputs are wrong; the coordinator catches it.)

For `spec_ref`, accepted forms:

- PR URL or number (`PR#42`, full URL, `org/repo#42`) — uses `gh`.
- Branch name — compared against the project's default branch.
- Commit SHA or range `base..head` — uses `git diff`.
- Path to a `.diff` or `.patch` file on disk.

The coordinator filters to paths matching `*.spec.md` under the project's
spec directory.

## Flow at a glance

```
   coordinate                    (smart_target)
     |
     |  appends:
     |    - tasks/NN-impl-<slug>.md   (one per spec)
     |    - tasks/NN-e2e-aggregate.md (one, depends on every impl task)
     v
   completed                     (coordinator task done)

   --- per impl-<slug> task ---
   implement                     (codex)
     |
     v
   completeness-review           (codex)
     |
     v
   completeness-aggregate        (smart_target)
     |
     v
   completeness-fix              (codex)
     |
     v
   +-> quality-review            (codex)
   |     |
   |     v
   |   quality-aggregate         (smart_target)
   |     |
   |     v
   |   quality-fix               (smart_target)
   +-- loop x review_passes
         |
         v
       completed                 (per-spec task done)

   --- e2e-aggregate task, after every impl-<slug> completes ---
   +-> e2e-write                 (codex)
   |     |
   |     v
   |   e2e-verify                (codex)
   +-- loop x e2e_passes
         |
         v
       completed                 (workflow done)
```

The state machine diagram (with per-task paths) lives at the top of
[`states.yaml`](./states.yaml).

## Inputs

The only required choice is one of `spec_path` / `spec_ref`. Everything
else has a sensible default.

| Input | Type | Default | What it does |
|---|---|---|---|
| `spec_path` | string | empty | Single-spec mode. Path to one spec file. |
| `spec_ref` | string | empty | Multi-spec mode. PR / branch / commit range / diff file whose changed `*.spec.md` files become the work set. |
| `spec_title` | string | `Spec Implementation` | Workspace title. |
| `implementation_target` | string | `codex[xhigh]:openai:gpt-5.5` | Agent that implements and closes completeness gaps. |
| `review_targets` | string[] | `[codex[xhigh]:openai:gpt-5.5]` | Reviewers — used in both completeness and quality reviews per spec. |
| `smart_target` | string | `codex[xhigh]:openai:gpt-5.5` | Coordinator; aggregator; writes fix plan; applies fixes. |
| `review_passes` | number | `2` | Quality review/fix cycles per spec. |
| `focus_areas` | string[] | `[]` | Optional focus sections each quality reviewer must address. |
| `e2e_writer` | string | `codex[xhigh]:openai:gpt-5.5` | Agent that adds e2e tests in the shared loop. |
| `e2e_verifier` | string | `codex[xhigh]:openai:gpt-5.5` | Agent that re-runs and audits the e2e suite. |
| `e2e_passes` | number | `2` | E2E write/verify cycles. |
| `e2e_test_root` | string | empty | Where e2e tests live (e.g., `e2e/`). Empty = let the writer discover. |
| `mock_agent` | string | `mock` | The mock agent selector every standard e2e test must target. |
| `release_only_marker` | string | `release-only` | Tag the CI runner uses to exclude real-agent tests from the default suite. |
| `release_only_test_root` | string | empty | Optional separate directory for release-only real-agent tests. |

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

Override the defaults per-instantiation when your project uses different
names:

```bash
--set mock_agent='MockAgent::canned' \
--set release_only_marker='@RealAgent' \
--set release_only_test_root='e2e/real-agent/'
```

### Common configurations

- **Single spec, balanced:** `--set spec_path=docs/.../my-feature.spec.md`.
- **All specs in the current branch:** `--set spec_ref=main..HEAD`.
- **A specific PR:** `--set spec_ref=PR#42`.
- **Faster, lower-confidence pass:** add `--set review_passes=1 --set e2e_passes=1`.
- **Heavier audit:** `--set review_passes=3` and add a third reviewer.

## Quick start

```bash
# Single spec
rhei instantiate spec-implementation \
  --set spec_path=docs/functional-spec/my-feature.spec.md \
  --output .agents/scratchpad/spec-implementation/

# All specs changed on the current branch
rhei instantiate spec-implementation \
  --set spec_ref=main..HEAD \
  --output .agents/scratchpad/spec-implementation/

rhei run .agents/scratchpad/spec-implementation/
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
| `runtime/manifests/` | Coordinator-written spec assignments and per-task spec paths. |
| `runtime/implement/` | Per-spec implementation notes (what was implemented, what was deferred). |
| `runtime/completeness/` | Per-reviewer gap inventories, merged gap list, fix log — per spec. |
| `runtime/quality/` | Per-reviewer findings, fix plan, and fix log — per spec, per pass. |
| `runtime/e2e/` | Shared write report + verify report — per pass. |

> **Note on tracked runtime artifacts.** The committed files under
> `runtime/quality/` and `runtime/completeness/` are a frozen record of the
> review passes that produced this branch's snapshot work. Some entries cite
> the in-flight source layout used during those passes — for example paths
> like `crates/rhei-cli/src/main_parts/snapshot_runtime_1.rs` or
> `crates/rhei-cli/src/main_parts/tests_7.rs`. After the source-tree split
> (see `docs/architecture/source-file-size.spec.md`), the equivalent code
> now lives under `crates/rhei-cli/src/cli/` with behavior-named modules
> (`snapshot_runtime_emit.rs`, `snapshot_runtime_preload.rs`,
> `tests_snapshot_runtime.rs`, etc.). Treat the paths and line numbers in
> the runtime logs as historical evidence pointers, not navigation links.

The fix-and-edit work itself happens in the repository checkout (resolved
via `git rev-parse --show-toplevel`), not in the workspace. The
coordinator also appends per-spec task files and the e2e-aggregate task
file under `tasks/` during the run.

## What's bundled

- `template.yaml` — input manifest.
- `states.yaml` — the `spec-implementation` state machine (diagram in the
  top comment).
- `index.rhei.md` — workspace index.
- `tasks/01-coordinate.md` — the initial coordinator task skeleton.
- `settings.json` — adds Codex `high` / `xhigh` modes (the default
  `smart_target` uses `xhigh`) and sets `agent_timeout: 2h` since
  implementation states can run long.

## Example

A pre-rendered example lives at
[`examples/spec-implementation-example/`](../../../../examples/spec-implementation-example/)
and passes `rhei validate` as shipped.
