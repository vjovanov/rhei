### Task accounting-smoke: Exercise dashboard accounting
**State:** collect

Example workspace for verifying task/state cost display.

Expected accounting rows:

| State | Input | Output | Cached input |
|---|---:|---:|---:|
| collect | 24000 | 2200 | 8000 |
| verify | 18000 | 1400 | 3000 |

Expected delay:

| State | Delay |
|---|---:|
| collect | 4s |
| verify | 3s |
