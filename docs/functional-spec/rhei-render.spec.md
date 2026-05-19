# FS-rhei-render: `rhei render`

Render a Rhei plan or Directory Workspace into a selected read-only output
format. Rendering is for inspection, export, and scripting; it does not validate
state-machine reachability beyond the parse/load step and does not modify
runtime state. §GOAL-rhei-outcomes

## 1. Usage

```bash
rhei render <RHEI_PLAN_OR_WORKSPACE> --format json
rhei render <RHEI_PLAN_OR_WORKSPACE> --format json --pretty
rhei render <RHEI_PLAN_OR_WORKSPACE> --format github --no-metadata --no-content
rhei render <RHEI_PLAN_OR_WORKSPACE> --format progress --no-color
```

`<RHEI_PLAN_OR_WORKSPACE>` may be a single `.rhei.md` file or a Directory
Workspace root.

## 2. Options

| Flag | Required | Applies to | Description |
|------|----------|------------|-------------|
| `--format <FORMAT>` | Yes | all | Output format: `json`, `github`, or `progress` |
| `--pretty` | No | `json` | Pretty-print JSON instead of compact JSON |
| `--no-color` | No | `progress` | Disable ANSI color in progress output |
| `--no-metadata` | No | `github` | Omit metadata in GitHub Markdown output |
| `--no-content` | No | `github` | Omit subtask content in GitHub Markdown output |

## 3. Formats

### 3.1. JSON

`--format json` emits the parsed plan AST as JSON. Compact JSON is the default;
`--pretty` emits indented JSON for human inspection.

When JSON format is selected, command errors are rendered as a single JSON
object on stderr so machine consumers do not need to parse two diagnostic
shapes.

### 3.2. GitHub Markdown

`--format github` emits Markdown suitable for GitHub issue-style review. By
default it includes plan metadata and subtask content. `--no-metadata` and
`--no-content` independently remove those sections.

### 3.3. Progress

`--format progress` emits a human-readable progress report. Color is enabled
only when stdout is a terminal and `NO_COLOR` is unset; `--no-color` disables
color regardless of terminal detection.

## 4. Behavior

1. Load the plan from the file or workspace.
2. Parse it into the Rhei AST defined by the plan language. §FS-rhei-plan-language
3. Render the parsed plan in the selected format.
4. Print the rendered document to stdout.

`rhei render` does not acquire task locks, run callbacks, spawn agents, spawn
programs, write runtime files, or rewrite plan files.

## Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) - source syntax and AST shape
- [List Command](rhei-list.spec.md) - filtered task inspection
- [Validate Command](rhei-validate.spec.md) - full semantic validation before execution
