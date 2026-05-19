# FS-rhei-completions: Rhei Completion UX Specification

Generate shell completion scripts for the `rhei` CLI. The command writes the script to stdout so users can either source it in the current shell or install it into their shell's normal completion directory.

The generated scripts are dynamic: the shell calls back into the installed
`rhei` binary at completion time. This keeps completions in sync with the
current working directory, including discovered template names for
`rhei instantiate <TEMPLATE>` and template-specific input arguments.

## UX Goal

Every meaningful command-line slot should complete. A user should be able to
start with `rhei <TAB>` and discover the command tree, then keep pressing
`<TAB>` through required positionals, option values, template inputs, task ids,
state names, shell names, agent names, and filesystem paths without consulting
documentation.

Completions are advisory and must never change command semantics. If completion
cannot load dynamic context, the CLI still runs normally and static completions
continue to work.

## 1. Usage

```bash
rhei completions <SHELL>
rhei completions <SHELL> --install
rhei completions <SHELL> --install --system
rhei completions <SHELL> --install --output <PATH>
rhei completions bash
rhei completions zsh
rhei completions fish
rhei completions powershell
rhei completions elvish
```

## 2. Arguments

| Argument | Description |
|----------|-------------|
| `<SHELL>` | Target shell. Supported values: `bash`, `zsh`, `fish`, `powershell`, `elvish` |

## 3. Options

| Flag | Default | Description |
|------|---------|-------------|
| `--install` | | Write the generated completion script to the shell's normal completion location instead of stdout |
| `--user` | user | Install into the current user's shell configuration directories |
| `--system` | | Install into system-wide completion directories, intended for package manager post-install scripts |
| `--output <PATH>` | | Write to an explicit path instead of the default user or system path |
| `--dry-run` | | Print the destination path without writing files |

## 4. Supported Shells

Rhei supports the notable interactive shells covered by `clap_complete`:

| Shell | Value | User install path | System install path |
|-------|-------|-------------------|---------------------|
| Bash | `bash` | `${XDG_DATA_HOME:-~/.local/share}/bash-completion/completions/rhei` | `/usr/local/share/bash-completion/completions/rhei` |
| Zsh | `zsh` | `~/.zfunc/_rhei` | `/usr/local/share/zsh/site-functions/_rhei` |
| Fish | `fish` | `${XDG_CONFIG_HOME:-~/.config}/fish/completions/rhei.fish` | `/usr/local/share/fish/vendor_completions.d/rhei.fish` |
| PowerShell | `powershell` | `${XDG_CONFIG_HOME:-~/.config}/powershell/rhei-completions.ps1` | `/usr/local/share/powershell/Completions/rhei-completions.ps1` |
| Elvish | `elvish` | `${XDG_CONFIG_HOME:-~/.config}/elvish/lib/rhei-completions.elv` | `/usr/local/share/elvish/lib/rhei-completions.elv` |

## 5. Output Contract

- The command prints only the completion script to stdout.
- Diagnostics and parse errors are written to stderr through the normal CLI error path.
- Unless `--install` or `--output` is passed, the command does not create, modify, or remove files.
- With `--install`, the command creates parent directories as needed, overwrites the target completion file atomically enough for normal CLI use, and prints the installed path to stdout.
- Generated completions call back into the current binary through the `COMPLETE` environment variable and reflect the current binary's command tree, global options, subcommands, flags, value enums, and supported dynamic argument values.
- `rhei instantiate <TEMPLATE>` completes discovered project and user template names using the same precedence as `rhei templates`: `<project>/.agents/rhei/templates/<name>/` first, then `~/.agents/rhei/templates/<name>/`. If the partially typed value is path-like (absolute, dot-prefixed, or contains `/`), directory completion is used instead.

## 6. Completion Contract

Completion is part of the CLI UX, not a best-effort extra. The generated scripts
must expose the same completion behavior across Bash, Zsh, Fish, PowerShell, and
Elvish as far as each shell's completion model permits.

