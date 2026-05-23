{% raw -%}
# ui-test-canonical

This template creates a runnable directory workspace for exercising the Rhei UI
with deterministic mock execution. Every task is named after the Rhei feature it
tests, so the task list itself reads as a coverage checklist. It covers agent and
program states, artifact contracts, target fan-out, counted loops, polling and
poll exhaustion, live failures, unavailable tooling, snapshots (emit and inherit,
including ancestor inheritance), transition callbacks, generated workspace
expansion, four-level task nesting, dependency blocking, human gates, and
terminal states — without requiring real AI services.

| Input | Type | Default | Description |
|---|---|---|---|
| `plan_title` | string | `Rhei UI Canonical Test` | Workspace title. |
| `scenario_name` | string | `dashboard checkout flow` | Scenario label used in overview text, mock artifacts, and the `\| slug` task id. |
| `primary_target` | string | `mock-agent[yolo]:mock:ui-implementer` | Mock target for implementation and fix-loop states. |
| `review_targets` | array<string> | two mock reviewer targets | Parallel review fan-out targets. |
| `loop_passes` | number | `2` | Counted fix-loop visits before the human gate. |
| `poll_interval` | string | `1s` | Delay between mock poll attempts (validated `^[0-9]+[smh]$`). |
| `poll_attempts` | number | `2` | Poll attempts before readiness / exhaustion. |
| `step_delay_seconds` | number | `0.1` | Sleep inserted into mock agents and scripts for visible live slots. |
| `include_generated_followup` | boolean | `true` | Gates both the runtime callback follow-up and the instantiation-time `followup-preview` task. |

## Task-to-feature coverage matrix

Each task's id and title name the feature it exercises; the body of each task
carries a one-line `Tests:` marker.

| Task | Rhei feature(s) under test |
|---|---|
| `full-pipeline` | agent + program states, artifact input/output contracts (incl. one `optional` input), single `target`, `personality`, snapshot `emit` (`on: success`), `all_targets` fan-out, counted `visits` fix-loop, `on_leave`/`on_enter` callbacks, terminal human gate |
| `full-pipeline.dependency-blocking` | `Prior` dependency edges / ready-set gating |
| `…dependency-blocking.three-level-nesting` | depth-3 nested rendering |
| `…three-level-nesting.four-level-nesting` | `structure.maxLevels: 4` depth-4 rendering |
| `full-pipeline.snapshot-inherit-ancestor` | snapshot `inherit` `from: ancestor` + `select`, `emit on: always` |
| `polling` | `poll` self-loop (`interval`, `max_attempts`), `exit_code: 75` and `exit_code: 0` |
| `poll-exhaustion` | poll exhaustion → `blocked` via `condition: pollAttempts >= pollMaxAttempts` |
| `live-failure-blocked` | live program failure → `blocked`, `exit_code: [1, 2, 42]` array match |
| `skill-unavailable-blocked` | `skill_unavailable` transition |
| `mcp-unavailable-blocked` | `mcp_unavailable` transition |
| `human-gate` | `gating` state + `**Assignee:**` (human-owned, never auto-claimed) |
| `blocked-seeded` | static `blocked` row |
| `terminal-completed` | terminal `completed` + pre-seeded frontmatter `metadata` |
| `terminal-cancelled` | terminal `cancelled` |
| `scenario-<slug>` | `\| slug` instantiation filter |
| `followup-preview` | instantiation-time `{% if %}` and `{% raw %}` blocks |
| `generated-followup-*` (runtime) | callback workspace expansion; `concurrent` `script-check` |

Instantiation templating itself covers `{{ var }}`, `{% for %}` (`review_targets`),
`{% if %}`, `{% raw %}`, the `| slug` filter, and the `string`/`number`/
`boolean`/`array` input types with `default` and `validate`.

## Intentionally out of scope

So the "canonical" claim stays honest, these features are deliberately not
exercised:

- **Legacy execution selectors** `all_models` and bare `model` — superseded by
  `target` / `all_targets`.
- **Non-CLI callback prefixes** `js:` / `py:` / `java:` — this fixture is a
  bash/CLI harness, so only `cli:` callbacks are used.
- **`max_retries` / `retry_delay` and `> **Result:**` links** — not parsed by the
  current engine (Result links are written by the runtime on completion, not
  seeded in the plan).
- **`snapshot.emit on: failure`** — mock agents never fail, so it cannot fire
  deterministically; `on: success` and `on: always` are exercised instead.
- **Multiple `nodeKinds`, `object`/`path` input types, positional inputs** — not
  needed by this fixture; it uses the default `task` kind.

The state machine diagram is in [`states.yaml`](states.yaml). The checked-in
smoke example lives at `examples/ui-test-canonical-example/`.

## Instantiate and run

```bash
rhei instantiate ui-test-canonical \
  --set scenario_name="dashboard checkout flow" \
  --output .agents/scratchpad/ui-test-canonical

rhei run .agents/scratchpad/ui-test-canonical --parallel 4 --dashboard
```

The run intentionally stops with tasks in `human-gate` and `blocked` so the UI
has live gates to render: `full-pipeline` parks at the human gate while the
seeded and live `blocked` tasks (poll exhaustion, live failure, unavailable
skill, unavailable MCP) stay visible for inspection.
{%- endraw %}
