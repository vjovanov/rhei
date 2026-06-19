# FS-rhei-templates: Rhei Templates Specification

This document specifies **Rhei Templates** — parameterized, reusable plan+state-machine bundles that can be instantiated with concrete inputs to produce ready-to-execute workspaces.

## Motivation

Rhei plans and state machines today are authored from scratch or copied manually. Common patterns — code review loops, release checklists, onboarding workflows — are recreated each time. Templates let users capture a proven workflow once and instantiate it with project-specific inputs, removing boilerplate and enforcing consistency.

## Concepts

A **template** is a directory containing:

1. A **manifest** (`template.yaml`) declaring the template's name, description, and typed input parameters.
2. A **state machine** (`states.yaml`) — optional; when absent, instantiation and validation fall back to the built-in `rhei` default.
3. **Prompt templates** (`prompt_templates/*.md`) — optional; when present next
   to `states.yaml`, the directory declares reusable state prompt fragments
   referenced by state-level `prompt_template` fields.
4. A **plan skeleton** — either a single-file plan (`plan.rhei.md`) or a directory-workspace layout (`index.rhei.md` + `tasks/`).
5. **Additional files** — any non-manifest files bundled with the template. Text files are rendered with a restricted MiniJinja template environment; binary files are copied verbatim into the output.

Materialized template files may contain **instantiation templates** (`{{ ... }}`, `{% ... %}`) that are resolved at instantiation time from user-supplied inputs. These are distinct from the single-brace **runtime variables** (`{name}`) defined by the state-machine and plan specifications. Rendering is ordered: first resolve MiniJinja templates across template files during `rhei instantiate`, then let later `rhei` commands resolve runtime `{...}` variables against the instantiated workspace. The manifest (`template.yaml`) is parsed before rendering and is never itself templated.

State-machine prompt templates are runtime prompt fragments, not instantiation
templates. When a template bundles `prompt_templates/*.md` next to
`states.yaml`, their single-brace placeholders are preserved through
`rhei instantiate`, substituted from each state's `prompt_template.values`, and
any runtime variables inside those values are then resolved during `rhei next`
or `rhei run`. Inline state `personality` and `instructions` remain valid.

## 1. Template Discovery

Templates are resolved in order, first match wins:

| Priority | Location | Scope |
|----------|----------|-------|
| 1 | `<project>/.agents/rhei/templates/<name>/` | Project-local |
| 2 | `~/.agents/rhei/templates/<name>/` | User-global |

The `<name>` is the directory name and serves as the template identifier used in CLI commands.

Discovery errors are handled per command:

- `rhei templates` skips unreadable or invalid template directories and prints a warning naming the skipped path.
- `rhei instantiate <name>` fails if the matched template directory is unreadable or invalid.
- `rhei instantiate <path>` fails if the explicit template path is unreadable or invalid.

## 2. Directory Layout

```
<name>/
├── template.yaml          # Manifest (required)
├── states.yaml            # State machine (optional)
├── prompt_templates/      # Reusable state prompt fragments (optional; requires states.yaml)
│   └── review.md
├── settings.json          # Template-shipped project settings (optional)
├── plan.rhei.md           # Single-file plan skeleton
│   ── OR ──
├── index.rhei.md          # Directory-workspace index skeleton
├── tasks/                 # Directory-workspace task skeletons
│   ├── 01-step.md
│   └── ...
└── ...                    # Additional files (text rendered; binary copied)
```

A template must contain exactly one plan entry point: either `plan.rhei.md` (single-file) or `index.rhei.md` (directory workspace). Containing both is an error.

## 3. Manifest Schema (`template.yaml`)

```yaml
name: <identifier>             # Template identifier (must match directory name)
version: <yaml-scalar>         # Template version (informational; displayed by `rhei templates`;
                               #   no semantic-version validation in v1)
description: <string>          # One-line human-readable summary

inputs:
  - name: <identifier>        # Variable name, referenced as {{name}} in skeletons
    description: <string>      # What this input controls
    type: <string|number|boolean|path|array|object>  # Value type (default: string)
    required: <boolean>        # Whether the input must be supplied (default: true)
    default: <value>           # Default value (makes the input optional; mutually
                               #   exclusive with required: true)
    positional: <integer>       # Optional 1-based positional CLI slot for short
                               #   `rhei instantiate <template> <value>` input
    validate: <regex>          # Optional regex the value must match
    items:                     # Required for `type: array`
      type: <...>
      ...
    properties:                # Optional for `type: object`
      <property-name>:
        type: <...>
        required: <boolean>
        default: <value>
        ...
```

