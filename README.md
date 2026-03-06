# Rhei

Rhei is a Rust workspace for parsing, validating, and rendering structured markdown plans.

Current workspace crates:
- `rhei-core`: AST types plus markdown plan parsing
- `rhei-validator`: semantic validation against a YAML state machine
- `rhei-output`: JSON, GitHub-style markdown, and progress-report rendering
- `rhei-cli`: `rhei` command for validation and rendering
- `rhei-napi`: Node.js bindings

## Markdown plan compiler

The markdown plan compiler currently supports:
- parsing saga/task/subtask structure from markdown plans
- validating task metadata and dependencies against a state machine in [`docs/state-machine.yaml`](docs/state-machine.yaml)
- rendering parsed plans as JSON, GitHub-style markdown, or terminal-oriented progress output

The primary reference documents are:
- [`docs/markdown-plan-compiler.md`](docs/markdown-plan-compiler.md) — saga and task breakdown
- [`docs/plan-language-spec.md`](docs/plan-language-spec.md) — plan language rules
- [`docs/state-machine.yaml`](docs/state-machine.yaml) — default validation state machine

## CLI usage

Validate a plan with the default state machine:

```bash
cargo run -p rhei-cli -- validate docs/markdown-plan-compiler.md
```

Validate using a specific state machine file:

```bash
cargo run -p rhei-cli -- --state-machine docs/state-machine.yaml validate docs/markdown-plan-compiler.md
```

Watch a plan and state machine for changes:

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

## Library usage

Typical flow inside Rust code:

1. Parse markdown with `rhei_core::parse`
2. Load a state machine with `rhei_validator::StateMachine::from_yaml_file`
3. Validate with `rhei_validator::validate_with_machine` or `rhei_validator::validate_from_machine_file`
4. Render with helpers from `rhei_output`

## Status notes

This documentation reflects the current repository behavior. In particular:
- parsing retains saga-level text and subtask body content
- validation enforces required `**State:**` metadata, dependency existence, metadata ordering, cycle detection, and subtask numbering checks
- rendering is available for JSON, GitHub-style markdown, and progress reports
- examples beyond repository documents are tracked separately by subtask 8.4
