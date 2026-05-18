# Product Management Template

This template creates a directory workspace for a repeated product-management loop. Multiple PM agents independently propose product entries, a stronger agent merges and validates every entry into a bounded implementation slice, and a cheaper implementation agent applies the accepted slice. The default loop runs twice.

## Inputs

| Name | Type | Default | Description |
|---|---|---|---|
| `plan_title` | string | `Product Management Run` | Title for the instantiated workspace. |
| `product_name` | string | required | Product, project, or feature area being managed. |
| `product_brief` | string | required | Product context, customer problem, or strategic goal for every agent. |
| `implementation_scope` | string | `docs/functional-spec, docs/decisions, README.md` | Paths or artifacts the implementation agent may change. |
| `pm_targets` | array<object> | Claude and Codex targets | Agents that independently produce PM entries each pass. Configure this array to add or replace targets. |
| `smart_target` | string | `codex[xhigh]:openai:gpt-5.5` | Agent that aggregates, validates, prioritizes, and slices work. |
| `implementation_target` | string | `codex[medium]:openai:gpt-5.4-mini` | Cheaper agent that implements the accepted slice. |
| `loop_passes` | number | `2` | Number of PM aggregate implementation cycles. |
| `focus_areas` | array<string> | empty | Optional focus areas every PM entry set should address. |
| `validation_criteria` | array<string> | user value, evidence, bounded scope, conflicts | Acceptance criteria for validated entries. |
| `max_entries_per_pass` | number | `3` | Maximum accepted entries implemented in one pass. |

## Task Paths

| Task kind | State path | Purpose |
|---|---|---|
| Product loop | `(product-run -> aggregate-validate -> implement) x loop_passes -> completed` | Fan out PM entries, validate and slice them, then apply the accepted product changes. |

## Flow

1. Instantiate the template with a product name and product brief.
2. The `product-run` state fans out to every configured PM target. Each target writes structured entries for the current pass.
3. The `aggregate-validate` state reads every entry file, deduplicates proposals, validates each entry, rejects or defers weak entries, and writes a bounded implementation slice.
4. The `implement` state applies the accepted slice within the declared implementation scope and writes a pass report.
5. If the loop budget remains, the task returns to `product-run` so the next PM pass can inspect the updated product state. Otherwise the task completes.

The state machine and loop diagram live in [states.yaml](states.yaml).

## Instantiate

```bash
rhei instantiate product-management \
  --set product_name="Rhei" \
  --set-file product_brief=./product-brief.md \
  --output .agents/scratchpad/product-management
```

For structured inputs such as custom targets, use a values file:

```bash
rhei instantiate product-management \
  --values product-management-values.yaml \
  --output .agents/scratchpad/product-management
```

See the pre-rendered example at `examples/product-management-example/`.