### 3.1. Validation Rules

- `name` must match the enclosing directory name.
- `inputs[].name` values must be unique within the manifest.
- `version` is informational metadata. In v1 it may be any YAML scalar and is not semantically validated.
- An input with `required: true` (the default) must not declare a `default`.
- An input with a `default` is implicitly `required: false`.
- An input with `required: false` and no `default` resolves to the empty string when not supplied by the user. Templates are pure text substitution in v1, so authors should tolerate that empty string directly or prefer declaring a `default`.
- `positional`, when present, must be a positive integer. Positional indexes must be unique within a manifest and contiguous starting at `1`.
- `type: array` inputs must declare `items`.
- `type: object` inputs may declare `properties`. Properties are required by default unless they declare either `required: false` or a `default`.
- `validate` is only valid on scalar input types (`string`, `number`, `boolean`, `path`).
- Optional inputs with no `default` resolve to type-shaped empty values:
  - `string`, `path` → `""`
  - `number`, `boolean` → `null`
  - `array` → `[]`
  - `object` → `{}`
- For `type: array` and `type: object`, positional values, `KEY=VALUE`, `--set KEY=...`, and `--set-file KEY=...` values are parsed as YAML/JSON snippets before validation.
- `type: path` values are rendered exactly as supplied by the user or manifest `default`; instantiation does not rewrite them to absolute paths. Relative `path` values are interpreted relative to the instantiating process `cwd` only when the CLI itself must resolve that path for its own file operations. The exception is an omitted optional `path` input with no `default`, which resolves to the empty string.
- `validate`, when present, is a Rust `regex`-crate pattern applied to the string representation of the resolved scalar value and anchored to the entire rendered value. It is enforced on every scalar it is declared on, including scalars nested inside `object` `properties` and `array` `items`; a failing match aborts instantiation with a path-qualified error (for example, `input 'agents[0].id' does not match validation pattern '…'`).

## 4. Template-Shipped Settings

A template may bundle a `settings.json` file alongside `template.yaml` to
declare project-scoped settings that the instantiated workspace should start
with — most commonly, the MCP server and skill profiles the state machine
references.

