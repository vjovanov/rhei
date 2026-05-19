# AR-agent-orchestrator-workflow: Agent-Orchestrator Workflow Architecture

This document describes how a user-directed agent creates a Rhei plan, validates
and fixes syntax, and passes it to the orchestrator for state-managed execution.
It expands the plan language, state machine, transition, and run-command
contracts into the component workflow they imply. §FS-rhei-plan-language
§FS-rhei-states §FS-rhei-transitions §FS-rhei-run

## 1. High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                              USER-DIRECTED AGENT                                     │
│  ┌─────────────┐    ┌──────────────────┐    ┌──────────────────┐                   │
│  │   User      │───▶│  Agent (e.g.,    │───▶│  Generate Rhei   │                   │
│  │   Request   │    │  Claude/Kilo)    │    │  Plan (.rhei.md) │                   │
│  └─────────────┘    └──────────────────┘    └────────┬─────────┘                   │
└────────────────────────────────────────────────────────┼───────────────────────────┘
                                                         │
                                                         ▼
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                           SYNTAX VALIDATION & REPAIR LOOP                           │
│                                                                                     │
│  ┌──────────────┐    ┌──────────────────┐    ┌──────────────────┐                  │
│  │  rhei-core   │───▶│  rhei-validator  │───▶│  Validation      │                  │
│  │  (Lexer +    │    │  (Semantic       │    │  Result          │                  │
│  │   Parser)    │    │   Checks)        │    │                  │                  │
│  └──────────────┘    └──────────────────┘    └────────┬─────────┘                  │
│         ▲                                             │                            │
│         │            ┌──────────────────┐             │                            │
│         └────────────│  Agent Fixes     │◀────────────┘                            │
│           (if errors)│  Syntax Errors   │    (errors returned)                     │
│                      └──────────────────┘                                          │
└────────────────────────────────────────────────────────────────────────────────────┘
                                                         │
                                                         │ (valid AST)
                                                         ▼
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                               ORCHESTRATOR ENGINE                                    │
│                                                                                     │
│  ┌──────────────────────────────────────────────────────────────────────────────┐  │
│  │                         State Machine (YAML)                                  │  │
│  │  ┌─────────┐    ┌─────────────┐    ┌─────────┐    ┌───────────┐              │  │
│  │  │ pending │───▶│ in-progress │───▶│ review  │───▶│ completed │              │  │
│  │  └─────────┘    └─────────────┘    └─────────┘    └───────────┘              │  │
│  └──────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
│  ┌──────────────────────────────────────────────────────────────────────────────┐  │
│  │                    Transition Management                                      │  │
│  │                                                                               │  │
│  │   1. Find ready tasks (dependencies satisfied)                               │  │
│  │   2. Trigger on_leave callback                                               │  │
│  │   3. Update task state in .rhei.md                                           │  │
│  │   4. Trigger on_enter callback                                               │  │
│  │   5. Handle callback results (success/redirect/reject)                       │  │
│  │   6. Loop until all tasks reach final states                                 │  │
│  │                                                                               │  │
│  └──────────────────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Detailed Sequence Diagram

