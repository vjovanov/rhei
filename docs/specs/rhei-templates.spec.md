# Rhei Templates Specification

This document specifies **Rhei Templates** — parameterized, reusable plan+state-machine bundles that can be instantiated with concrete inputs to produce ready-to-execute workspaces.

## Motivation

Rhei plans and state machines today are authored from scratch or copied manually. Common patterns — code review loops, release checklists, onboarding workflows — are recreated each time. Templates let users capture a proven workflow once and instantiate it with project-specific inputs, removing boilerplate and enforcing consistency.

## Concepts

A **template** is a directory containing:

1. A **manifest** (`template.yaml`) declaring the template's name, description, and typed input parameters.
2. A **state machine** (`states.yaml`) — optional; when absent, instantiation and validation fall back to the built-in `rhei` default.
3. A **plan skeleton** — either a single-file plan (`plan.rhei.md`) or a directory-workspace layout (`index.rhei.md` + `tasks/`).
4. **Additional files** — any non-manifest files bundled with the template. Text files are rendered with instantiation-variable substitution; binary files are copied verbatim into the output.

Materialized template files may contain **instantiation variables** (`{{name}}`) that are resolved at instantiation time from user-supplied inputs. These are distinct from the single-brace **runtime variables** (`{name}`) defined by the state-machine and plan specifications. Rendering is ordered: first resolve `{{...}}` across template files during `rhei instantiate`, then let later `rhei` commands resolve runtime `{...}` variables against the instantiated workspace. The manifest (`template.yaml`) is parsed before rendering and is never itself templated.

## Template Discovery

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

## Directory Layout

```
<name>/
├── template.yaml          # Manifest (required)
├── states.yaml            # State machine (optional)
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

## Manifest Schema (`template.yaml`)

```yaml
name: <identifier>             # Template identifier (must match directory name)
version: <yaml-scalar>         # Template version (informational; displayed by `rhei templates`;
                               #   no semantic-version validation in v1)
description: <string>          # One-line human-readable summary

inputs:
  - name: <identifier>        # Variable name, referenced as {{name}} in skeletons
    description: <string>      # What this input controls
    type: <string|number|boolean|path>  # Value type (default: string)
    required: <boolean>        # Whether the input must be supplied (default: true)
    default: <value>           # Default value (makes the input optional; mutually
                               #   exclusive with required: true)
    validate: <regex>          # Optional regex the value must match
```

### Validation Rules

- `name` must match the enclosing directory name.
- `inputs[].name` values must be unique within the manifest.
- `version` is informational metadata. In v1 it may be any YAML scalar and is not semantically validated.
- An input with `required: true` (the default) must not declare a `default`.
- An input with a `default` is implicitly `required: false`.
- An input with `required: false` and no `default` resolves to the empty string when not supplied by the user. Templates are pure text substitution in v1, so authors should tolerate that empty string directly or prefer declaring a `default`.
- `type: path` values are rendered exactly as supplied by the user or manifest `default`; instantiation does not rewrite them to absolute paths. Relative `path` values are interpreted relative to the instantiating process `cwd` only when the CLI itself must resolve that path for its own file operations. The exception is an omitted optional `path` input with no `default`, which resolves to the empty string.
- `validate`, when present, is a Rust `regex`-crate pattern applied to the string representation of the value and anchored to the entire rendered value.

## Template-Shipped Settings

A template may bundle a `settings.json` file alongside `template.yaml` to
declare project-scoped settings that the instantiated workspace should start
with — most commonly, the MCP server and skill profiles the state machine
references.

The file uses the standard project settings schema documented in
[Agents Specification — Global and Project Settings](rhei-agents.spec.md#global-and-project-settings).
It is treated as a text template file: instantiation variables (`{{name}}`)
are resolved at instantiation time so a template can parameterize
workspace-specific values (workspace ids, paths, hostnames) without exposing
host secrets.

On instantiation the rendered file is written to `.rhei/settings.json` in the
output tree, where `rhei run` and `rhei validate` automatically pick it up and
compose it over the user's global `~/.config/rhei/settings.json`.

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
  validate` (step 6 above) surfaces any remaining dangling references as
  errors.
