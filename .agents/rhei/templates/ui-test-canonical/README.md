# ui-test-canonical

This template creates a runnable directory workspace for exercising the Rhei UI with deterministic mock execution. It covers three-level nested tasks, dependency blocking, agent and program states, artifact inputs and outputs, target fan-out, counted loops, polling, callbacks, generated workspace expansion, snapshot emit/inherit declarations, human gates, and terminal states without requiring real AI services.

| Input | Type | Default | Description |
|---|---|---|---|
| `plan_title` | string | `Rhei UI Canonical Test` | Workspace title. |
| `scenario_name` | string | `dashboard checkout flow` | Scenario label used in overview text and mock artifacts. |
| `primary_target` | string | `mock-agent[yolo]:mock:ui-implementer` | Mock target for implementation and fix-loop states. |
| `review_targets` | array<string> | two mock reviewer targets | Parallel review fan-out targets. |
| `loop_passes` | number | `2` | Counted fix-loop visits before the human gate. |
| `poll_interval` | string | `1s` | Delay between mock poll attempts. |
| `poll_attempts` | number | `2` | Poll attempts before readiness. |
| `step_delay_seconds` | number | `0.1` | Sleep inserted into mock agents and scripts for visible live slots. |
| `include_generated_followup` | boolean | `true` | Enables one callback-generated follow-up task. |

| Task Kind | Path Through State Machine |
|---|---|
| Main task or normal subtask | `collect-inputs -> script-normalize -> mock-implement -> script-build -> parallel-review -> aggregate -> fix-loop x N -> human-gate` |
| Polling task | `script-poll -> script-poll -> script-check -> completed` |
| Smoke check task | `script-check -> completed` |
| Seeded gate | `human-gate` until a human transitions it to `completed` or `cancelled` |
| Seeded terminal examples | already in `completed` or `cancelled` |
| Generated follow-up | appended by the `aggregate -> fix-loop` callback, then `script-check -> completed` |

Narrative flow:

1. Mock intake agents write raw input artifacts for the scenario.
2. Mock programs normalize those inputs and create program logs.
3. A mock implementation agent writes implementation artifacts and emits a snapshot.
4. A build script consumes implementation output and writes build artifacts.
5. Two mock review targets run in parallel and write per-target findings.
6. A deterministic aggregate script merges findings and a transition callback may append a follow-up task file to the workspace.
7. A counted fix-loop agent inherits the implementation snapshot when available and writes one fix artifact per visit.
8. Finished work lands in `human-gate` so the live UI shows an operator stop point; seeded checks and terminal examples cover the rest of the surface.

The state machine diagram is in [`states.yaml`](states.yaml). The checked-in smoke example lives at `examples/ui-test-canonical-example/`.

Instantiate with representative defaults:

```bash
rhei instantiate ui-test-canonical \
  --set scenario_name="dashboard checkout flow" \
  --output .agents/scratchpad/ui-test-canonical
```

Run it with the live UI:

```bash
rhei run .agents/scratchpad/ui-test-canonical --parallel 4 --dashboard
```

The run intentionally stops with tasks in `human-gate` and `blocked` so the UI has live gates to render.
