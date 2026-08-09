# FS-rhei-validate: `rhei validate`

Validate a Rhei plan or Directory Workspace against the Rhei plan language,
the resolved state machine, project settings, and runtime context checks. The
command is read-only and exists to make execution predictable before a worker
or orchestrator mutates plan state. §GOAL-rhei-outcomes

## 1. Usage

```bash
rhei validate [RHEI_PLAN_OR_WORKSPACE]
rhei validate --watch [RHEI_PLAN_OR_WORKSPACE]
rhei --state-machine <PATH> validate [RHEI_PLAN_OR_WORKSPACE]
```

`<RHEI_PLAN_OR_WORKSPACE>` may be a single `.rhei.md` file, a Directory
Workspace root, or a Panta project directory; omitted, the target is resolved
by walking up from the current directory (§FS-rhei-panta.6). When a workspace
root is passed, validation loads `index.rhei.md` and the workspace task files.
A project target validates the whole merged graph.

### 1.1. Why there is no `--rhei`

Unlike `rhei list`, `rhei run`, and `rhei reset`, `rhei validate` takes no
`--rhei` narrowing flag. Narrowing those commands selects *tickets to act on*,
which is well defined. Narrowing validation would have to select *diagnostics
to report*, and a project's diagnostics are not partitioned by rhei: the state
machine, merged settings, link bases, and cross-rhei `**Prior:**` resolution
are all project-wide, and a load failure in one rhei is what stops the others
from resolving. A flag that filtered the reported subset would hide real
errors behind an apparently narrower green — the opposite of what validation
is for. Validate the project; the diagnostics name their own rhei.

## 2. Options

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--watch` | No | false | Re-run validation when the plan or resolved states file changes |
| `--state-machine <PATH>` | No | built-in/default discovery | Global option selecting an explicit states YAML file |

## 3. State Machine Resolution

Validation uses the state-machine resolution order defined in the
[Plan Language Specification](rhei-plan-language.spec.md#13-state-machine-resolution):
explicit `--state-machine <PATH>` first, rhei-local `**States:**` declarations,
Panta default inheritance for rheis that omit `**States:**`, omitted effective
declarations as the built-in `rhei` machine, declared `**States:** rhei` with
built-in fallback, and declared custom names only when a matching
auto-discovered file is available.

If a plan declares a non-default state machine name and no matching
auto-discovered file is available, validation fails and directs the caller to
pass `--state-machine`.

## 4. Behavior

1. Load and parse the plan. Single-file validation and Directory Workspace task
   file validation collect every recoverable parse error before returning so
   users can fix related issues in one pass. Workspace index parse errors remain
   fail-fast because later task-file diagnostics may depend on index structure.
2. Resolve the state machine and validate plan semantics, including state
   values, task ids, dependencies, node policy, terminal and gating states,
   counted-loop syntax, and artifact contracts. §FS-rhei-plan-language §FS-rhei-states
3. Load merged global and project settings, then validate referenced agents,
   models, MCP servers, skills, and snapshot settings used by the state
   machine. §FS-rhei-agents §FS-rhei-snapshots
4. Validate snapshot plan context and report orphaned snapshot diagnostics as
   warnings when a snapshot cache exists. §FS-rhei-snapshot-operations
5. Report every ticket that reached a successful terminal state while one of
   its `**Prior:**` dependencies is still unsatisfied as a **warning** naming
   the ticket and each blocking prior with its state. Such a plan contradicts
   the dependency semantics it declares, and no other surface reveals it: a
   terminal ticket drops out of `rhei list --blocked` and out of readiness
   entirely, so the plan reads as healthy. The condition is reachable through
   the deliberate `rhei transition` escape hatch
   (§FS-rhei-transition-cmd.3), by editing a `**Prior:**` onto an
   already-completed ticket, and by a prior that was later cancelled — all
   legitimate authoring moves. It is therefore a warning, never an error:
   validation must surface the inconsistency without making an existing plan
   unloadable.
6. Exit non-zero when any validation error remains. Warnings do not make the
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
