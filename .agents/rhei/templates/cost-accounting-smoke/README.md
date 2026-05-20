# cost-accounting-smoke

This template creates a local smoke-test workspace for Rhei cost accounting. It
overrides the `codex` agent with a shell script that writes structured usage to
`RHEI_ACCOUNTING_USAGE_PATH`, then runs one task through `collect` and `verify`
states so the dashboard can show per-task and per-state input, output, cached
input, coverage, and cost without contacting an external model.

## Inputs

| Input | Type | Default | Description |
|---|---|---|---|
| `plan_title` | string | `Cost Accounting Smoke` | Title of the instantiated workspace. |
| `task_title` | string | `Exercise task/state accounting` | Title of the smoke task. |
| `scenario` | string | dashboard accounting verification | Task body context. |
| `collect_input_tokens` | number | `12000` | Input tokens emitted by `collect`. |
| `collect_output_tokens` | number | `800` | Output tokens emitted by `collect`. |
| `collect_cached_input_tokens` | number | `4000` | Cached input tokens emitted by `collect`. |
| `verify_input_tokens` | number | `6000` | Input tokens emitted by `verify`. |
| `verify_output_tokens` | number | `500` | Output tokens emitted by `verify`. |
| `verify_cached_input_tokens` | number | `1000` | Cached input tokens emitted by `verify`. |
| `collect_delay_seconds` | number | `4` | Seconds `collect` sleeps before emitting usage. |
| `verify_delay_seconds` | number | `3` | Seconds `verify` sleeps before emitting usage. |

## Task Paths

| Task kind | State path | Purpose |
|---|---|---|
| Smoke task | `collect` -> `verify` -> `completed` | Emits one structured usage record per state. |

## Flow

1. Instantiate the template.
2. Run the workspace with `rhei run`.
3. The template-local `.rhei/settings.json` replaces `codex` with
   `scripts/accounting-smoke-agent.sh`.
4. The `collect` state waits for the configured delay, then emits the configured collect token dimensions.
5. The `verify` state waits for the configured delay, then emits the configured verify token dimensions.
6. Open the generated dashboard and inspect the Cost tab and Tasks tab.

The state machine diagram lives in [states.yaml](states.yaml).

## Instantiate

```bash
rhei instantiate cost-accounting-smoke \
  --set plan_title="Cost Accounting Smoke" \
  --set task_title="Exercise task/state accounting" \
  --output examples/cost-accounting-smoke-example
```

## Example

A pre-rendered example lives at
[`examples/cost-accounting-smoke-example/`](../../../../examples/cost-accounting-smoke-example/)
and passes `rhei validate` as shipped.
