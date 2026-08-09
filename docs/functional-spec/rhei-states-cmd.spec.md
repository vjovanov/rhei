# FS-rhei-states-cmd: `rhei states`

Print the resolved state machine so humans and scripts can inspect available
states, transitions, profiles, node policy, artifacts, agents, programs, and
snapshot configuration before executing a plan. §GOAL-rhei-outcomes

## 1. Usage

```bash
rhei states
rhei states <RHEI_PLAN>
rhei states --json
rhei --state-machine <PATH> states
```

`rhei states` reports the machine the target plan actually runs under, resolved
exactly as every other command resolves it (§FS-rhei-plan-language.1.3). An
omitted `<RHEI_PLAN>` resolves to the nearest enclosing project, workspace, or
lone plan, matching `rhei list` and `rhei run`. A command that reported the
built-in default while the project ran a declared machine would misdescribe
every state name the author is about to type.

## 2. Options

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--json` | No | false | Emit the state machine as JSON instead of human-readable text |
| `--state-machine <PATH>` | No | resolved from the plan | Global option selecting an explicit states YAML file |

## 3. Behavior

1. Load the explicit states YAML file when `--state-machine` is supplied.
   §FS-rhei-states
2. Otherwise resolve the target plan and load the machine its `**States:**`
   declaration selects, including a declaration inherited from
   `index.panta.md`. §FS-rhei-plan-language.1.3
3. Fall back to the built-in default state machine when no plan or project
   resolves — there is then nothing to declare a machine.
4. Render a complete inspection view of the machine.
5. Print the result to stdout.

Discovery failures are non-fatal only when the target was inferred: an
auto-discovered plan that fails to load reports a warning on stderr and prints
the built-in default, so the command stays usable while the author is repairing
that very plan. An explicitly named plan propagates its load error.

The command is read-only. It does not validate task state, run callbacks, spawn
agents, spawn programs, or write runtime files.

### 3.1. Source Line

Text output opens with a `Source:` line naming the resolved states file, or
`the built-in default state machine` when no file backs it. The resolution
rules have several outcomes and the rendered machine alone does not distinguish
them.

## 4. Text Output

Text output includes:

- State machine name and version.
- Model list when present.
- Profile initial states and allowed state sets.
- Node policy when present.
- Each state with description and flags such as `final`, `gating`, and
  `concurrent`.
- Per-state execution details such as visits, polling, targets, models, agent,
  agent mode, timeouts, program presence, MCP servers, skills, snapshots,
  inputs, outputs, personality, and instructions.
- Declared transitions and annotations for callbacks, conditions, and timeouts.

## 5. JSON Output

`--json` emits a pretty JSON object with stable top-level fields:

```json
{
  "name": "default",
  "models": [],
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
