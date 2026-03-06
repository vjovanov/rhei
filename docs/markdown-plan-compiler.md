# Saga: Markdown Plan Compiler

## Overview

Build a compiler for structured markdown plans that validates and processes hierarchical task documents. The compiler serves three primary purposes:

1. **GitHub Integration**: Reflect a hierarchy of GitHub/similar tickets
2. **AI Agent State Management**: Enable AI coding agents to maintain state and know minimal context while tracking progress
3. **Human Oversight**: Allow humans to oversee and manage agent work

## Implementation

- **Language**: Rust
- **Architecture**: Plugin-based design to allow future extensibility

## Format Specification

See [Plan Language Specification](plan-language-spec.md) for the formal grammar.

### Saga Level
- Must be top level and start with the token `# Saga: `
- Can have arbitrary structure for description, context, and requirements
- The last section must be `## Tasks` which contains the strict task structure

### Task Level
- Each task is a level-three heading starting with `### Task <id>: `
- Task IDs can be numbers (1, 2, 3) or identifiers (review, cleanup)
- Tasks can have metadata fields as the first lines after the heading
- Metadata fields are marked with `**Field:** value` syntax

### Subtask Level
- Subtasks are level-four headings under Tasks
- Must be prefixed with `#### Subtask <task-number>.<subtask-number>: `
- The subtask number includes its parent task number for clear hierarchy

### Metadata Fields

Only two metadata fields are supported:

1. **Prior**: `**Prior:** Task <nr1>, Task <nr2>, ...`
   - Lists task numbers that must be completed before this task
   - Compiler validates that referenced task numbers exist

2. **State**: `**State:** <state>`
   - Current state of the task
   - Compiler validates state consistency against states definition
   - This is a mandatory field.
   - State must be always first.
   - If it has spaces it must be escaped with ` `

## Example Plan Format

```markdown
# Saga: User Authentication System

## Overview
Implement a complete user authentication system with login, registration, and password reset capabilities.

## Requirements
- Secure password hashing
- JWT token-based authentication
- Email verification

## Tasks

### Task 1: Database Schema Design
**State:** completed

Design the database schema for user accounts and sessions.

#### Subtask 1.1: Define User Table
Create the users table with fields for email, password hash, and metadata.

#### Subtask 1.2: Define Session Table
Create the sessions table for tracking active user sessions.

### Task 2: Authentication API Endpoints
**State:** in-progress
**Prior:** Task 1

Implement the REST API endpoints for authentication.

#### Subtask 2.1: Login Endpoint
Implement POST /api/auth/login endpoint.

#### Subtask 2.2: Registration Endpoint
Implement POST /api/auth/register endpoint.

#### Subtask 2.3: Logout Endpoint
Implement POST /api/auth/logout endpoint.

### Task 4: Review the UX
**State:** completed

### Task 3: Frontend Integration
**State:** pending
**Prior:** Task 2

Integrate authentication into the frontend application.

#### Subtask 3.1: Login Form
Create the login form component.

#### Subtask 3.2: Registration Form
Create the registration form component.
```

## States

The states definition will be stored in a separate file and loaded by the compiler. A typical states file might include:

- `pending` - Task not yet started
- `in-progress` - Task currently being worked on
- `blocked` - Task blocked by dependencies or issues
- `completed` - Task finished
- `cancelled` - Task no longer needed

## Tasks

### Task 1: Project Setup and Architecture
**State:** pending

Set up the Rust project structure and define the overall architecture with a plugin-based design for future extensibility.

#### Subtask 1.1: Initialize Rust Project
Create a new Rust project using Cargo with appropriate workspace structure for multiple crates.

#### Subtask 1.2: Define Project Structure
Create the initial project structure with crates for:
- `rhei-core`: Lexer, parser, AST definitions
- `rhei-validator`: Semantic validation
- `rhei-cli`: Command-line interface
- `rhei-output`: Output generators

#### Subtask 1.3: Set Up Development Tooling
Configure Cargo workspace, add development dependencies (testing frameworks, linting), and set up CI configuration.

### Task 2: Lexer Implementation
**State:** pending
**Prior:** Task 1

Implement the lexical analyzer in Rust to tokenize markdown plan documents.

