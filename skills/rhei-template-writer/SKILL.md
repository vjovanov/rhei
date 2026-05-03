---
name: rhei-template-writer
description: Design and generate Rhei Templates — parameterized, reusable bundles of a plan skeleton, optional state machine, optional settings, and a typed input manifest that can be instantiated with `rhei instantiate` to produce ready-to-execute workspaces. Use when users want to capture a recurring workflow (review loops, release checklists, onboarding, audits) once and instantiate it later with concrete inputs.
---

# Rhei Template Writer

Produce a Rhei Template directory — a manifest plus a plan skeleton (and, when needed, a state machine and settings) — that `rhei instantiate` can render into a concrete, executable workspace.

The template writer runs before `rhei instantiate`. It packages a proven workflow; `rhei instantiate` materializes it with user-supplied inputs. It does not replace the plan writer or state machine writer — it composes their outputs into something reusable.

## When To Use This Skill

Use this skill when a workflow is going to be instantiated more than once with the same shape but different inputs:

- A code-review loop parameterized by target directory and pass count.
- A release checklist parameterized by version, release type, and environments.
- An onboarding workflow parameterized by new hire name and team.
- A compliance audit parameterized by scope and reviewers.
- Any "scaffold a workspace" request where plans and state machines would otherwise be copy-pasted.

Do **not** use this skill when:

- The user only needs a single plan — use `rhei-plan-writer` directly.
- The user only needs a state machine — use `rhei-state-machine-writer` directly.
- The workflow varies so much that parameterization costs more than it saves.

## Required Inputs

Gather all three before designing the template. If anything is missing, ask and stop.

### 1. Workflow intent

- What workflow is being captured? One sentence.
- Which parts change per instantiation, and which parts stay constant?
- Is the workflow single-file (one plan) or multi-file (directory workspace)?

### 2. Parameter surface

- Which values must the user supply?
- Which values have sensible defaults?
- What are the types (`string`, `number`, `boolean`, `path`, `array`, `object`)?
- Any validation regexes (for example, semver strings, ticket-id patterns)?

### 3. State machine scope

- Does the workflow fit the built-in `rhei` state machine? If yes, bundle no `states.yaml`.
- Does it need a custom machine? If yes, author it via `rhei-state-machine-writer` and bundle it.
- Does it need MCP servers, skills, or agents declared in `settings.json`? If yes, bundle one.

## Output Contract

A template is a directory. Required layout:

```
<name>/
├── template.yaml          # Manifest (required)
├── README.md              # Template documentation (required — see Required Accompaniments)
├── states.yaml            # State machine (optional — omit to use the built-in `rhei` default)
├── settings.json          # Template-shipped project settings (optional)
├── plan.rhei.md           # Single-file plan skeleton  — OR —
├── index.rhei.md          # Directory-workspace index skeleton
├── tasks/                 # Directory-workspace task skeletons (only with index.rhei.md)
│   ├── 01-step.md
│   └── ...
└── ...                    # Additional files: text rendered, binary copied
```

Every template must also ship a pre-rendered example workspace checked in elsewhere (see *Required Accompaniments*).

When a custom state machine is needed, `states.yaml` is a required generated artifact, not background design notes. Produce the complete YAML file in the template directory, include it in any file-by-file response, and keep the rendered plan's `**States:** <name>` declaration aligned with `states.yaml:name`.

Rules:

- The directory name is the template identifier. It must match `manifest.name` exactly.
- Exactly one plan entry point: `plan.rhei.md` **or** `index.rhei.md`. Having both is an error.
- `template.yaml` is parsed before rendering and is never templated itself. It is also excluded from the instantiated output.
- Hidden files / directories (names beginning with `.`) and `template.yaml` are excluded from the output.
- A root-level `settings.json` is moved to `.rhei/settings.json` in the output — do not write the `.rhei/` path in the template source.
- Binary files (any file with null bytes in the first 8 KiB) are copied verbatim. Text files are rendered through the instantiation template environment and must decode as UTF-8.

## Required Accompaniments

Every template must ship with three pieces of context alongside the skeleton itself. Omit any of them and the template is considered incomplete.

### 1. `README.md` at the template root

One file at `<template>/README.md` describing:

- What the template does (one-paragraph summary).
- A table of inputs: name, type, default, description.
- A short per-task-kind summary of how each task walks the state machine (a table is usually enough).
- The narrative flow in numbered steps (what the coordinator does, what the fan-out looks like, where human gates live).
- The canonical `rhei instantiate` invocation with representative `--set` arguments.
- A link to the checked-in example.

Do not inline the state machine diagram here — it belongs in `states.yaml` comments (see below). Link to `states.yaml` from the README.

The README is rendered through the instantiation environment like any other text file. Keep `{{...}}` out of it unless you want per-instantiation copies to diverge; generally the README documents the template itself, not the rendered output.

