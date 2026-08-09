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

`<RHEI_PLAN_OR_WORKSPACE>` may be a single `.rhei.md` file, a Directory
Workspace root, or a Panta project directory; omitted, the target is resolved by
walking up from the current directory (§FS-rhei-panta.6). A member rhei renders
its project narrowed to that rhei.

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

`--format progress` emits a human-readable progress report, led by a completion
summary: `4/9 tickets done (44%)`, counting every ticket at every depth against
the resolved state machine's **final** states. The summary is omitted — never
guessed — when no state machine resolves, because "done" is a property of the
machine and a custom one need not have a state called `completed`.

Color is enabled only when stdout is a terminal and `NO_COLOR` is unset;
`--no-color` disables color regardless of terminal detection.

### 3.4. Rendering a merged project

A Panta project merges every rhei's tickets into one flat, project-qualified
task list (§AR-rhei-panta.3). The text formats — `github` and `progress` — must
put each ticket back under the rhei that owns it: a run of rhei headings
followed by every rhei's tickets in one undifferentiated list is not a document
a reader can use, and a rhei with no content section of its own leaves a heading
with nothing beneath it.

Each rhei renders as one block: its title as the heading, its own content
sections beneath that (without the `Rhei <id> / ` merge prefix), then its
tickets. A rhei that holds no tickets says so rather than rendering an empty
heading. Manifest-level content sections stay above the blocks, where they
describe the project rather than any one rhei.

A plan that is not a merged project — a single-file plan, a Directory Workspace
loaded on its own — keeps its authored shape: content sections, then one
`## Tasks` chapter.

`--format json` is unaffected: it emits the AST, and the merged section titles
are part of that AST.

## 4. Behavior

1. Load the plan from the file, workspace, or project.
2. Parse it into the Rhei AST defined by the plan language. §FS-rhei-plan-language
3. Narrow to the rhei the target named, when it named one (§FS-rhei-panta.6).
4. Render the parsed plan in the selected format.
5. Print the rendered document to stdout.

`rhei render` does not acquire task locks, run callbacks, spawn agents, spawn
programs, write runtime files, or rewrite plan files.

## Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) - source syntax and AST shape
- [List Command](rhei-list.spec.md) - filtered task inspection
- [Validate Command](rhei-validate.spec.md) - full semantic validation before execution