```mermaid
sequenceDiagram
    participant User
    participant Agent as User-Directed Agent
    participant Rhei as Rhei Core<br/>(Lexer + Parser)
    participant Validator as Rhei Validator
    participant Orch as Orchestrator
    participant SM as State Machine
    participant CB as Callbacks

    %% Phase 1: Plan Creation
    rect rgb(240, 248, 255)
        Note over User,Agent: Phase 1: Plan Creation
        User->>Agent: Describe project/workflow
        Agent->>Agent: Analyze requirements
        Agent->>User: Propose plan structure
        User->>Agent: Approve/refine plan
        Agent->>Agent: Generate .rhei.md plan
    end

    %% Phase 2: Validation Loop
    rect rgb(255, 250, 240)
        Note over Agent,Validator: Phase 2: Validation & Repair Loop
        loop Until Valid
            Agent->>Rhei: Submit plan for parsing
            Rhei->>Rhei: Tokenize (Lexer)
            Rhei->>Rhei: Parse to AST (Parser)

            alt Parse Error
                Rhei-->>Agent: Syntax errors with spans
                Agent->>Agent: Fix syntax issues
            else Parse Success
                Rhei->>Validator: Pass AST
                Validator->>Validator: Check dependency integrity
                Validator->>Validator: Validate state values
                Validator->>Validator: Detect cycles (DAG check)
                Validator->>Validator: Verify child task id numbering

                alt Validation Errors
                    Validator-->>Agent: Semantic errors
                    Agent->>Agent: Fix semantic issues
                else Validation Success
                    Validator-->>Agent: ✓ Plan is valid
                end
            end
        end
    end

    %% Phase 3: Orchestration
    rect rgb(240, 255, 240)
        Note over Orch,CB: Phase 3: Orchestrator State Management
        User->>Agent: Approve plan execution
        Agent->>Orch: Execute plan (rhei.run())
        Note over User,Agent: User & Agent coordinate execution
        Orch->>SM: Load state machine (YAML)

        loop While tasks remain non-final
            Orch->>Orch: Find ready tasks<br/>(deps satisfied, non-final)

            alt No ready tasks
                Orch->>Orch: Wait for external trigger<br/>or condition/timeout
            else Ready task found
                Note over Orch,CB: Transition: current_state → target_state

                %% on_leave
                Orch->>SM: Validate transition allowed
                SM-->>Orch: TransitionRule
                Orch->>CB: Invoke on_leave(ctx)

                alt Callback rejects
                    CB-->>Orch: {success: false, error}
                    Orch->>Orch: Task stays in current state
                else Callback redirects
                    CB-->>Orch: {success: true, nextState: X}
                    Orch->>SM: Validate redirect X allowed
                else Callback approves
                    CB-->>Orch: {success: true, data}
                end

                %% State update
                Orch->>Orch: Update task state in .rhei.md

                %% on_enter
                Orch->>CB: Invoke on_enter(ctx)

                alt on_enter fails
                    CB-->>Orch: {success: false}
                    Orch->>Orch: Rollback state
                    Orch->>Orch: Apply error_handling policy
                else on_enter succeeds
                    CB-->>Orch: {success: true}
                    Orch->>Orch: Transition complete
                end
            end
        end

        Orch-->>Agent: All tasks in final states
    end

    Agent-->>User: Workflow complete
```

---

## 3. Component Responsibilities

### 3.1. User-Directed Agent

The agent (e.g., a coding assistant like Claude) interprets user intent and generates structured plans:

| Responsibility | Description |
|----------------|-------------|
| **Interpret Requirements** | Understand user's project goals and constraints |
| **Generate Plan** | Create a `.rhei.md` file following the [Plan Language Specification](../functional-spec/rhei-plan-language.spec.md) |
| **Fix Errors** | Iteratively correct syntax and semantic errors until validation passes |
| **Monitor Progress** | Track task completion and adjust plans as needed |

### 3.2. Validation Pipeline

The validation pipeline ensures plan correctness before execution:

```
┌─────────────────────────────────────────────────────────────────┐
│                      rhei-core                                   │
├─────────────────────────────────────────────────────────────────┤
│  Lexer (lexer.rs)                                               │
│  ├── Tokenizes markdown into structured tokens                  │
│  ├── Identifies: RheiHeader, TaskHeader, MetadataState, etc.   │
│  └── Produces token stream with span information                │
│                                                                  │
│  Parser (parser.rs)                                              │
│  ├── Consumes token stream                                       │
│  ├── Builds AST (Plan → recursive Task tree)                    │
│  └── Reports parse errors with line/column info                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    rhei-validator                                │
├─────────────────────────────────────────────────────────────────┤
│  Semantic Checks:                                                │
│  ├── Dependency integrity (all Prior refs exist)                │
│  ├── State validity (states match states.yaml)                  │
│  ├── Acyclic check (DAG via topological sort)                   │
│  └── Child task ids (Task N.M under Task N; depth ≤ maxLevels)  │
└─────────────────────────────────────────────────────────────────┘
```

### 3.3. Orchestrator Engine

The orchestrator manages workflow execution through state transitions:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Orchestrator Engine                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────────┐    ┌──────────────────┐                   │
│  │  Task Scheduler  │    │  State Machine   │                   │
│  │                  │    │                  │                   │
│  │ • Find ready     │◀──▶│ • Load YAML      │                   │
│  │   tasks          │    │ • Validate       │                   │
│  │ • Check deps     │    │   transitions    │                   │
│  │ • Queue work     │    │ • Track states   │                   │
│  └──────────────────┘    └──────────────────┘                   │
│           │                       │                              │
│           ▼                       ▼                              │
│  ┌──────────────────────────────────────────────────────┐       │
│  │              Transition Executor                      │       │
│  │                                                       │       │
│  │  1. on_leave(ctx) → validate exit from current       │       │
│  │  2. Update .rhei.md file with new state              │       │
│  │  3. on_enter(ctx) → initialize in new state          │       │
│  │  4. Handle: success / redirect / rejection / error   │       │
│  └──────────────────────────────────────────────────────┘       │
│                              │                                   │
│                              ▼                                   │
│  ┌──────────────────────────────────────────────────────┐       │
│  │              Callback Dispatcher                      │       │
│  │                                                       │       │
│  │  Platform-specific invocation:                        │       │
│  │  • CLI:     bash functions (stdin/stdout JSON)       │       │
│  │  • Node.js: NAPI native callbacks                    │       │
│  │  • Python:  PyO3 bindings                            │       │
│  │  • Java:    JNI method calls                         │       │
│  └──────────────────────────────────────────────────────┘       │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 4. State Transition Flow