### 2. State machine diagram as a comment block in `states.yaml`

When the template bundles a `states.yaml`, add an ASCII diagram as a YAML comment block at the very top of the file — before `name:`. The diagram must cover:

- Every non-terminal state and the transitions between them.
- Which state is `initial`.
- Which states are `final`.
- Gating states (where the agent must stop).
- Any `all_models` / `all_targets` fan-out points, named explicitly.
- A short list of per-task paths through the machine (`coordinator: split → completed`, etc.), so readers can see how different task kinds traverse the same graph.

This lives in the YAML file, not just the README, so anyone reading the state machine sees the picture without a context switch. If the template has no `states.yaml` (i.e., it uses the built-in `rhei` machine), skip this — the built-in is documented elsewhere.

### 3. A pre-rendered example that passes `rhei validate`

For every template, check in one pre-rendered example so reviewers and users can see a working instantiation without running `rhei instantiate` themselves. The example is the template's smoke test.

- Place it under `examples/<template-name>-example/` at the repo root (project conventions may vary; match what neighbouring templates do).
- Generate it with `rhei instantiate <template> --set ... --output examples/<template-name>-example`.
- Overwrite the rendered `README.md` with an example-specific one that records the `--set` values used, the validate command, and the regenerate command. This keeps the template's own README as documentation and the example's README as an instantiation log.
- The example must pass `rhei validate examples/<template-name>-example` as shipped.
- Re-generate the example any time the template changes state shape, inputs, or the default rendering of the seed files.

Pick inputs that exercise every non-trivial code path in the template — e.g., if there's a `focus_areas` input and an empty-list branch, set it to a non-empty list in the example so the rendered output demonstrates the branch. If one example can't reasonably cover every branch, pick the most interesting combination and note the trade-off in the example's README.

## Manifest Contract (`template.yaml`)

```yaml
name: <identifier>             # Must match the enclosing directory name
version: <yaml-scalar>         # Informational; displayed by `rhei templates`
description: <string>          # One-line human-readable summary

inputs:
  - name: <identifier>         # Variable name, referenced as {{name}} in skeletons
    description: <string>      # What this input controls
    type: <string|number|boolean|path|array|object>  # Default: string
    required: <boolean>        # Default: true (unless `default` is present)
    default: <value>           # Mutually exclusive with required: true
    validate: <regex>          # Rust regex applied to the rendered scalar value
    items:                     # Required when type: array
      type: <...>
    properties:                # Optional when type: object
      <property-name>:
        type: <...>
        required: <boolean>
        default: <value>
```

Rules the writer enforces at author time:

- `name` matches the directory name.
- `description` is non-empty after trimming.
- Input names are unique within the manifest.
- `required: true` and `default:` are mutually exclusive. A `default` implicitly makes the input optional.
- `validate` is only valid on scalar types (`string`, `number`, `boolean`, `path`).
- `type: array` entries must declare `items`.
- `type: object` properties are required by default unless they declare `required: false` or a `default`.
- Optional inputs with no `default` resolve to type-shaped empty values at instantiation time (`""` for `string`/`path`, `null` for `number`/`boolean`, `[]` for `array`, `{}` for `object`). Author the template to tolerate the empty value, or declare a `default`.
- `type: path` values are rendered verbatim — do not assume the instantiator rewrites them to absolute paths.

## Instantiation Template Syntax

The instantiator uses a restricted MiniJinja environment. **Instantiation templates are distinct from Rhei runtime variables.** Both can coexist in the same file:

| Form | Resolved by | Example | When |
|---|---|---|---|
| `{{ name }}` | `rhei instantiate` | `{{target}}` | At instantiation time |
| `{% ... %}` | `rhei instantiate` | `{% for t in targets %}...{% endfor %}` | At instantiation time |
| `{name}` | `rhei next` / `rhei run` | `{task_id}`, `{visit_count}`, `{output.review-notes.path}` | At runtime |

Supported MiniJinja constructs in v1:

- `{{ expr }}` interpolation.
- `{% for item in items %}` ... `{% endfor %}`.
- `{% if cond %}` ... `{% else %}` ... `{% endif %}`.
- `{% raw %}` ... `{% endraw %}` to emit literal `{{` / `{%`.
- `|slug` filter for filesystem-safe slugs.

The renderer is strict:

- Referencing an undefined variable or missing object property is an error — typos fail instantiation immediately.
- External includes / imports are not available.
- Unresolved `{{...}}` in the output is an error.
- Runtime `{name}` variables **pass through** untouched; they are not errors at instantiation time.

Escaping: prefer `{% raw %}{{task_id}}{% endraw %}` to emit a literal `{{...}}`. The legacy `\{{` escape also works and the backslash is consumed.

## Design Rules

### Manifest

