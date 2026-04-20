# 0002 - Replace `Subtask` with hierarchical task nodes and configurable node kinds

## Status

proposed

## Context

Rhei currently hard-codes a two-level planning model:

- top-level `Task` nodes
- second-level `Subtask` nodes

That split is no longer a good fit for the implementation or for the authoring
model we want.

First, the implementation already treats subtasks as more than lightweight
checklist items. Subtasks carry `**State:**`, appear in agent prompts, are
reset by `rhei reset`, and block `rhei complete` while they remain
non-terminal. In practice, subtasks are already "small tasks" with a special
name.

Second, the fixed `Task`/`Subtask` split prevents deeper decomposition. A plan
author can break work into one extra level, but not into a third or fourth
level when a task needs progressive disclosure.

Third, the language currently bakes structural meaning into the word
`Subtask`. That makes it difficult to represent alternative node kinds such as
bugs without inventing a second parallel abstraction.

Finally, the current model spreads the same concept across multiple code paths:
the parser, AST, validator, JSON output, CLI rendering, completion checks, and
reset logic all distinguish `Task` from `Subtask`. That duplication is
artificial if both are really workflow nodes with state and content.

## Decision

Replace the fixed `Task`/`Subtask` model with a recursive task-tree model.

### 1. Semantic model

Rhei should represent one structural concept: a task node.

Each node has:

- a hierarchical id
- a title
- a state
- optional dependencies
- optional assignee
- optional free-form content
- an optional node kind
- zero or more child task nodes

The AST should therefore move from:

- `Task { ..., subtasks: Vec<Subtask> }`

to a recursive shape such as:

```rust
struct TaskNode {
    id: NodeId,
    title: String,
    state: String,
    prior: Vec<NodeId>,
    assignee: Option<String>,
    kind: String,
    content: String,
    result: Option<ResultLink>,
    children: Vec<TaskNode>,
}
```

`Rhei.tasks` remains the list of root nodes.

### 2. Hierarchical ids

The fixed `task_number` / `subtask_number` pair should be replaced by a
general hierarchical id.

- Top-level ids remain unchanged: `1`, `api`, `fix-cache-key`
- Child ids use dotted paths: `1.1`, `1.2.3`, `api.cache`, `api.cache.fix`

Each segment should reuse the existing task-id segment rules:

- numeric segment: `NUMBER`
- named segment: `IDENTIFIER`

This preserves today's top-level ids while allowing arbitrary hierarchy.

### 3. Configurable depth

Plan structure should declare the maximum allowed task depth in plan metadata,
not in the state machine.

Proposed metadata shape:

```yaml
structure:
  maxLevels: 3
```

Rationale:

- hierarchy depth is a property of the plan language, not the workflow state
  machine
- the same state machine should work across shallow and deep plans
- structure belongs with other plan-scoped authoring choices

In markdown-backed plans, `maxLevels` is practically capped by heading depth:

- `###` = level 1 task
- `####` = level 2 task
- `#####` = level 3 task
- `######` = level 4 task

So the initial implementation should support `1..=4` levels. If deeper trees
are needed later, they should use a new syntax rather than overloading heading
levels past Markdown's practical limit.

### 4. Remove `Subtask` syntax

`Subtask` should disappear from the normative model.

A child node is simply another task node at a deeper level:

```markdown
### Task 1: Release readiness
**State:** pending

#### Task 1.1: Verify release notes
**State:** pending

##### Task 1.1.1: Compare changelog and notes
**State:** pending
```

For migration, the parser should temporarily accept legacy
`#### Subtask <path>: ...` as an alias for a level-2 task node and emit a
deprecation warning.

### 5. Configurable node kinds

Task hierarchy and node kind should be orthogonal.

Every node remains a task node structurally, but it may be tagged with a
configurable kind such as `task` or `bug`.

Proposed metadata shape:

```yaml
structure:
  maxLevels: 3
  nodeKinds:
    - task
    - bug
```

Node kind should be parsed from the heading keyword, using the configured
`structure.nodeKinds` set:

```markdown
#### Bug 1.2: Fix null-cache panic
**State:** pending
```

Parsing rule:

- the parser reads `structure.nodeKinds`
- each declared kind enables a corresponding heading keyword
- matching is case-insensitive in configuration, but headings render in Title
  Case by convention (`task` -> `Task`, `bug` -> `Bug`)
- when `structure.nodeKinds` is omitted, the default is `[task]`

This makes kind part of the authored syntax instead of redundant metadata, and
it lets the parser determine node kind directly from the heading line.

### 6. Execution semantics

In the first version of this change, only leaf nodes should be claimable by
`rhei next` and `rhei run`.

Non-leaf nodes remain useful as:

- decomposition containers
- roll-up descriptions
- result anchors for larger work packages

This avoids ambiguous behavior where both a parent and its children are
simultaneously runnable.

Validation should generalize today's parent/subtask checks into tree checks:

- child ids must extend the parent id path
- sibling ids must be unique
- node depth must not exceed `structure.maxLevels`
- a terminal ancestor may not contain non-terminal descendants
- links inside child-node content are validated the same way as task content

### 7. Dependency references

`**Prior:**` should resolve against any task node, not only roots.

Dependencies should reference the full hierarchical id, and may use any
declared node-kind keyword:

```markdown
#### Task 2.2: Ship the fix
**State:** pending
**Prior:** Bug 1.2, Task 1.3.1
```

The referenced kind must match the declared kind of the target node.

### 8. Migration plan

Deliver the change in phases:

1. Add recursive AST support and compatibility parsing for `Subtask`
2. Rename validator logic from subtask-specific rules to generic tree rules
3. Switch CLI/output/rendering code from `subtasks` to `children`
4. Add node-kind parsing/validation and JSON output for `kind`
5. Deprecate authored `Subtask` syntax in docs and examples
6. Remove `Subtask` compatibility parsing in the next major version

## Consequences

- The language matches the implementation more honestly: current subtasks are
  already stateful workflow nodes, and this proposal removes the naming fiction.
- Plans can use progressive disclosure beyond two levels without inventing
  external linked plans or fake top-level tasks.
- Bugs become first-class plan nodes via parsed heading kind, without needing a
  separate workflow mechanism.
- `rhei next`, `rhei run`, validators, renderers, and JSON output all need a
  breaking internal refactor from `subtasks` to recursive `children`.
- Existing plans can be supported with a compatibility window because
  `Subtask 1.2` maps naturally to `Task 1.2`.
- The first implementation intentionally limits hierarchy depth to what Markdown
  heading levels can represent cleanly.
