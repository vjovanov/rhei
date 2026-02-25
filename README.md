# Rhei

Rhei is a Rust workspace for parsing, validating, executing, and rendering structured markdown plans.

## Why Rhei

Rhei is the only agent task-planning system that combines all of:

- **Markdown is the source of truth.** A plan is a `.rhei.md` file you can read,
  diff, and edit in any editor — not a database, not a chat scratchpad.
- **Explicit prerequisite DAG.** `**Prior:**` declares dependencies, validated
  for cycles, missing references, and kind mismatches.
- **Hierarchical tasks** with configurable depth (`maxLevels` 1–4) and custom
  node kinds (`Task`, `Bug`, `Spike`, …).
- **Pluggable YAML state machines.** Define your own states, allowed transitions,
  per-node-kind profiles, and required input/output artifact contracts per
  state — including counted review loops via `visits: n` and `state-2` suffixes.
- **Multi-agent coordination over git.** Directory Workspace mode shards tasks
  into per-file markdown so swarms can advance in parallel without merge
  conflicts; `rhei transition` provides atomic compare-and-swap on state.
- **Deterministic ready-work selection.** `rhei next` claims the next eligible
  task by terminal-state prerequisites and node policy, no LLM guesswork.
- **Full validator.** `rhei validate` checks syntax, state validity, dependency
  integrity, hierarchy/id alignment, link integrity, terminal-tree coherence,
  and artifact contracts.
- **Templates: automate your complex daily routines in minutes.** Capture a
  recurring workflow — code review loops, release checklists, onboarding,
  audits — once as a parameterized template (plan skeleton + state machine +
  typed inputs), then `rhei instantiate` it with concrete values to spin up a
  ready-to-execute workspace. See
  [`docs/specs/rhei-templates.spec.md`](docs/specs/rhei-templates.spec.md).

See [`docs/comparison.md`](docs/comparison.md) for a detailed comparison against
beads, beans, opencode, Claude Code TodoWrite, Cline, Cursor, Roo, Devin, and
Augment.

Current workspace crates:
- `rhei-plan-core` (`rhei_core`): AST types plus markdown plan parsing
- `rhei-cli-validator` (`rhei_validator`): semantic validation against a YAML states definition
- `rhei-cli-output` (`rhei_output`): JSON, GitHub-style markdown, and progress-report rendering
- `rhei-cli`: `rhei` command for validation, execution, and rendering
- `rhei-api-napi`: Node.js bindings

## Markdown plan compiler

The markdown plan compiler currently supports:
- parsing rhei/task/subtask structure from markdown plans
- validating task metadata and dependencies against a states definition in [`docs/specs/states.yaml`](docs/specs/states.yaml)
- rendering parsed plans as JSON, GitHub-style markdown, or terminal-oriented progress output

The primary reference documents are:
- [`docs/overview.md`](docs/overview.md) — **start here** for tool usage and specification index
- [`docs/agent-orchestrator-workflow.md`](docs/agent-orchestrator-workflow.md) — orchestrator/worker interaction model
- [`docs/rhei.spec.md`](docs/rhei.spec.md) — plan language specification
- [`docs/specs/rhei-states.spec.md`](docs/specs/rhei-states.spec.md) — states specification
- [`docs/specs/states.yaml`](docs/specs/states.yaml) — default validation states definition

## Install

### Cargo

Install the `rhei` CLI from this checkout with Cargo:

```bash
cargo install --path crates/rhei-cli --locked --force
```

Install the published CLI package from crates.io:

```bash
cargo install rhei-cli --locked
```

The crates.io package is named `rhei-cli`; the installed command is still
`rhei`.

Use `--locked` so Cargo respects the repository lockfile. This avoids resolving newer dependency versions that may require a newer Rust compiler than the project currently targets.

Cargo installs the binary to `~/.cargo/bin/rhei`. Make sure `~/.cargo/bin` is on `PATH` before any older system install location:

```bash
type -a rhei
rhei version
```

