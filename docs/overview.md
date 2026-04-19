# Rhei Overview

Rhei is a structured markdown plan system for hierarchical task management. It enables parsing, validation, execution, and rendering of Rhei plans with support for formal state transitions.

## Purpose

Rhei serves three primary use cases:

1. **GitHub/Ticket Integration** — Reflect a hierarchy of tickets with dependencies and states
2. **AI Agent State Management** — Enable coding agents to maintain state and track progress with minimal context
3. **Human Oversight** — Allow humans to oversee, review, and manage automated work

## Quick Start

### CLI Installation

Build and run the CLI from source:

```bash
cargo build -p rhei-cli --release
```

### Basic Usage

Validate a plan file:

```bash
rhei validate path/to/plan.rhei.md
```

Watch for changes:

```bash
rhei validate --watch path/to/plan.rhei.md
```

Render a plan as JSON for other tools:

```bash
rhei render path/to/plan.rhei.md --format json --pretty
```

See the [README](../README.md) for complete CLI options.

---

## Core Concepts

### Plans

A **Rhei plan** is a structured markdown document representing a project or workflow. Each plan contains:

- A title (`# Rhei: <title>`)
- Optional content sections (overview, requirements, etc.)
- A `## Tasks` section with hierarchical task definitions

### Tasks and Subtasks

**Tasks** are the primary work units within a saga. Each task has:

- A unique identifier (numeric or named)
- A mandatory state
- Optional dependencies on other tasks
- Optional subtasks for detailed breakdown

**Subtasks** provide finer granularity within tasks. They are numbered relative to their parent task (e.g., Subtask 2.1, 2.2 for Task 2).

### States and Transitions

Tasks progress through defined **states** (for example `draft` → `pending` → `agent-review` → `completed`). The state machine can be:

- Simple: A flat list of valid states for validation
- Formal: Full transition rules with callbacks for automation

---

## Specification Documents

### Language and Format

| Document | Description |
|----------|-------------|
| [Plan Language Specification](rhei.spec.md) | Formal EBNF grammar, token types, and semantic constraints for Rhei plan documents |
| [States Specification](states-spec.md) | Basic states configuration format |
| [Formal State Transitions](formal-state-transitions.md) | Advanced state machine with transitions, callbacks, and multi-platform integration |

### Reference Files

| File | Description |
|------|-------------|
| [states.yaml](specs/states.yaml) | Default states definition used for validation |
| [release-automation.rhei.md](../examples/release-automation.rhei.md) | A checked-in example plan |

### Examples

The [`examples/`](../examples/) directory contains working plan documents:

| Example | Features Demonstrated |
|---------|----------------------|
| [`release-automation.rhei.md`](../examples/release-automation.rhei.md) | Mixed task IDs, dependencies, code blocks |
| [`human-review-loop.rhei.md`](../examples/human-review-loop.rhei.md) | Review states, dependency chains |
| [`escaped-state-values.rhei.md`](../examples/escaped-state-values.rhei.md) | States with spaces, custom states files |

See [`examples/README.md`](../examples/README.md) for verification commands.

---

## Architecture

Rhei is structured as a Rust workspace with focused crates:

```
rhei/
├── crates/
│   ├── rhei-core/       # Lexer, parser, AST types
│   ├── rhei-validator/  # Semantic validation
│   ├── rhei-output/     # JSON, markdown, progress rendering
│   ├── rhei-cli/        # Command-line interface
│   └── rhei-napi/       # Node.js bindings
└── docs/
```

### Crate Responsibilities

| Crate | Role |
|-------|------|
| `rhei-core` | Tokenizes markdown, parses into AST, defines data structures |
| `rhei-validator` | Validates dependencies, states, cycles, subtask numbering |
| `rhei-output` | Renders AST to JSON, GitHub markdown, terminal progress |
| `rhei-cli` | Provides `validate`, `render`, and `version` commands |
| `rhei-napi` | Exposes Rust functionality to JavaScript via N-API |

### Processing Pipeline

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Markdown   │────▶│   Lexer     │────▶│   Parser    │────▶│    AST      │
│   Input     │     │  (tokens)   │     │  (rhei-core)│     │             │
└─────────────┘     └─────────────┘     └─────────────┘     └──────┬──────┘
                                                                   │
                    ┌─────────────┐     ┌─────────────┐            │
                    │   Output    │◀────│  Validator  │◀───────────┘
                    │ (render)    │     │ (semantic)  │
                    └─────────────┘     └─────────────┘
```

---

## Library Usage

For programmatic integration in Rust:

```rust
use rhei_core::parse;
use rhei_validator::{StateMachine, validate_with_machine};
use rhei_output::{render_json, render_github, render_progress};

// 1. Parse markdown into AST
let saga = parse(markdown_content)?;

// 2. Load state machine
let machine = StateMachine::from_yaml_file("states.yaml")?;

// 3. Validate
let errors = validate_with_machine(&saga, &machine);

// 4. Render
let json = render_json(&saga, true)?;  // pretty print
let github = render_github(&saga, options)?;
let progress = render_progress(&saga, options)?;
```

---

## Formal State Transitions

For workflows requiring automation, Rhei supports formal state transitions with callbacks. This enables:

- Declarative transition rules in YAML
- Pre/post transition callbacks (`on_leave`, `on_enter`)
- Conditional transitions and timeouts
- Multi-platform execution (CLI/bash, Node.js, Python, Java)

See [Formal State Transitions](formal-state-transitions.md) for:

- [`TransitionContext`](formal-state-transitions.md#transitioncontext-data-structure) — Data passed to callbacks
- [YAML State Machine Format](formal-state-transitions.md#yaml-state-machine-format-specification) — Configuration schema
- [Platform Examples](formal-state-transitions.md#example-2-cli-integration-with-bash-functions) — CLI, JavaScript, Python, Java integrations

---

## Document Conventions

### File Extensions

- `.rhei.md` — Rhei plan documents
- `.yaml` — States and state machine definitions

### State Values

Single-word states are written directly:

```markdown
**State:** pending
```

Multi-word states require backtick escaping:

```markdown
**State:** `in progress`
```

### Task References

Dependencies use the `Task <id>` format:

```markdown
**Prior:** Task 1, Task 2
```

---

## Related Resources

- [README](../README.md) — Project overview and CLI reference
- [AGENTS.md](../AGENTS.md) — CI verification commands for contributors
- [Cargo.toml](../Cargo.toml) — Workspace configuration
