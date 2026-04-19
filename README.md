# Rhei

Rhei is a Rust workspace for parsing, validating, and rendering structured markdown plans.

Current workspace crates:
- `rhei-core`: AST types plus markdown plan parsing
- `rhei-validator`: semantic validation against a YAML states definition
- `rhei-output`: JSON, GitHub-style markdown, and progress-report rendering
- `rhei-cli`: `rhei` command for validation and rendering
- `rhei-napi`: Node.js bindings

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

## CLI usage

Validate a plan with the default states definition:

```bash
cargo run -p rhei-cli -- validate docs/markdown-plan-compiler.md
```

Validate using a specific states file:

```bash
cargo run -p rhei-cli -- --state-machine docs/states.yaml validate docs/markdown-plan-compiler.md
```

Watch a plan and states file for changes:

```bash
cargo run -p rhei-cli -- validate --watch docs/markdown-plan-compiler.md
```

Render a plan as pretty JSON:

```bash
cargo run -p rhei-cli -- render docs/markdown-plan-compiler.md --format json --pretty
```

Render a plan as GitHub-style markdown without metadata or subtask body text:

```bash
cargo run -p rhei-cli -- render docs/markdown-plan-compiler.md --format github --no-metadata --no-content
```

Render a terminal progress report without ANSI color:

```bash
cargo run -p rhei-cli -- render docs/markdown-plan-compiler.md --format progress --no-color
```

Print crate versions surfaced by the CLI:

```bash
cargo run -p rhei-cli -- version
```

Reset a plan back to the initial state declared in its state machine:

```bash
cargo run -p rhei-cli -- --state-machine docs/specs/states.yaml reset docs/markdown-plan-compiler.md
```

## Library usage

Typical flow inside Rust code:

1. Parse markdown with `rhei_core::parse`
2. Load a states definition with `rhei_validator::StateMachine::from_yaml_file`
3. Validate with `rhei_validator::validate_with_machine` or `rhei_validator::validate_from_machine_file`
4. Render with helpers from `rhei_output`

## Status notes

This documentation reflects the current repository behavior. In particular:
- parsing retains rhei-level text and subtask body content
- validation enforces required `**State:**` metadata, dependency existence, metadata ordering, cycle detection, and subtask numbering checks
- rendering is available for JSON, GitHub-style markdown, and progress reports
- examples beyond repository documents are tracked separately by subtask 8.4