### 6.1. Global Rules

- Complete subcommands at every command position.
- Complete every flag and option, including global flags such as
  `--state-machine`.
- Complete every value enum from the same source of truth as argument parsing.
- Complete every filesystem path argument with a kind-specific path completer.
- Complete dynamic values from local project state when enough prior arguments
  are available to resolve them.
- Filter candidates by the current token prefix.
- Preserve user-typed quoting and escaping through the shell completion engine.
- Do not print human diagnostics to stdout during dynamic completion.
- Do not write files, acquire task locks, run callbacks, spawn agents, spawn
  programs, or mutate metadata during completion.
- Degrade quietly. If a plan, workspace, state machine, settings file, or
  template manifest cannot be read, return the best static/path completions that
  remain valid.

### 6.2. Candidate Display

When the shell supports descriptions, candidates should include concise help:

| Candidate | Help content |
|-----------|--------------|
| Subcommand | The command summary from the CLI definition |
| Flag / option | The option help from the CLI definition |
| Template | Template description and source (`project` or `user`) |
| Template input key | Type, required/default status, positional index when present, and description |
| Task id | Task title and current state |
| State name | State description when available |
| Assignee | Number of matching tasks when available |
| Node kind | Number of matching tasks when available |
| Agent / mode / model | Resolved provider/config description when available |
| Filesystem path | Shell-native path display |

Required values should sort before optional values. Values in the project
should sort before values from user/global configuration. Already supplied
non-repeatable values should be hidden unless the current token is editing that
same value.

### 6.3. Filesystem Values

Filesystem completion should match the argument's domain:

| Argument | Completion behavior |
|----------|---------------------|
| `RHEI_PLAN` | Complete `.rhei.md` files and workspace directories containing `index.rhei.md`; plain directories remain traversable |
| `--state-machine` | Complete `.yaml` and `.yml` files; directories remain traversable |
| `--values` | Complete `.yaml`, `.yml`, and `.json` files; directories remain traversable |
| `--output` | Complete directories and allow new leaf paths |
| `completions --output` | Complete files and allow new leaf paths |
| `--set-file KEY=PATH` | Complete files after the `=`; directories remain traversable |
| Template `type: path` values | Complete files and directories after the input key or positional slot |
| `reset RHEI_PLAN` | Complete `.rhei.md` files and workspace directories |

Path-like template references are completed as directories. A template reference
is path-like when it is absolute, dot-prefixed, or contains `/`.

### 6.4. Dynamic Context

Dynamic completion may inspect only local files and configuration:

- The current working directory and its `.agents/rhei/` tree.
- User Rhei configuration under the documented user config directories.
- The plan/workspace argument already present on the command line.
- The selected template's `template.yaml`.
- The selected or auto-discovered state machine.

Completion must not use the network. It must not execute template output,
program states, state callbacks, or agent commands.

## 7. Command Coverage

All command arguments and option values should complete as follows.