- Users may edit `.rhei/settings.json` after instantiation to replace
  template-declared entries, add project-specific overrides, or clear the
  `defaults` lists.

## Instantiation Variable Syntax

Instantiation variables use **double-brace** syntax: `{{name}}`. This is intentionally distinct from the single-brace runtime variables (`{task_id}`, `{visit_count}`, etc.) that later `rhei` commands resolve at execution time.

### Resolution Rules

- Instantiation variables are resolved **at instantiation time**, producing a concrete plan with no remaining `{{...}}` markers.
- Runtime variables (`{...}`) pass through instantiation untouched — they remain in the output for `rhei next` to resolve later.
- An unresolved `{{name}}` where `name` is not a declared input is an **error** (fail-closed), unlike runtime variables which fail-open. This catches typos early.
- Instantiation always happens before any runtime `{...}` substitution from bundled or default state machines.
- Instantiation variables can appear in any **text** file that is materialized into the output tree. `template.yaml` is parsed before rendering, is excluded from output, and must not rely on `{{...}}` substitution.
- A file is considered text if it contains no null bytes in its first 8 KiB; all other files are binary.
- Text template files must decode as UTF-8. If a file is classified as text but cannot be decoded as UTF-8, instantiation fails with a file-read error.
- Binary files (images, compiled artifacts) are copied without instantiation-variable resolution or UTF-8 decoding.

### Escaping

