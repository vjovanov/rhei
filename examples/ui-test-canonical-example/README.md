# ui-test-canonical - example

A pre-rendered instantiation of the
[`ui-test-canonical`](../../.agents/rhei/templates/ui-test-canonical/)
template. This example is a smoke test and a convenient fixture for developing
the Rhei UI against deterministic mock execution.

Every task is named after the Rhei feature it exercises (see the template's
task-to-feature coverage matrix). It intentionally includes runnable work,
four-level task nesting, dependency blocking, generated follow-up work, a seeded
human gate, and several tasks that reach `blocked` — both seeded and live (poll
exhaustion, a live program failure, and unavailable skill/MCP tooling). A live
run is expected to stop with `full-pipeline` parked at the human gate and the
`blocked` tasks visible for inspection.

## Inputs used

```yaml
plan_title: Rhei UI Canonical Test
scenario_name: dashboard checkout flow
primary_target: mock-agent[yolo]:mock:ui-implementer
review_targets:
  - mock-agent[yolo]:mock:review-alpha
  - mock-agent[slow]:mock:review-beta
loop_passes: 2
poll_interval: 1s
poll_attempts: 2
step_delay_seconds: 0.1
include_generated_followup: true
```

The same values are checked in at `instantiation-values.yaml`.
For an already-rendered workspace, override the per-node delay at run time with
`MOCK_NODE_DELAY_SECONDS`.

## Validate

```bash
rhei validate examples/ui-test-canonical-example
rhei run examples/ui-test-canonical-example --dry-run
```

## Live UI run

```bash
MOCK_NODE_DELAY_SECONDS=0.5 rhei run examples/ui-test-canonical-example --parallel 4 --dashboard
```

## Regenerate

This example is a pure instantiation of the template plus two checked-in
overrides: this `README.md` (the template renders its own README in its place)
and `instantiation-values.yaml`. To regenerate:

```bash
rm -rf examples/ui-test-canonical-example
rhei instantiate .agents/rhei/templates/ui-test-canonical \
  --values .agents/rhei/templates/ui-test-canonical/.example-values.yaml \
  --output examples/ui-test-canonical-example
cp .agents/rhei/templates/ui-test-canonical/.example-values.yaml \
  examples/ui-test-canonical-example/instantiation-values.yaml
git checkout -- examples/ui-test-canonical-example/README.md
```