#### Subtask 2.1: Define Token Types
Define Rust enums for all token types needed for the plan format:
- `SagaHeader`
- `TasksSection`
- `TaskHeader { number: u32 }`
- `SubtaskHeader { task_number: u32, subtask_number: u32 }`
- `MetadataPrior { task_numbers: Vec<u32> }`
- `MetadataState { state: String }`
- `TextContent`

#### Subtask 2.2: Implement Tokenizer
Build the tokenizer using Rust iterators that converts raw markdown text into a stream of tokens.

#### Subtask 2.3: Handle Edge Cases
Handle edge cases like escaped characters, code blocks, and nested markdown elements using Rust's pattern matching.

### Task 3: Parser Implementation
**State:** pending
**Prior:** Task 2

Implement the parser in Rust to build an Abstract Syntax Tree from the token stream.

#### Subtask 3.1: Define AST Node Types
Define Rust structs for AST node types representing the plan structure:
- `Saga { title: String, content: Vec<ContentBlock>, tasks: Vec<Task> }`
- `Task { number: u32, title: String, metadata: TaskMetadata, subtasks: Vec<Subtask> }`
- `Subtask { task_number: u32, subtask_number: u32, title: String, content: String }`
- `TaskMetadata { depends_on: Vec<u32>, state: Option<String> }`

#### Subtask 3.2: Implement Recursive Descent Parser
Build the parser using recursive descent approach with Rust's Result type for error handling.

#### Subtask 3.3: Error Recovery
Implement error recovery using Rust's error handling patterns, providing span information for helpful error messages.

### Task 4: States Loader
**State:** completed
**Prior:** Task 1

Implement the states definition loader in Rust.

#### Subtask 4.1: Define States File Format
Use YAML format for states definitions, leveraging the `serde` and `serde_yaml` crates.

#### Subtask 4.2: Implement States Parser
Build the deserializer for states definition files using serde derive macros.

#### Subtask 4.3: State Transition Validation
Implement logic using Rust traits to validate state transitions according to the states definition.

### Task 5: Semantic Validator
**State:** completed
**Prior:** Task 3, Task 4

Implement semantic validation on the parsed AST using Rust's type system.

#### Subtask 5.1: Dependency Validation
Validate that all task numbers referenced in `Prior` fields exist in the plan using HashMap lookups.

#### Subtask 5.2: State Consistency Validation
Validate that task states are consistent with the loaded states definition.

#### Subtask 5.3: Circular Dependency Detection
Detect and report circular dependencies using graph algorithms (topological sort).

#### Subtask 5.4: Task Numbering Validation
Validate that task and subtask numbers follow the expected format (subtask numbers must include parent task number).

### Task 6: Output Generators
**State:** pending
**Prior:** Task 5

Implement output generators for different use cases using Rust traits for extensibility.

#### Subtask 6.1: JSON Output
Generate JSON representation of the plan using `serde_json` for programmatic access.

#### Subtask 6.2: GitHub Issues Output
Generate output suitable for creating GitHub issues and linking them hierarchically (markdown format).

#### Subtask 6.3: Progress Report Output
Generate human-readable progress reports showing task states and dependencies using colored terminal output.

### Task 7: CLI Interface
**State:** pending
**Prior:** Task 6

Build the command-line interface for the compiler using `clap`.

#### Subtask 7.1: Argument Parsing
Implement command-line argument parsing using the `clap` crate with derive macros.

#### Subtask 7.2: Error Reporting
Implement user-friendly error reporting with line numbers and context using the `miette` or `ariadne` crate.

#### Subtask 7.3: Watch Mode
Implement watch mode using the `notify` crate for continuous validation during editing.

### Task 8: Testing and Documentation
**State:** pending
**Prior:** Task 7

Comprehensive testing and documentation using Rust conventions.

#### Subtask 8.1: Unit Tests
Write unit tests for lexer, parser, and validator components using Rust's built-in test framework.

#### Subtask 8.2: Integration Tests
Write integration tests with sample plan documents in the `tests/` directory.

#### Subtask 8.3: Documentation
Write user documentation and API documentation using `rustdoc` conventions.

#### Subtask 8.4: Example Plans
Create example plan documents demonstrating all features in an `examples/` directory.