1. **Name inputs for the thing, not the form.** Prefer `target`, `review_passes`, `release_version` over `input_path`, `count`, `string1`.
2. **Prefer `default:` over required.** If a sensible default exists, declare it — the user can always override with `--set`.
3. **Use `validate` for format-shaped values.** Semver strings, slug identifiers, date fragments. Don't use it for open-ended free text.
4. **Keep the input surface small.** Every input is cost for the user; every missing input forces a prompt. If the workflow has five knobs but two are always flipped the same way, fold them.
5. **Document inputs in `description`.** The description is what the user sees when they forget what the input does. Describe the effect, not the type.

### Plan skeleton

1. **Treat the skeleton like a plan authored by `rhei-plan-writer`.** It must pass `rhei validate` after rendering. Follow the plan writer's contract: exactly one H1 `# Rhei: <title>`, optional `**States:**`, `## Tasks` last (for single-file), tasks with `**State:**` first, `**Prior:**` second.
2. **Never author `**Assignee:**` or `> **Result:**`.** These are runtime-owned.
3. **Use `{{...}}` only where the plan actually depends on input.** A template that uses `{{target}}` in every task title is noisier than one that only interpolates where the value matters.
4. **Use `{% for %}` to fan out tasks only when the user-supplied input controls multiplicity.** For example, one task per reviewer or per environment. Keep generated IDs stable and unique.
5. **Keep runtime variables runtime.** Don't try to resolve `{task_id}` at instantiation time — it must remain `{task_id}` in the output for `rhei next` to resolve per task.

### State machine (optional)

1. **Omit `states.yaml` when the built-in `rhei` machine fits.** That's the default, and leaving it out makes the template smaller and auto-pickable.
2. **Bundle a custom `states.yaml` when the template needs non-default states, artifact contracts, visit loops, program states, model fan-out, or team gates.** Follow `rhei-state-machine-writer`; do not stop at a prose summary of the machine.
3. **If the rendered plan declares `**States:** <name>`, the bundled `states.yaml`'s `name` must match.** Auto-discovery keys off the YAML's `name` field.
4. **Use `{{...}}` inside `states.yaml` only where the workflow needs parameterized control.** Common patterns: `visits: {{review_passes}}`, `model: {{model}}`.
5. **Respect the runtime/instantiation boundary.** `{task_id}` stays literal; `{{model}}` resolves at instantiation.

### Settings (optional)

1. **Only bundle `settings.json` when the template references MCP servers, skills, or agent profiles that aren't guaranteed to exist in the user's global config.** Otherwise leave it out.
2. **Use `{{...}}` for workspace-specific values** (workspace ids, hostnames, paths), and the settings-file's standard `${VAR}` expansion for secrets — `${VAR}` is resolved at `rhei run` time on the user's machine, not at instantiation.
3. **Every MCP or skill id referenced by the bundled `states.yaml` must be declared here or in the user's global settings.** `rhei validate` (invoked post-instantiation) surfaces dangling references.
4. **Write `settings.json` at the root of the template.** `rhei instantiate` moves it to `.rhei/settings.json` in the output automatically.

### Additional files

1. **Bundle scripts and runbooks the state machine references from callbacks** (`on_leave`, `on_enter`, or `program` states).
2. **Binary assets are copied verbatim.** Images, fonts, and compiled artifacts pass through without rendering.
3. **Avoid bundling anything the user can reasonably supply themselves.** Smaller templates are easier to audit.

## Workflow

1. Confirm the workflow, parameters, and state-machine scope with the user.
2. Pick single-file (`plan.rhei.md`) or directory workspace (`index.rhei.md` + `tasks/`). Prefer single-file unless the workflow produces enough tasks that per-file concurrency matters.
3. Draft `template.yaml` with the minimum required inputs.
4. Draft the plan skeleton. Interpolate `{{...}}` only where input shapes the output. Keep runtime `{...}` variables where they belong.
5. Decide whether to bundle `states.yaml`. If yes, apply `rhei-state-machine-writer` to produce the complete machine body, wire in `{{...}}` interpolations where needed, and add the state machine diagram as a comment block at the top of the file.
6. Decide whether to bundle `settings.json`. If yes, declare MCP servers, skills, and `defaults` that match the state machine.
7. Place the template in a discoverable directory (see *File Placement*).
8. Validate with `rhei instantiate --dry-run <template> --set ...` and fix any rendering or validation errors.
9. Write `README.md` at the template root (inputs table, per-task paths through the state machine, flow, instantiate command, link to the example).
10. Generate the pre-rendered example into `examples/<template-name>-example/` and overwrite its README with an example-specific one. Run `rhei validate` on the example and on at least two other input combinations to catch branches the example doesn't cover.

## Response Discipline

