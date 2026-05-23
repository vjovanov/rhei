# ui-test-canonical - example

A pre-rendered instantiation of the
[`ui-test-canonical`](../../.agents/rhei/templates/ui-test-canonical/)
template. This example is a smoke test and a convenient fixture for developing
the Rhei UI against deterministic mock execution.

It intentionally includes runnable work, three-level task nesting, generated
follow-up work, a seeded human gate, a seeded blocked task, and terminal
examples. A live run is expected to stop with non-terminal gated work visible
for inspection.

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

## Validate

```bash
rhei validate examples/ui-test-canonical-example
rhei run examples/ui-test-canonical-example --dry-run
```

## Live UI Run

```bash
rhei run examples/ui-test-canonical-example --parallel 4 --dashboard
```

## Regenerate

```bash
rm -rf examples/ui-test-canonical-example
rhei instantiate .agents/rhei/templates/ui-test-canonical \
  --values .agents/rhei/templates/ui-test-canonical/.example-values.yaml \
  --output examples/ui-test-canonical-example
```

After regenerating, restore this README and the checked-in
`instantiation-values.yaml` if the generator overwrote them.