| Command | Argument / option | Completion source |
|---------|-------------------|-------------------|
| global | `--state-machine` | YAML file path completion |
| `validate` | `RHEI_PLAN` | Rhei plan/workspace path completion |
| `render` | `RHEI_PLAN` | Rhei plan/workspace path completion |
| `render` | `--format` | `json`, `github`, `progress` |
| `states` | `--json` | Static flag completion |
| `list` | `RHEI_PLAN` | Rhei plan/workspace path completion |
| `list` | `--state` | Comma-aware state name completion from the resolved state machine |
| `list` | `--assignee` | Assignee values present in the selected plan/workspace |
| `list` | `--kind` | Node kinds present in the selected plan/workspace |
| `list` | `--has-prior` | Task ids from the selected plan/workspace |
| `list` | `--parent` | Task ids from the selected plan/workspace |
| `list` | `--contains` | No fixed candidates; shell should preserve free text |
| `list` | `--limit` | Small integer suggestions (`10`, `25`, `50`, `100`, `0`) |
| `templates` | `--source` | `all`, `project`, `user` |
| `instantiate` | `TEMPLATE` | Discovered template names or directory paths |
| `instantiate` | `[input ...]` | Template-specific positional and `KEY=VALUE` completion |
| `instantiate` | `--set` | Template input `KEY=VALUE` completion |
| `instantiate` | `--set-file` | Template input `KEY=PATH` completion |
| `instantiate` | `--values` | YAML/JSON file path completion |
| `instantiate` | `--output` | Output path completion |
| `run` | `RHEI_PLAN` | Rhei plan/workspace path completion |
| `run` | `--parallel` | Small integer suggestions (`1`, `2`, `4`, `8`, `0`) |
| `run` | `--agent` | Agent names from resolved settings/state machine |
| `run` | `--agent-mode` | Modes for the selected/resolved agent |
| `run` | `--model` | Model aliases from resolved settings/state machine |
| `run` | `--program-timeout` | Duration examples (`30s`, `1m`, `5m`, `15m`, `1h`) |
| `next` | `RHEI_PLAN` | Rhei plan/workspace path completion |
| `next` | `--task` | Task ids from the selected plan/workspace |
| `complete` | `RHEI_PLAN` | Rhei plan/workspace path completion |
| `complete` | `--task` | Task ids from the selected plan/workspace |
| `complete` | `--result` | No fixed candidates; shell should preserve free text |
| `transition` | `RHEI_PLAN` | Rhei plan/workspace path completion |
| `transition` | `--task` | Task ids from the selected plan/workspace |
| `transition` | `--from` | Current task state when `--task` is known; otherwise state names |
| `transition` | `--to` | Allowed target states from `--from`; when `--task` is known and `--from` is omitted, allowed target states from the task's current state |
| `reset` | `RHEI_PLAN` | Rhei plan/workspace path completion |
| `install-skills` | `--agent` | `claude-code`, `cursor`, `windsurf`, `copilot`, `kilocode`, `pi`, `codex`, `antigravity`, `all` |
| `install-skills` | `--skills` | Comma-aware skill name completion |
| `completions` | `SHELL` | `bash`, `zsh`, `fish`, `powershell`, `elvish` |
| `completions` | `--output` | File path completion |

Boolean flags complete as flags only; they do not take `true` / `false` values.

## 8. Template Input Completion

`rhei instantiate` has the richest completion behavior because the valid
arguments depend on the selected template. Completion must parse the command
line to identify:

1. The selected template name or path.
2. The template manifest inputs.
3. Inputs already supplied by positional values, bare `KEY=VALUE`, `--set`,
   `--set-file`, and `--values`.
4. The cursor token currently being edited.

The parser used for completion should follow the same rules as instantiation:
manifest defaults < `--values` files < positional input values < bare
`KEY=VALUE` and `--set` < `--set-file`.

### 8.1. Template Name Slot

Before a template is selected:

- Complete project templates first, then user templates.
- Show the template description as candidate help.
- If a project and user template share a name, show only the project template.
- If the current token is path-like, complete directories instead of template
  names.

### 8.2. Positional Slots

After a template is selected, a bare argument without `=` completes the next
available positional input.

Given:

```yaml
inputs:
  - name: spec
    type: path
    positional: 1
  - name: criteria
    type: string
    required: false
```

the cursor in this command completes filesystem paths for `spec`:

```bash
rhei instantiate spec-review <TAB>
```

After the first positional value is present, completion should stop suggesting a
second bare positional unless the manifest declares `positional: 2`:

```bash
rhei instantiate spec-review docs/functional-spec/rhei-plan-language.spec.md <TAB>
```

At that point candidates should be remaining input assignments such as
`criteria=`, plus normal flags.

If a template declares exactly one required input and no `positional` fields,
completion treats the first bare value as that input. This matches the
single-required-input fallback in the template spec.

If completion cannot determine a valid positional slot, it should suggest
remaining `KEY=` assignments rather than inventing a positional meaning.