To emit a literal `{{` in output, write `\{{` in the template. The backslash is consumed during instantiation. To emit a literal `\{{`, write `\\{{`. More generally, only `\` immediately before `{{` is consumed; backslashes elsewhere are preserved verbatim.

## CLI Commands

### `rhei instantiate`

Create a concrete plan workspace from a template.

```
rhei instantiate <template> [options]

Arguments:
  <template>                   Template name or path to a template directory

Options:
  --set <key>=<value>          Set an input value (repeatable)
  --set-file <key>=<path>      Set an input value from file contents (repeatable)
  --values <file>              Load input values from a YAML or JSON file (repeatable;
                                 later files, then --set, then --set-file override earlier values)
  --output <path>              Output directory (default: ./<template-name>/ where
                                 <template-name> is the directory basename of the
                                 resolved template)
  --execute                    Instantiate and immediately begin execution
  --dry-run                    Show what would be generated without writing files
  --keep-on-error              Keep output directory on validation failure
  --list-inputs                Print the template's input schema and exit
```

#### Behavior

1. **Locate template.** Resolve `<template>` through the discovery chain unless it is already a filesystem path. Direct paths include absolute paths, relative paths containing `/`, and dot-prefixed relative paths such as `./my-template` or `../templates/review`.
2. **Load manifest.** Parse `template.yaml`, validate schema.
3. **Collect inputs.** Resolve inputs using this precedence order: manifest defaults < `--values` files from left to right < `--set` flags from left to right < `--set-file` flags from left to right. Error on missing required inputs. Validate types and `validate` patterns. Type validation applies to the raw input value only; downstream type errors (for example, a `string` value substituted into a YAML integer field) surface at step 6 as plan validation failures.
4. **Resolve variables.** Walk all materialized text files in the template directory. Replace every `{{name}}` with its resolved value. `template.yaml` is parsed before this step and is never rendered into the output. Error on any unresolved `{{...}}` reference.
5. **Write output.** In normal mode, copy the resolved tree to `--output`. `--output` must not already exist; instantiation fails rather than merging into or overwriting an existing path. In `--dry-run` mode, the CLI skips the output-path existence check, materializes into a temporary scratch directory instead of `--output`, validates that scratch output, and reports what would have been written. Preserve directory structure and file permissions. Hidden files and directories (names starting with `.`) and `template.yaml` itself are excluded from the output. A root-level `settings.json` in the template is moved to `.rhei/settings.json` under the output root; all other files preserve their template-relative paths.
6. **Validate.** Run `rhei validate` on the instantiated plan. If the output root contains `states.yaml`, treat that file as the state machine for validation; otherwise fall back to the built-in default. Validation composes the merged settings (global, then output-root `.rhei/settings.json`) and resolves every `agent`, `model`, `mcp_servers`, and `skills` reference declared in the state machine. Warnings are printed; errors abort (output directory is removed on error unless `--keep-on-error` is passed).
7. **Execute (optional).** When `--execute` is passed, invoke `rhei run <output>` after successful validation. `rhei run` uses the instantiated output's root `states.yaml` by default when present; otherwise it falls back to the built-in default.

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Any instantiation error, including template lookup failure, missing or invalid inputs, unresolved instantiation variables, output-path conflicts, validation failure, or `--execute` runtime failure surfaced by the CLI |

### `rhei templates`

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

  - name: review_passes
    description: Number of review iterations
    type: number
    default: 2

  - name: model
    description: Model to use for review
    default: claude
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
    instructions: |
      Review pass {visit_count} of {visits} for Task {task_id}: {task_title}.
      Focus on `{{target}}`.
      Write findings to `{output.review-notes.path}`.
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
rhei instantiate code-review \
  --set target=src/auth/ \
  --set review_passes=3 \
  --output ./reviews/auth-review/

# Or instantiate and run immediately:
rhei instantiate code-review \
  --set target=src/auth/ \
  --execute
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

## Interaction with Existing Features

| Feature | Interaction |
|---------|-------------|
| **Runtime variables** (`{task_id}`, etc.) | Pass through instantiation untouched. Resolved later by `rhei next`. |
| **State machines** | A template may bundle its own `states.yaml` at the output root. If present, `rhei instantiate`, `rhei validate`, `rhei run`, `rhei next`, and related commands that operate on the instantiated workspace use that sibling/root `states.yaml` by default when `--state-machine` is not supplied; otherwise they fall back to the built-in default. When the rendered plan declares `**States:** <name>`, that declaration participates in lookup: the auto-discovered `states.yaml` is the active configuration and its `name` must match `<name>`. `--state-machine <path>` overrides the auto-discovered file. Templates that rely on non-default state names should therefore bundle `states.yaml`. |
| **Directory workspaces** | Templates can produce directory workspaces. The `tasks/` directory and `index.rhei.md` are resolved like any other template file. |
| **`rhei validate`** | Runs automatically post-instantiation. Template authors can validate their templates with `rhei instantiate --dry-run`. |
| **Program states** | Program states (`program` field) work in templates. Instantiation variables resolve in `program` strings, `program.command` arrays, `program.env` values, and `program.working_directory`. Runtime variables in those fields pass through to `rhei run`. |
| **Skills** | The `rhei-plan-worker` skill works on instantiated plans identically to hand-authored plans. No skill changes required. |
| **`install-skills`** | Unchanged. Templates are orthogonal to skill installation. |

## Grammar Extension

The `rhei_document` and `workspace_index` productions are unchanged. Templates are a pre-processing layer that produces valid Rhei documents — the parser never sees `{{...}}` syntax.

No changes to the Rhei plan grammar are required.

## Manifest Fields

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
| `type` | `string` \| `number` \| `boolean` \| `path` | No | Defaults to `string`. |
| `required` | boolean | No | Defaults to `true` unless a `default` is present. |
| `default` | YAML value | No | Must be compatible with `type`; mutually exclusive with `required: true`. |
| `validate` | string | No | Rust `regex`-crate pattern, matched against the fully rendered value. |

## File Extension

Template manifest files use the `.yaml` extension. Template plan entry points are exactly `plan.rhei.md` or `index.rhei.md`. In directory-workspace templates, files under `tasks/` may use `.md` consistent with standard Rhei workspaces.

## Related Specifications

- [Plan Language Specification](../rhei.spec.md) — Grammar and semantics of the output format
- [States Specification](rhei-states.spec.md) — State machine format (bundled in templates)
- [Program States Specification](rhei-programs.spec.md) — Deterministic program execution (program states work in templates)
- [How Rhei Is Used](rhei-usage.spec.md) — Roles and coordination patterns
