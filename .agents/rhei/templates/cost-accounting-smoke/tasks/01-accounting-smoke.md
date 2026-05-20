### Task accounting-smoke: {{task_title}}
**State:** collect

{{scenario}}

Expected accounting rows:

| State | Input | Output | Cached input |
|---|---:|---:|---:|
| collect | {{collect_input_tokens}} | {{collect_output_tokens}} | {{collect_cached_input_tokens}} |
| verify | {{verify_input_tokens}} | {{verify_output_tokens}} | {{verify_cached_input_tokens}} |

Expected delay:

| State | Delay |
|---|---:|
| collect | {{collect_delay_seconds}}s |
| verify | {{verify_delay_seconds}}s |
