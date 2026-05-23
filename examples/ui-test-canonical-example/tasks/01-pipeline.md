### Task full-pipeline: Walk the complete agent and program pipeline
**State:** human-gate

Tests: agent + program states, artifact input/output contracts, a single
`target`, snapshot `emit` (`on: success`), the `all_targets` review fan-out, the
counted `visits` fix-loop, transition `on_leave`/`on_enter` callbacks, the
callback-generated follow-up expansion, and the terminal human gate — one task
that walks the entire main path.

#### Task full-pipeline.dependency-blocking: Block until a prior task reaches a terminal state
**State:** completed
**Prior:** Task polling

Tests: `Prior` dependency edges and ready-set gating across task branches (this
branch cannot start until `polling` completes).

##### Task full-pipeline.dependency-blocking.three-level-nesting: Render a third hierarchy level
**State:** completed

Tests: nested task rendering at depth three.

###### Task full-pipeline.dependency-blocking.three-level-nesting.four-level-nesting: Render a fourth hierarchy level
**State:** completed

Tests: `structure.maxLevels: 4` and depth-four hierarchy rendering.

#### Task full-pipeline.snapshot-inherit-ancestor: Inherit an ancestor's implementation snapshot
**State:** completed

Tests: snapshot `inherit` with `from: ancestor` — selecting the parent
`full-pipeline` task's `mock-implement` snapshot via `select` — together with
`emit on: always`.