The file uses the standard project settings schema documented in
[Agents Specification — Global and Project Settings](rhei-agents.spec.md#11-global-and-project-settings).
It is treated as a text template file: instantiation variables (`{{name}}`)
are resolved at instantiation time so a template can parameterize
workspace-specific values (workspace ids, paths, hostnames) without exposing
host secrets.

On instantiation the rendered file is written to `.agents/rhei/settings.json`
in the output tree, where `rhei run` and `rhei validate` automatically pick it
up and compose it over the user's global `~/.config/rhei/settings.json`.

Example:

```json
// settings.json inside the template
{
  "mcp_servers": {
    "linear": {
      "command": ["npx", "-y", "@modelcontextprotocol/server-linear"],
      "env": { "LINEAR_WORKSPACE": "{{linear_workspace_id}}" }
    }
  },
  "skills": {
    "review-checklist": { "path": ".rhei/skills/review-checklist" }
  },
  "defaults": {
    "mcp_servers": ["linear"]
  }
}
```

With the matching manifest input:

```yaml
# template.yaml
inputs:
  - name: linear_workspace_id
    description: Linear workspace id used by the Linear MCP server
    type: string
    required: true
```

Rules:

- `settings.json` must be a UTF-8 JSON file. Instantiation fails if parsing
  the rendered file fails.
- References to host secrets use the settings file's standard `${VAR}`
  expansion — **not** template variables — so secrets are resolved at
  `rhei run` time on the user's machine, not baked into the output.
- A template that references MCP or skill ids in its `states.yaml` must
  declare matching registry entries either in its bundled `settings.json`
  or expect the user to provide them in their global settings. `rhei
  validate` (step 7 below) surfaces any remaining dangling references as
  errors.
- Users may edit `.agents/rhei/settings.json` after instantiation to replace
  template-declared entries, add project-specific overrides, or clear the
  `defaults` lists.

## 5. Instantiation Template Syntax

Instantiation rendering uses a restricted [MiniJinja](https://github.com/mitsuhiko/minijinja)-style environment. This is intentionally distinct from the single-brace runtime variables (`{task_id}`, `{visit_count}`, etc.) that later `rhei` commands resolve at execution time.

Supported constructs in v1:

- `{{ expr }}` for interpolation
- `{% for item in items %}` ... `{% endfor %}` for loops
- `{% if cond %}` ... `{% else %}` ... `{% endif %}` for conditionals
- `{% raw %}` ... `{% endraw %}` for literal template syntax
- `|slug` filter for filesystem-safe slugs

The renderer is **strict**:

- Referencing an undefined variable or missing object property is an error.
- `template.yaml` is parsed before rendering and is never templated.
- The environment does not load external templates; includes/imports are unavailable.

### 5.1. Resolution Rules

- Instantiation templates are resolved **at instantiation time**, producing a concrete plan with no remaining `{{...}}` / `{% ... %}` markers.
- Runtime variables (`{...}`) pass through instantiation untouched — they remain in the output for `rhei next` to resolve later.
- An unresolved `{{name}}` or missing `foo.bar` property is an **error** (fail-closed), unlike runtime variables which fail-open. This catches typos early.
- Instantiation always happens before any runtime `{...}` substitution from bundled or default state machines.
- Instantiation variables can appear in any **text** file that is materialized into the output tree. `template.yaml` is parsed before rendering, is excluded from output, and must not rely on `{{...}}` substitution.
- A file is considered text if it contains no null bytes in its first 8 KiB; all other files are binary.
- Text template files must decode as UTF-8. If a file is classified as text but cannot be decoded as UTF-8, instantiation fails with a file-read error.
- Binary files (images, compiled artifacts) are copied without instantiation-variable resolution or UTF-8 decoding.

### 5.2. Escaping

To emit a literal `{{` or `{%`, prefer MiniJinja raw blocks:

```jinja
{% raw %}{{target.slug}}{% endraw %}
```

For backward compatibility, `\{{` also emits a literal `{{` and the backslash is consumed during instantiation.

## 6. CLI Commands

### 6.1. `rhei instantiate`

Create a concrete plan workspace from a template.

```
rhei instantiate [template] [input ...] [options]

Arguments:
  [template]                   Template name or path to a template directory
  [input ...]                  Positional input values or key=value assignments

Options:
  --set <key>=<value>          Set an input value (repeatable)
  --set-file <key>=<path>      Set an input value from file contents (repeatable)
  --values <file>              Load input values from a YAML or JSON file (repeatable;
                                 later files, then input args / --set, then --set-file
                                 override earlier values)
  --output <path>              Output directory (default: ./<template-name>/ where
                                 <template-name> is the directory basename of the
                                 resolved template)
  --execute                    Instantiate and immediately begin execution
  --dry-run                    Show what would be generated without writing files
  --keep-on-error              Keep output directory on validation failure
  --list-inputs                Print the template's input schema and exit
```

#### 6.1.1. Input UX

`rhei instantiate` supports three equivalent ways to provide simple template
inputs:

```bash
rhei instantiate spec-review docs/functional-spec/rhei-run.spec.md
rhei instantiate code-review target=src/auth review_passes=3
rhei instantiate code-review --set target=src/auth --set review_passes=3
```

`--set` remains the explicit, script-safe form. Bare `KEY=VALUE` arguments are
short syntax for `--set KEY=VALUE`. Bare values without `=` are positional input
values.

A template may opt into positional input values by declaring `positional` on
one or more inputs:

```yaml
inputs:
  - name: spec
    description: Path to the specification file to review
    type: path
    positional: 1

  - name: criteria
    description: Additional things to look for during review
    type: string
    required: false
    default: ""
```

With that manifest, these commands are equivalent:

```bash
rhei instantiate spec-review docs/functional-spec/rhei-run.spec.md
rhei instantiate spec-review spec=docs/functional-spec/rhei-run.spec.md
rhei instantiate spec-review --set spec=docs/functional-spec/rhei-run.spec.md
```

For backward-compatible convenience, if a template declares exactly one
required input and no `positional` fields, one bare input value maps to that
required input. Templates with zero required inputs, multiple required inputs,
or multiple bare input values must declare `positional` fields or use explicit
`KEY=VALUE` / `--set` input.

Input arguments are parsed as follows:

- `KEY=VALUE` is an assignment when `KEY` is a valid input identifier. The key
  must name a declared template input.
- A value containing `=` is treated as a positional value when the text before
  the first `=` is not a valid input identifier. This keeps path-like values
  containing `=` usable.
- A positional value is assigned to the input with the matching `positional`
  index. When using the single-required-input fallback, it is assigned to that
  required input.
- Supplying the same input more than once is allowed. Later values from the
  same precedence group override earlier values.
- If both a positional value and an explicit assignment set the same input, the
  explicit assignment wins.
- `--set-file KEY=PATH` remains the file-content form and has higher precedence
  than positional values, `KEY=VALUE`, and `--set`.

#### 6.1.2. Behavior

1. **Show choices when omitted.** If no template is provided, print the same human-readable discovered-template list as `rhei templates` and exit successfully.
2. **Locate template.** Resolve `<template>` through the discovery chain unless it is already a filesystem path. Direct paths include absolute paths, relative paths containing `/`, and dot-prefixed relative paths such as `./my-template` or `../templates/review`. When a named template is not found and a discovered template name is sufficiently similar, include that closest name as a suggestion in the lookup error.
3. **Load manifest.** Parse `template.yaml`, validate schema.
4. **Collect inputs.** Resolve inputs using this precedence order: manifest defaults < `--values` files from left to right < positional input values < `KEY=VALUE` input arguments and `--set` flags from left to right < `--set-file` flags from left to right. Error on missing required inputs, unknown input names, ambiguous positional values, or duplicate `positional` declarations. Validate types and `validate` patterns. For `array` / `object` inputs, positional values, `KEY=VALUE`, `--set`, and `--set-file` values are parsed as YAML/JSON snippets before validation.
5. **Render templates.** Walk all materialized text files in the template directory and render them through the restricted MiniJinja environment. `template.yaml` is parsed before this step and is never rendered into the output. Error on any unresolved instantiation template reference.
6. **Write output.** In normal mode, copy the resolved tree to `--output`. `--output` must not already exist; instantiation fails rather than merging into or overwriting an existing path. In `--dry-run` mode, the CLI skips the output-path existence check, materializes into a temporary scratch directory instead of `--output`, validates that scratch output, and reports what would have been written. Preserve directory structure and file permissions. Hidden files and directories (names starting with `.`) and `template.yaml` itself are excluded from the output. A root-level `settings.json` in the template is moved to `.agents/rhei/settings.json` under the output root; all other files preserve their template-relative paths.
7. **Validate.** Run `rhei validate` on the instantiated plan. If the output root contains `states.yaml`, treat that file as the state machine for validation; otherwise fall back to the built-in default. Validation composes the merged settings (global, then output-root `.agents/rhei/settings.json`) and resolves every `agent`, `model`, `mcp_servers`, and `skills` reference declared in the state machine. Warnings are printed; errors abort (output directory is removed on error unless `--keep-on-error` is passed).
8. **Print summary.** After successful validation, print a human-readable instantiation summary with the output path, task/state counts, instantiated output tree, rendered task tree, the last few rendered task definitions in source order, and a stop-point explanation. For normal instantiation without `--execute`, the stop point is the next ready task and the reason is that execution has not started.
9. **Print invocation.** Print a shell-safe `rhei instantiate ... --output <path>` command that shows how to instantiate the same template and input values again. The printed command uses the resolved output path, so shell expressions such as `$(date ...)` appear as the concrete path value seen by the CLI.
10. **Execute (optional).** When `--execute` is passed, invoke `rhei run <output>` after successful validation. `rhei run` uses the instantiated output's root `states.yaml` by default when present; otherwise it falls back to the built-in default.

#### 6.1.3. Instantiation Summary Output

After successful validation, `rhei instantiate` prints a compact report before
the reproducible invocation. The report is intended to be useful for both
humans and agent-orchestrator logs: it shows what was created, which tasks are
present, the most recent task definitions, and why instantiation stopped.

The summary uses these headings in order:

```text
=== Instantiation Summary ===
Output: <output-path>
Tasks: <count>
States: <state=count[, ...]>

Files:
  <output-path>/
  ...

Task tree:
  - Task <id>: <title> [<state>]
  ...

Recent task definitions:
--- Task <id>: <title> [<state>] ---
### Task <id>: <title>
**State:** <state>
...

Stopped:
  <stop reason>
```

`Files` prints the full instantiated output tree, including hidden files that
were generated by instantiation such as `.agents/rhei/settings.json`. The root line
uses the user-requested `--output` path. In `--dry-run`, this is still the
requested path even though the rendered files live in a temporary scratch
directory and are discarded after validation.

`Task tree` prints every rendered task in source order, preserving hierarchy by
indentation. Each line includes the canonical task kind, task id, title, and
current state.

`Recent task definitions` prints up to the last five rendered task definitions
in source order. Each definition reconstructs the task heading, `**State:**`,
`**Prior:**` when present, `**Assignee:**` when present, and the task body. It
does not inline child task definitions under a parent; child tasks appear as
their own recent definitions when they are among the last five tasks.

`Stopped` explains the current hand-off point:

- In `--dry-run`, the reason says rendering and validation succeeded but no
  files were written to the requested output path.
- If all tasks are terminal, the reason says the plan is already complete.
- If one or more tasks are in a gating state, the reason lists the first few
  human-gated tasks.
- If at least one task is ready, the reason names the next ready task and shows
  `rhei run <output>` and `rhei next <output>` follow-up commands.
- If no task is ready because prerequisites are incomplete, the reason lists
  the first few blocked tasks.
- If none of the above applies, the reason says validation succeeded but no
  claimable task was found.

#### 6.1.4. Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Any instantiation error, including template lookup failure, missing or invalid inputs, unresolved instantiation variables, output-path conflicts, validation failure, or `--execute` runtime failure surfaced by the CLI |

### 6.2. `rhei templates`

List available templates.

```
rhei templates [options]

Options:
  --json                       Output as JSON
  --source <project|user|all>  Filter by source (default: all)
```

Prints a table of discovered templates with name, version, description, source path, and required input count.

When discovery encounters an invalid template directory, `rhei templates` skips it and prints a warning instead of failing the entire listing.

## Example

### Template: `code-review`

```
~/.agents/rhei/templates/code-review/
├── template.yaml
├── states.yaml
├── prompt_templates/
│   └── review.md
├── index.rhei.md
├── tasks/
│   └── 01-review.md
└── workflow.sh
```

#### `template.yaml`

```yaml
name: code-review
version: 1.0.0
description: Multi-pass code review with fix loop

inputs:
  - name: target
    description: File or directory to review
    type: path
    positional: 1

  - name: review_passes
    description: Number of review iterations
    type: number
    default: 2

  - name: model
    description: Model to use for review
    default: claude
```

#### `prompt_templates/review.md`

```markdown
Review pass {visit_count} of {visits} for Task {task_id}: {task_title}.
Focus on `{{target}}`.
Write findings to `{review_notes_path}`.
```

#### `index.rhei.md`

```markdown
# Rhei: Code Review — {{target}}
**States:** code-review

## Overview
Automated {{review_passes}}-pass review of `{{target}}` using {{model}}.
```

#### `tasks/01-review.md`

```markdown
### Task review: Review {{target}}
**State:** review

Review `{{target}}` for correctness, style, and security issues.
Write findings to `{output.review-notes.path}`.
```

#### `states.yaml`

```yaml
name: code-review
version: 1.0.0

states:
  review:
    description: Review pass
    prompt_template:
      name: review
      values:
        visit_count: "{visit_count}"
        visits: "{visits}"
        task_id: "{task_id}"
        task_title: "{task_title}"
        review_notes_path: "{output.review-notes.path}"
    visits: {{review_passes}}
    outputs:
      - name: review-notes
        path: runtime/reviews/task-{task_id}-review-{visit_count}.md

  fix:
    description: Fix findings from review
    instructions: |
      Fix pass {visit_count} of {visits} for Task {task_id}: {task_title}.
      Read `{input.review-notes.path}` and apply fixes to `{{target}}`.
      Write a summary of changes to `{output.fix-summary.path}`.
    visits: {{review_passes}}
    inputs:
      - name: review-notes
        path: runtime/reviews/task-{task_id}-review-{visit_count}.md
    outputs:
      - name: fix-summary
        path: runtime/fixes/task-{task_id}-fix-{visit_count}.md

  completed:
    description: Review and fixes complete.
    final: true

  cancelled:
    description: Review cancelled.
    final: true

transitions:
  - from: review
    to: fix
    description: Review pass done, apply fixes.
  - from: fix
    to: review
    condition: visitCount < visits
    description: Return for another review pass.
  - from: fix
    to: completed
    condition: visitCount >= visits
    description: All passes complete.
  - from: "*"
    to: cancelled
    description: Cancel from any state.

profiles:
  default:
    initial: review
    allowed: [review, fix, completed, cancelled]

node_policy:
  root: default
  default: default
```

### Instantiation

```bash
rhei instantiate code-review src/auth/ \
  review_passes=3 \
  --output ./reviews/auth-review/

# Or instantiate and run immediately:
rhei instantiate code-review src/auth/ --execute
```

### Resulting Output (`./reviews/auth-review/`)

```markdown
# index.rhei.md
# Rhei: Code Review — src/auth/
**States:** code-review

## Overview
Automated 3-pass review of `src/auth/` using claude.
```

```yaml
# states.yaml (instantiation variables resolved; path text preserved as authored input)
name: code-review
states:
  review:
    instructions: |
      Review pass {visit_count} of {visits} for Task {task_id}: {task_title}.
      Focus on `src/auth/`.
    visits: 3
    ...
```

All `{{...}}` are resolved during instantiation. All `{...}` remain for runtime.

## 7. Interaction with Existing Features

| Feature | Interaction |
|---------|-------------|
| **Runtime variables** (`{task_id}`, etc.) | Pass through instantiation untouched. Resolved later by `rhei next`. |
| **State machines** | A template may bundle its own `states.yaml` at the output root. If present, `rhei instantiate`, `rhei validate`, `rhei run`, `rhei next`, and related commands that operate on the instantiated workspace use that sibling/root `states.yaml` by default when `--state-machine` is not supplied; otherwise they fall back to the built-in default. When the rendered plan declares `**States:** <name>`, that declaration participates in lookup: the auto-discovered `states.yaml` is the active configuration and its `name` must match `<name>`. `--state-machine <path>` overrides the auto-discovered file. If a sibling `prompt_templates/` directory exists next to the active `states.yaml`, its direct `.md` files are loaded with that state machine. Templates that rely on non-default state names should therefore bundle `states.yaml`. |
| **Directory workspaces** | Templates can produce directory workspaces. The `tasks/` directory and `index.rhei.md` are resolved like any other template file. |
| **`rhei validate`** | Runs automatically post-instantiation. Template authors can validate their templates with `rhei instantiate --dry-run`. |
| **Program states** | Program states (`program` field) work in templates. Instantiation variables resolve in `program` strings, `program.command` arrays, `program.env` values, and `program.working_directory`. Runtime variables in those fields pass through to `rhei run`. |
| **Skills** | The `rhei-plan-worker` skill works on instantiated plans identically to hand-authored plans. No skill changes required. |
| **`install-skills`** | Unchanged. Templates are orthogonal to skill installation. |

## 8. Grammar Extension

The `rhei_document` and `workspace_index` productions are unchanged. Templates are a pre-processing layer that produces valid Rhei documents — the parser never sees `{{...}}` syntax.

No changes to the Rhei plan grammar are required.

## 9. Manifest Fields

`template.yaml` is a YAML mapping parsed by field name, not by key order. The following schema is normative:

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `name` | string | Yes | Must match the enclosing template directory name and the identifier rules described above. |
| `version` | YAML scalar | Yes | Informational metadata only in v1. |
| `description` | string | Yes | Must be non-empty after trimming. |
| `inputs` | sequence of mappings | No | Defaults to an empty list when omitted. |

Each `inputs[]` entry is a YAML mapping with these fields:

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `name` | string | Yes | Unique within the manifest; referenced as `{{name}}` in instantiation variables. |
| `description` | string | Yes | Must be non-empty after trimming. |
| `type` | `string` \| `number` \| `boolean` \| `path` \| `array` \| `object` | No | Defaults to `string`. |
| `required` | boolean | No | Defaults to `true` unless a `default` is present. |
| `default` | YAML value | No | Must be compatible with `type`; mutually exclusive with `required: true`. |
| `positional` | positive integer | No | Optional 1-based CLI positional input slot. Values must be unique and contiguous starting at `1`. |
| `validate` | string | No | Rust `regex`-crate pattern, matched against the fully rendered value. |

## 10. File Extension

Template manifest files use the `.yaml` extension. Template plan entry points are exactly `plan.rhei.md` or `index.rhei.md`. In directory-workspace templates, files under `tasks/` may use `.md` consistent with standard Rhei workspaces.

## Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) — Grammar and semantics of the output format
- [States Specification](rhei-states.spec.md) — State machine format (bundled in templates)
- [Program States Specification](rhei-programs.spec.md) — Deterministic program execution (program states work in templates)
- [How Rhei Is Used](rhei-usage.spec.md) — Roles and coordination patterns
