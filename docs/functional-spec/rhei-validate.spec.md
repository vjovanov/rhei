# FS-rhei-validate: `rhei validate`

Validate a Rhei plan or Directory Workspace against the Rhei plan language,
the resolved state machine, project settings, and runtime context checks. The
command is read-only and exists to make execution predictable before a worker
or orchestrator mutates plan state. §GOAL-rhei-outcomes

## 1. Usage

```bash
rhei validate <RHEI_PLAN_OR_WORKSPACE>
rhei validate --watch <RHEI_PLAN_OR_WORKSPACE>
rhei --state-machine <PATH> validate <RHEI_PLAN_OR_WORKSPACE>
```

`<RHEI_PLAN_OR_WORKSPACE>` may be a single `.rhei.md` file or a Directory
Workspace root. When a workspace root is passed, validation loads
`index.rhei.md` and the workspace task files.

## 2. Options

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--watch` | No | false | Re-run validation when the plan or resolved states file changes |
| `--state-machine <PATH>` | No | built-in/default discovery | Global option selecting an explicit states YAML file |

## 3. State Machine Resolution

Validation resolves the state machine in this order:

1. Use `--state-machine <PATH>` when supplied.
2. Use a sibling `states.yaml` for a single-file plan, or
   `<workspace>/states.yaml` for a Directory Workspace, when that file exists
   and its `name` matches the plan's `**States:**` declaration.
3. Fall back to the built-in default state machine when the plan declares the
   built-in machine name.

If a plan declares a non-default state machine name and no matching
auto-discovered file is available, validation fails and directs the caller to
pass `--state-machine`.

## 4. Behavior

1. Load and parse the plan. Single-file validation collects every recoverable
   parse error before returning so users can fix related issues in one pass.
2. Resolve the state machine and validate plan semantics, including state
   values, task ids, dependencies, node policy, terminal and gating states,
   counted-loop syntax, and artifact contracts. §FS-rhei-plan-language
   §FS-rhei-states
3. Load merged global and project settings, then validate referenced agents,
   models, MCP servers, skills, and snapshot settings used by the state
   machine. §FS-rhei-agents §FS-rhei-snapshots
4. Validate snapshot plan context and report orphaned snapshot diagnostics as
   warnings when a snapshot cache exists. §FS-rhei-snapshot-operations
5. Exit non-zero when any validation error remains. Warnings do not make the
   command fail.

`rhei validate` does not acquire task locks, run callbacks, spawn agents,
spawn programs, create runtime files, or rewrite the plan.

## 5. Watch Mode

With `--watch`, the command resolves the same state machine once, prints a
watch-start message, runs an initial validation pass, and then re-runs
validation when the plan file or resolved states file changes.

Watch mode reports each pass independently. A failed pass does not terminate
the watcher; file watcher initialization errors do.

## 6. Output

On success:

```text
Validation succeeded
```

Warnings are printed after the success line:

```text
Validation succeeded
warning: <diagnostic>
```

On failure, diagnostics are emitted through the normal CLI error renderer and
the process exits non-zero.

## Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) - parse and semantic constraints
- [States Specification](rhei-states.spec.md) - state machine format and defaults
- [Agents Specification](rhei-agents.spec.md) - settings and agent/model references
- [Snapshots Specification](rhei-snapshots.spec.md) - snapshot runtime model
- [Snapshot Operations Specification](rhei-snapshot-operations.spec.md) - snapshot CLI and orphan diagnostics