### 8.3. Assignment Keys

At any input position after the template name, completion should suggest
remaining input assignment keys as `KEY=`:

```bash
rhei instantiate code-review <TAB>
# target=       review_passes=       model=
```

Assignments are available both as bare input arguments and as `--set` values:

```bash
rhei instantiate code-review target=src <TAB>
rhei instantiate code-review --set <TAB>
```

Candidate help should include:

```text
path, required, positional 1 - File or directory to review
number, default 2 - Number of review iterations
string, default claude - Model to use for review
```

Inputs already supplied by an explicit assignment should be hidden from the
normal suggestion list. They may reappear when the current token is editing the
same input because overriding an input remains legal.

### 8.4. Assignment Values

When the cursor is after `KEY=`, complete the value according to the input type:

| Input type | Value completion |
|------------|------------------|
| `path` | File and directory paths |
| `boolean` | `true`, `false` |
| `number` | No fixed values unless the input declares examples in a future manifest extension |
| `string` | No fixed values unless the input declares examples in a future manifest extension |
| `array` | Snippets `[]`, `[item]` where supported; otherwise no fixed values |
| `object` | Snippets `{}` where supported; otherwise no fixed values |

Examples:

```bash
rhei instantiate spec-review spec=<TAB>          # path completion
rhei instantiate foo enabled=<TAB>               # true / false
rhei instantiate foo --set target=<TAB>          # path completion when target is type:path
rhei instantiate foo --set-file prompt=<TAB>     # file path completion
```

For `--set-file KEY=PATH`, the key completes like `--set`, but the value always
uses file path completion regardless of the input type because the value is read
from disk.

### 8.5. Values Files

When one or more `--values` files are present, completion may parse readable
YAML/JSON files to suppress already-supplied input keys. Failure to parse a
values file must not fail completion; it only means those values are ignored for
completion ranking.

`--values <TAB>` completes `.yaml`, `.yml`, and `.json` files.

### 8.6. List Inputs Mode

`--list-inputs` does not require user-supplied required inputs. Completion after
`--list-inputs` should still offer flags such as `--values` and `--output` only
when they remain syntactically valid, but it should not pressure the user to
fill required template inputs.

### 8.7. Unknown Or Invalid Templates

If the template cannot be found or its manifest cannot be parsed, completion
should still offer:

- Static flags and options for `rhei instantiate`.
- Path completion for path-like template references.
- No template-specific input candidates.

The shell must not display parse errors as completion candidates.

## 9. Task And State Completion

Commands that inspect or operate on tasks should complete task and plan-derived
values after a plan/workspace argument is present:

```bash
rhei list plan.rhei.md --has-prior <TAB>
rhei list plan.rhei.md --state dra<TAB>
rhei next plan.rhei.md --task <TAB>
rhei complete workspace/ --task <TAB>
rhei transition workspace/ --task <TAB>
```

Task candidates use the task id as the inserted value. Help should include the
task title and current state:

```text
1       Write initial draft [draft]
auth    Review auth module [review]
```

`rhei transition --from` and `--to` should use the resolved state machine:

- `--from` completes the selected task's current state when `--task` is known.
- Without a known task, `--from` completes all state names.
- `--to` completes allowed target states from the selected `--from`.
- If `--from` is omitted but `--task` is known, `--to` completes allowed target
  states from the task's current state.
- State candidate help should include state descriptions when available.

Completion must not claim a task, alter `**Assignee:**`, run callbacks, or
evaluate transition conditions with side effects.

`rhei list` filter completion should use values that actually occur in the
selected plan/workspace when possible:

- `--state` completes state names and aliases from the resolved state machine.
  The option is comma-aware and replaces only the segment after the last comma.
- `--assignee` completes distinct `**Assignee:**` values present in the plan.
- `--kind` completes distinct task/node kinds present in the plan.
- `--has-prior` and `--parent` complete task ids.
- `--contains` remains free text.
- `--limit` completes useful numeric examples but accepts any non-negative
  integer.

