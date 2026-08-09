# spec-review

A small, self-contained **counted review/fix loop** for a single
specification file. Point it at a spec and it runs two review passes — each
followed by a fix pass that applies the findings directly to the spec — then
stops. This is the simplest template in the set and the canonical reference for
the *loop* pattern (`visits: N` + a `visitCount`-conditioned transition).

## What it does

One task walks a four-state machine: `review` writes findings for the `spec` to
a per-pass artifact, `fix` reads those findings and edits the spec, and the two
states each declare `visits: 2`, so the task loops `review → fix → review → fix`
and then transitions to `completed`. A wildcard `* → cancelled` lets a human
abort from any state.

## When to use it

- You have one written spec and want a bounded, hands-off review-and-fix pass
  over it.
- You want a minimal example of the counted-loop shape to copy into a larger
  template. For multi-spec, multi-agent, or aggregation work, reach for
  `spec-implementation`, `changeset-review`, or `multi-model-analysis` instead.

## Inputs

| Input | Type | Default | What it does |
|---|---|---|---|
| `spec` | string | *(required)* | The specification file to review. Rendered exactly as supplied, so use a path the spawned agent can read from the instantiated workspace. |
| `criteria` | string | empty | Extra things each review pass must look for (e.g., `thread safety, backward compatibility`). Empty means a general consistency/completeness/correctness/clarity review. |

`spec` is positional-friendly via the single-required-input fallback:
`rhei instantiate spec-review docs/.../my.spec.md` works without `--set`.

## Flow at a glance

```
review [initial]  --(write findings)-->  fix
   ^                                       |
   |  visitCount < visits                  |
   +---------------------------------------+
                                           |
                       visitCount >= visits
                                           v
                                     completed [final]

any non-final state --> cancelled [final]
```

The full state machine diagram (with per-task paths) lives at the top of
[`states.yaml`](./states.yaml).

Per-task path:

| Task | Path through the machine |
|---|---|
| `spec-review` | `review → fix → review → fix → completed` (2 passes) |

## Quick start

```bash
rhei instantiate spec-review \
  --set spec=docs/functional-spec/rhei-templates.spec.md \
  --set criteria="thread safety, backward compatibility" \
  --output .agents/scratchpad/spec-review/

rhei run .agents/scratchpad/spec-review/
```

## What's bundled

- `template.yaml` — the two inputs.
- `states.yaml` — the `spec-review` counted-loop machine (diagram in the top
  comment). Reviews run on the bundled `codex` agent.
- `settings.json` — project defaults with `agent_timeout: 20m`, so the quick
  start runs without requiring a global timeout setting.
- `index.rhei.md` — workspace index.
- `tasks/01-review.md` — the single review task skeleton.

## Example

A pre-rendered example lives at
[`examples/spec-review-example/`](../../../../examples/spec-review-example/)
and passes `rhei validate` as shipped.
