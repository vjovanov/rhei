# Rhei

Rhei is an agent runtime for governed work. It turns Markdown workflows into
predictable agent and program execution with explicit state, dependencies,
artifacts, monitoring, snapshots, and reusable templates. The runtime can be
driven from the `rhei` CLI, embedded Rust crates, and language bindings.

## Why Rhei

Rhei is the only agent runtime that combines all of:

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
- **Runtime orchestration from CLI or API.** `rhei run` advances ready work
  through state machines, spawns agents or deterministic programs, captures
  logs and artifacts, and exposes the same model through reusable crates and
  bindings.
- **Full validator.** `rhei validate` checks syntax, state validity, dependency
  integrity, hierarchy/id alignment, link integrity, terminal-tree coherence,
  and artifact contracts.
- **Templates: automate your complex daily routines in minutes.** Capture a
  recurring workflow — code review loops, release checklists, onboarding,
  audits — once as a parameterized template (plan skeleton + state machine +
  typed inputs), then `rhei instantiate` it with concrete values to spin up a
  ready-to-execute workspace. See
  [`docs/functional-spec/rhei-templates.spec.md`](docs/functional-spec/rhei-templates.spec.md).

See [`docs/functional-spec/comparison.md`](docs/functional-spec/comparison.md) for a detailed comparison against
beads, beans, opencode, Claude Code TodoWrite, Cline, Cursor, Roo, Devin, and
Augment.

Current workspace crates:
- `rhei-plan-core` (`rhei_core`): core plan model for the agent runtime,
  including AST types, parsing, callbacks, and workspace primitives
- `rhei-agent-core` (`rhei_agent_core`): embeddable runtime-facing facade for
  agent workflow integrations
- `rhei-cli-validator` (`rhei_validator`): semantic validation against a YAML states definition
- `rhei-cli-output` (`rhei_output`): JSON, GitHub-style markdown, and progress-report rendering
- `rhei-cli`: `rhei` command-line driver for validation, execution, monitoring,
  snapshots, templating, and rendering
- `rhei-api`: language API package surface; the N-API implementation lives in
  `crates/rhei-napi`

## Agent runtime

The runtime currently supports:
- parsing Rhei, task, and subtask structure from Markdown workflows
- validating task metadata, dependencies, state machines, and artifact
  contracts against [`docs/functional-spec/states.yaml`](docs/functional-spec/states.yaml)
- selecting ready work deterministically with `rhei next`
- atomically advancing work with `rhei transition`, `rhei complete`, and
  `rhei reset`
- orchestrating agents and deterministic programs with `rhei run`
- recording runtime logs, results, snapshots, and dashboard state under
  `runtime/`
- rendering plans as JSON, GitHub-style markdown, or terminal-oriented progress
  output
- rendering a self-contained HTML **Flow** visualization of a plan or workspace
  with `rhei viz` (the same surface `rhei run` serves live)

The primary reference documents are:
- [`docs/architecture/overview.md`](docs/architecture/overview.md) — **start here** for tool usage and specification index
- [`docs/architecture/agent-orchestrator-workflow.spec.md`](docs/architecture/agent-orchestrator-workflow.spec.md) — orchestrator/worker interaction model
- [`docs/functional-spec/rhei-language-reference.spec.md`](docs/functional-spec/rhei-language-reference.spec.md) — canonical entry point for the authored Rhei language surface
- [`docs/functional-spec/rhei-plan-language.spec.md`](docs/functional-spec/rhei-plan-language.spec.md) — plan language specification
- [`docs/functional-spec/rhei-states.spec.md`](docs/functional-spec/rhei-states.spec.md) — states specification
- [`docs/functional-spec/states.yaml`](docs/functional-spec/states.yaml) — default validation states definition

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

See [Tab Completions](docs/functional-spec/tab-completions.md) for shell-specific setup notes,
default install paths, and system-wide installation.

## CLI usage

Validate a plan with the built-in default states definition:

```bash
cargo run -p rhei-cli -- validate examples/release-automation.rhei.md
```

Validate using a specific states file:

```bash
cargo run -p rhei-cli -- --state-machine docs/functional-spec/states.yaml validate examples/release-automation.rhei.md
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

Render a self-contained HTML Flow visualization and open it in the browser:

```bash
cargo run -p rhei-cli -- viz examples/release-automation.rhei.md --open
```

`rhei viz <plan|workspace>` writes a single offline HTML page — the plan, every
task and subtask with its state, the resolved state machine, and the surroundings
inspector — under the workspace's `runtime/` directory (`runtime/<input>.html`,
or `runtime/rhei-viz.html` for a workspace directory), the same place a live run
freezes its final dashboard, or to `--output <FILE>`. Writing under `runtime/`
keeps generated HTML out of the source tree. It is the same Flow surface
`rhei run` serves live, frozen to a file; the live agent terminal and intervene
composer are inert in the static page. See
[`docs/functional-spec/rhei-viz.spec.md`](docs/functional-spec/rhei-viz.spec.md).

Message a running agent during a live run (the headless sibling of the Flow
dashboard's intervene composer):

```bash
cargo run -p rhei-cli -- intervene --plan examples/release-automation.rhei.md \
  --task 3 -m "focus the review on error handling"
```

`rhei intervene` discovers the live run's dashboard from `runtime/dashboard.json`
and writes the message to the target agent's stdin — the same `/intervene`
channel the dashboard composer uses, never a plan transition. It only reaches
agents whose profile keeps stdin open (`intervene_stdin`); see [Enabling live
intervention](docs/functional-spec/rhei-agents.spec.md#112-agents). Every
delivery is recorded to `runtime/interventions.log`.

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
cargo run -p rhei-cli -- --state-machine docs/functional-spec/states.yaml reset examples/release-automation.rhei.md
```

## Development hooks

Install the pre-commit hook to run grounding checks before each commit:

```bash
pre-commit install
```

The checked-in hook runs:

```bash
grund check .
```

## Library usage

Typical flow inside Rust code that embeds the runtime model:

1. Add `rhei_agent_core = { package = "rhei-agent-core", version = "0.1.0" }`
2. Parse markdown with `rhei_agent_core::parse`
3. Load a states definition with `rhei_validator::StateMachine::from_yaml_file`
4. Validate with `rhei_validator::validate_with_machine` or `rhei_validator::validate_from_machine_file`
5. Render with helpers from `rhei_output`

The published package names are conflict-free, while the Rust crate import
names include `rhei_agent_core`, `rhei_core`, `rhei_validator`, and
`rhei_output`.

## Status notes

This documentation reflects the current repository behavior. In particular:
- parsing retains rhei-level text and subtask body content
- validation enforces required `**State:**` metadata, dependency existence, metadata ordering, cycle detection, and subtask numbering checks
- runtime execution is available through `rhei run`, `rhei next`,
  `rhei transition`, `rhei complete`, `rhei reset`, and `rhei snapshot`
- rendering is available for JSON, GitHub-style markdown, and progress reports
- examples beyond repository documents are tracked separately by subtask 8.4