## 10. Agent And Skill Completion

`rhei run` should complete agent-related overrides from the same merged settings
and state-machine resolution used by execution:

- `--agent` completes configured agent names.
- `--agent-mode` completes modes for the selected agent when `--agent` is
  present; otherwise it completes the union of known modes.
- `--model` completes configured model aliases and model ids when available.

`rhei install-skills --skills` is comma-aware:

```bash
rhei install-skills --skills rhei-plan-<TAB>
rhei install-skills --skills rhei-plan-worker,rhei-<TAB>
```

Completion should replace only the segment after the last comma and should not
duplicate skills already present in the comma-separated list.

## 11. Performance And Reliability

Completion should feel instant. Implementations should prefer shallow,
targeted reads over full validation:

- Template name completion reads only template manifests.
- Template input completion reads only the selected manifest and optionally
  supplied values files.
- Task completion parses the selected plan/workspace but does not run full
  validation unless validation data is already needed for state completion.
- State completion loads the selected or auto-discovered state machine.

Completion should avoid unbounded directory walks. Project/user template roots
and workspace task directories may be scanned; arbitrary recursive scans should
be avoided unless the shell's path completer owns the traversal.

## 12. Acceptance Tests

The completion test suite should cover at least:

- Every subcommand appears at `rhei <TAB>`.
- Every value enum completes from the parser's enum source.
- `RHEI_PLAN`, `--state-machine`, `--values`, `--output`, and `--set-file`
  path completion use the expected path domain.
- `instantiate` completes project/user template names with project precedence.
- `instantiate` completes positional `type: path` inputs as paths.
- `instantiate` completes remaining input keys as `KEY=`.
- `instantiate` completes `KEY=` right-hand sides by input type.
- `instantiate --set` and bare `KEY=VALUE` share key/value completion behavior.
- `instantiate --set-file` completes keys and then file paths.
- Single-required-input fallback completes the first bare value using that
  input's type.
- Already supplied input keys are not suggested again unless editing that token.
- `next`, `complete`, and `transition` complete task ids from a plan/workspace.
- `transition --from` and `--to` complete state names from the resolved state
  machine.
- `install-skills --skills` completes comma-separated skill segments.
- Invalid manifests, unreadable plans, and unreadable values files degrade
  without stdout diagnostics.

## 13. System Installation

`cargo install` only installs the `rhei` binary. It cannot install shell completion files by itself.

System packages, distro packages, Homebrew formulae, and local install scripts should install the binary first, then run one completion install command per supported shell:

```bash
rhei completions bash --install --system
rhei completions zsh --install --system
rhei completions fish --install --system
rhei completions powershell --install --system
rhei completions elvish --install --system
```

PowerShell and Elvish system paths are provided for package scripts that manage those runtimes, but users may still need to source the generated file from their profile depending on their local shell configuration.

## Examples

Install Fish completions:

```bash
mkdir -p ~/.config/fish/completions
rhei completions fish > ~/.config/fish/completions/rhei.fish
```

Install Fish completions to the default user path:

```bash
rhei completions fish --install
```

Generate Zsh completions into a local completion directory:

```bash
mkdir -p ~/.zfunc
rhei completions zsh > ~/.zfunc/_rhei
```

Preview Bash completions without installing them:

```bash
rhei completions bash
```

## Non-Goals

- `rhei completions` does not edit shell rc files, profiles, or startup scripts.
- It does not generate completions for external agent commands spawned by `rhei run`.

## Related Specifications

- [Templates Specification](rhei-templates.spec.md) — Template input syntax,
  positional inputs, and instantiation precedence.
- [List Command](rhei-list.spec.md) — Task listing filters that provide dynamic
  completion sources.
- [States Specification](rhei-states.spec.md) — State names, aliases, terminal
  states, and transition metadata.
- [Agents Specification](rhei-agents.spec.md) — Agent, mode, model, and settings
  resolution.