When returning a template in chat instead of editing files directly, print a file-by-file artifact list. Include every required file as a fenced block with its path. If a custom state machine is needed, one of those blocks must be `<template>/states.yaml` and must contain the full YAML, including the top comment diagram. If no custom state machine is needed, say explicitly that the template intentionally uses the built-in `rhei` machine and therefore omits `states.yaml`.

Do not describe a custom state machine only in prose, and do not leave `states.yaml` for a later step unless the user explicitly asks for an outline instead of a complete template.

## Validation Checklist

Before returning the template, verify:

- Directory name matches `manifest.name`.
- `template.yaml` declares `name`, `version`, `description`, and optionally `inputs`.
- Every input has a unique `name` and a non-empty `description`.
- No input mixes `required: true` with a `default`.
- `type: array` inputs declare `items`; `validate` is only present on scalar types.
- Exactly one plan entry point (`plan.rhei.md` or `index.rhei.md`) exists.
- The entry point uses the Rhei Plan grammar (`# Rhei: <title>`, `## Tasks` last for single-file, task headings with `**State:**` first).
- No `**Assignee:**` or `> **Result:**` authored in the plan skeleton.
- Every `{{...}}` variable is declared in `manifest.inputs` (or is a nested property on an object input).
- Runtime `{name}` variables that pass through instantiation are valid against the active state machine's variable namespace (`{task_id}`, `{task_title}`, `{visit_count}`, `{visits}`, `{model}`, `{input.<name>.path}`, `{output.<name>.path}`, `{meta.<key>}`).
- If `states.yaml` is bundled and the rendered plan declares `**States:** <name>`, the YAML's `name` field matches `<name>`.
- If the workflow needs a custom state machine, `states.yaml` exists as a concrete artifact in the template output; it is not merely described in README text or the final response.
- If `states.yaml` is bundled, it passes the state-machine-writer validation checklist (profiles, node_policy, terminal reachability, etc.).
- If `settings.json` is bundled, it is valid JSON after rendering and every MCP / skill / agent id referenced by `states.yaml` is declared.
- `rhei instantiate --dry-run` produces an output that passes `rhei validate`.
- `<template>/README.md` exists and covers: one-paragraph summary, inputs table, per-task paths through the state machine, numbered flow, canonical `rhei instantiate` command, and a link to the pre-rendered example.
- If `states.yaml` is bundled, it begins with an ASCII state machine diagram as a YAML comment block covering states, transitions, initial / final / gating markers, fan-out points, and per-task paths.
- A pre-rendered example under `examples/<template-name>-example/` exists, was generated by `rhei instantiate`, has an example-specific `README.md` (inputs used, validate command, regenerate command), and passes `rhei validate <example-path>` as shipped.

## File Placement

Templates are resolved by `rhei instantiate <name>` in this order (first match wins):

| Priority | Location | Scope |
|---|---|---|
| 1 | `<project>/.agents/rhei/templates/<name>/` | Project-local |
| 2 | `~/.agents/rhei/templates/<name>/` | User-global |

Place project-scoped templates under `.agents/rhei/templates/` so the rest of the team picks them up from the checkout. Place personal templates under `~/.agents/rhei/templates/` so they're available across projects. A template can also be instantiated directly from an arbitrary filesystem path (`rhei instantiate ./path/to/template/`) — useful for authoring and review.

## Example Skeleton

A minimal one-input template using the built-in `rhei` machine and a single-file plan:

```
release-notes/
├── template.yaml
└── plan.rhei.md
```

```yaml
# template.yaml
name: release-notes
version: 1.0
description: Draft, review, and publish release notes for a version

inputs:
  - name: version
    description: Semantic version being released (e.g., 1.4.0)
    type: string
    validate: "^\\d+\\.\\d+\\.\\d+$"

  - name: channel
    description: Publication channel
    default: beta
```

```markdown
# plan.rhei.md
# Rhei: Release {{version}} notes

## Tasks

### Task draft: Draft notes for {{version}}
**State:** draft

Draft the release notes for `{{version}}` targeting the `{{channel}}` channel.
Include highlights, breaking changes, and migration notes.

### Task review: Review draft
**State:** draft
**Prior:** Task draft

Review the draft for accuracy, tone, and completeness.

### Task publish: Publish to {{channel}}
**State:** draft
**Prior:** Task review

Publish the reviewed notes to the `{{channel}}` channel.
```

Instantiation:

```bash
rhei instantiate release-notes \
  --set version=1.4.0 \
  --set channel=stable \
  --output ./releases/1.4.0/
```

## Missing Information Handling

If required input is missing:

- Ask the user to supply workflow intent, parameter surface, and state-machine scope.
- Do not invent parameters to cover uncertain parts of the workflow.
- If the template would be a thin wrapper over a single plan with no real parameterization, push back: `rhei-plan-writer` is the better tool.
- If the workflow needs more than roughly ten inputs or more than one state machine, push back: consider splitting into separate templates.
