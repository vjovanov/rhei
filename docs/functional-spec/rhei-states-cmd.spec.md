# FS-rhei-states-cmd: `rhei states`

Print the resolved state machine so humans and scripts can inspect available
states, transitions, profiles, node policy, artifacts, agents, programs, and
snapshot configuration before executing a plan. §GOAL-rhei-outcomes

## 1. Usage

```bash
rhei states
rhei states --json
rhei --state-machine <PATH> states
```

`rhei states` does not take a plan argument. Without `--state-machine`, it
prints the built-in default state machine.

## 2. Options

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--json` | No | false | Emit the state machine as JSON instead of human-readable text |
| `--state-machine <PATH>` | No | built-in default | Global option selecting an explicit states YAML file |

## 3. Behavior

1. Load the explicit states YAML file when `--state-machine` is supplied;
   otherwise load the built-in default state machine. §FS-rhei-states
2. Render a complete inspection view of the machine.
3. Print the result to stdout.

The command is read-only. It does not load a plan, validate task state, run
callbacks, spawn agents, spawn programs, or write runtime files.

## 4. Text Output

Text output includes:

- State machine name and version.
- Model list when present.
- Prompt-template list when present, including which prompt fields each
  template defines.
- Profile initial states and allowed state sets.
- Node policy when present.
- Each state with description and flags such as `final`, `gating`, and
  `concurrent`.
- Per-state execution details such as visits, polling, targets, models, agent,
  agent mode, timeouts, program presence, MCP servers, skills, snapshots,
  inputs, outputs, personality, and instructions.
- Per-state prompt-template reference when present.
- Declared transitions and annotations for callbacks, conditions, and timeouts.

## 5. JSON Output

`--json` emits a pretty JSON object with stable top-level fields:

```json
{
  "name": "default",
  "models": [],
  "prompt_templates": {},
  "profiles": null,
  "node_policy": null,
  "version": 1,
  "states": [],
  "transitions": []
}
```

When JSON output is selected, command errors are rendered as a single JSON
object on stderr.

## Related Specifications

- [States Specification](rhei-states.spec.md) - state machine schema and defaults
- [Transitions Specification](rhei-transitions.spec.md) - transition schema and callbacks
- [Agents Specification](rhei-agents.spec.md) - agent/model execution fields
- [Programs Specification](rhei-programs.spec.md) - deterministic program states
- [Snapshots Specification](rhei-snapshots.spec.md) - snapshot state fields