If an older `/usr/local/bin/rhei` appears before `~/.cargo/bin/rhei`, either adjust `PATH` or invoke the Cargo-installed binary directly:

```bash
~/.cargo/bin/rhei version
```

### npm

Install the CLI from npm:

```bash
npm install -g rhei
rhei version
```

Use the JavaScript helper API:

```bash
npm install rhei-api
```

```js
const { version, runCaptureSync } = require("rhei-api");

console.log(version());
const result = runCaptureSync(["validate", "plan.rhei.md"]);
```

The npm packages install the Rust CLI through Cargo during installation, so
Rust and Cargo must be available on `PATH`.

### PyPI

Install the CLI from PyPI:

```bash
python3 -m pip install rhei-cli
rhei version
```

Use the Python helper API:

```bash
python3 -m pip install rhei-api
```

```python
import rhei_api

print(rhei_api.version())
result = rhei_api.run(["validate", "plan.rhei.md"], capture_output=True)
```

The PyPI package name is `rhei-cli` because `rhei` is already taken on PyPI.
The installed command is still `rhei`.

### Completions

Install shell completions for the current user:

```bash
rhei completions bash --install
rhei completions zsh --install
rhei completions fish --install
rhei completions powershell --install
rhei completions elvish --install
```

Installed completions are dynamic, so `rhei instantiate <TAB>` offers template
names from `.agents/rhei/templates/` and `~/.agents/rhei/templates/`.

See [Tab Completions](docs/tab-completions.md) for shell-specific setup notes,
default install paths, and system-wide installation.

## CLI usage

Validate a plan with the built-in default states definition:

```bash
cargo run -p rhei-cli -- validate examples/release-automation.rhei.md
```

Validate using a specific states file:

```bash
cargo run -p rhei-cli -- --state-machine docs/specs/states.yaml validate examples/release-automation.rhei.md
```

Watch a plan and states file for changes:

```bash
cargo run -p rhei-cli -- validate --watch examples/release-automation.rhei.md
```

Render a plan as pretty JSON:

```bash
cargo run -p rhei-cli -- render examples/release-automation.rhei.md --format json --pretty
```

Render a plan as GitHub-style markdown without metadata or subtask body text:

```bash
cargo run -p rhei-cli -- render examples/release-automation.rhei.md --format github --no-metadata --no-content
```

Render a terminal progress report without ANSI color:

```bash
cargo run -p rhei-cli -- render examples/release-automation.rhei.md --format progress --no-color
```

Claim the next ready task and inspect its instructions:

```bash
cargo run -p rhei-cli -- next examples/release-automation.rhei.md
```

Complete a task and record the result:

```bash
cargo run -p rhei-cli -- complete examples/release-automation.rhei.md --task 1 --result "Brief approved"
```

Print crate versions surfaced by the CLI:

```bash
cargo run -p rhei-cli -- version
```

Reset a plan back to the initial state declared in its state machine:

```bash
cargo run -p rhei-cli -- --state-machine docs/specs/states.yaml reset examples/release-automation.rhei.md
```

## Library usage

Typical flow inside Rust code:

1. Add `rhei_core = { package = "rhei-plan-core", version = "0.1.0-alpha.1" }`
2. Parse markdown with `rhei_core::parse`
3. Load a states definition with `rhei_validator::StateMachine::from_yaml_file`
4. Validate with `rhei_validator::validate_with_machine` or `rhei_validator::validate_from_machine_file`
5. Render with helpers from `rhei_output`

The published package names are conflict-free, while the Rust crate import
names remain `rhei_core`, `rhei_validator`, and `rhei_output`.

## Status notes

This documentation reflects the current repository behavior. In particular:
- parsing retains rhei-level text and subtask body content
- validation enforces required `**State:**` metadata, dependency existence, metadata ordering, cycle detection, and subtask numbering checks
- rendering is available for JSON, GitHub-style markdown, and progress reports
- examples beyond repository documents are tracked separately by subtask 8.4