The orchestrator advances tasks through states based on the state machine definition:

```
                          ┌─────────────────────────────────────┐
                          │         State Machine YAML          │
                          │                                     │
                          │  states:                            │
                          │    pending:     {}                  │
                          │    in-progress: {}                  │
                          │    review:      {}                  │
                          │    completed:   {final: true}       │
                          │                                     │
                          │  transitions:                       │
                          │    - from: pending                  │
                          │      to: in-progress               │
                          │      on_leave: validate_deps       │
                          │      on_enter: start_work          │
                          │    ...                              │
                          │                                     │
                          │  profiles:                          │
                          │    default:                         │
                          │      initial: pending               │
                          │      allowed: [pending,             │
                          │        in-progress, review,         │
                          │        completed]                   │
                          │  node_policy:                       │
                          │    root: default                    │
                          │    default: default                 │
                          └───────────────┬─────────────────────┘
                                          │
                                          ▼
    ┌─────────────────────────────────────────────────────────────────────┐
    │                    Task Lifecycle Example                            │
    │                                                                      │
    │   Task 2: Implement Feature                                          │
    │   **Prior:** Task 1                                                  │
    │                                                                      │
    │   ┌─────────┐  deps met   ┌─────────────┐  work done  ┌─────────┐   │
    │   │ pending │────────────▶│ in-progress │────────────▶│ review  │   │
    │   └─────────┘             └─────────────┘             └────┬────┘   │
    │        │                                                   │        │
    │        │                                       ┌───────────┴───┐    │
    │        │                                       │               │    │
    │        ▼                                  approved        changes   │
    │   Waiting for                                  │          needed    │
    │   Task 1 to                                    ▼               │    │
    │   complete                              ┌───────────┐          │    │
    │                                         │ completed │          │    │
    │                                         └───────────┘          │    │
    │                                                                │    │
    │                                         ◀──────────────────────┘    │
    │                                         (back to in-progress)       │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 5. Trigger Types

The orchestrator responds to different trigger sources:

| Trigger | `triggeredBy` | Description |
|---------|---------------|-------------|
| **User** | `'user'` | Explicit API call (CLI command, programmatic transition) |
| **Callback** | `'callback'` | Callback returns `nextState` override |
| **System** | `'system'` | Condition met or timeout elapsed |
| **Engine** | `'engine'` | Orchestrator auto-advances ready tasks during `rhei.run()` |

---

## 6. Error Handling

```
┌─────────────────────────────────────────────────────────────────┐
│                     Error Scenarios                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  on_leave Rejection (success: false)                            │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  • Task remains in current state                          │   │
│  │  • Error message logged/returned                          │   │
│  │  • No state file modification                             │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  on_enter Failure                                                │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  1. State is rolled back to original                      │   │
│  │  2. error_handling.on_enter_failure policy applied        │   │
│  │  3. May trigger transition to 'retrying' state            │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  Invalid Redirect                                                │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  • Callback returns nextState not in state machine        │   │
│  │  • TransitionForbiddenError raised                        │   │
│  │  • Task remains in current state                          │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Summary

1. **Agent Creates Plan**: User-directed agent generates a `.rhei.md` file with hierarchical tasks
2. **Validation Loop**: Rhei lexer/parser and validator check syntax and semantics; agent fixes any errors
3. **Orchestrator Executes**: Once valid, the orchestrator loads the state machine and manages transitions
4. **State Progression**: Tasks advance through states via callbacks (`on_leave` → state update → `on_enter`)
5. **Completion**: Workflow finishes when all tasks reach final states (`completed`, `cancelled`, etc.)

## Related Documentation

- [Plan Language Specification](../functional-spec/rhei-plan-language.spec.md) — Formal EBNF grammar
- [States Specification](../functional-spec/rhei-states.spec.md) — State machine format and default states
- [Transitions Specification](../functional-spec/rhei-transitions.spec.md) — Advanced state machine with callbacks
- [Run Specification](../functional-spec/rhei-run.spec.md) — Orchestrated execution loop
- [Overview](overview.md) — Project architecture and crate responsibilities
