# Cost Accounting Smoke Example

This is a pre-rendered instantiation of the `cost-accounting-smoke` template.
It verifies that Rhei can display accounting by task and state without calling
an external model. The example replaces `codex` with
`scripts/accounting-smoke-agent.sh`, which writes structured usage records to
the accounting capture path declared by `rhei run`.

## Inputs Used

| Input | Value |
|---|---|
| `plan_title` | `Cost Accounting Smoke Example` |
| `task_title` | `Exercise dashboard accounting` |
| `scenario` | `Example workspace for verifying task/state cost display.` |
| `collect_input_tokens` | `24000` |
| `collect_output_tokens` | `2200` |
| `collect_cached_input_tokens` | `8000` |
| `verify_input_tokens` | `18000` |
| `verify_output_tokens` | `1400` |
| `verify_cached_input_tokens` | `3000` |
| `collect_delay_seconds` | `4` |
| `verify_delay_seconds` | `3` |

## Verify

```bash
rhei validate examples/cost-accounting-smoke-example
```

## Regenerate

```bash
rhei instantiate .agents/rhei/templates/cost-accounting-smoke \
  --set 'plan_title=Cost Accounting Smoke Example' \
  --set 'task_title=Exercise dashboard accounting' \
  --set 'scenario=Example workspace for verifying task/state cost display.' \
  --set collect_input_tokens=24000 \
  --set collect_output_tokens=2200 \
  --set collect_cached_input_tokens=8000 \
  --set verify_input_tokens=18000 \
  --set verify_output_tokens=1400 \
  --set verify_cached_input_tokens=3000 \
  --set collect_delay_seconds=4 \
  --set verify_delay_seconds=3 \
  --output examples/cost-accounting-smoke-example
```

Run the example with:

```bash
rhei run examples/cost-accounting-smoke-example --parallel 1
```
