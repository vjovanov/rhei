### Task intake: Collect canonical UI scenario inputs
**State:** collect-inputs

Seed the mock run with deterministic inputs for the canonical UI scenario.

#### Task intake.context: Capture dashboard context
**State:** script-check

Produce a nested intake check so the UI renders executable child work.

##### Task intake.context.assets: Inventory fixture assets
**State:** script-check

Run a grandchild smoke check under the intake branch.

#### Task intake.warning: Reproduce a seeded warning path
**State:** script-check

Run a deterministic warning check that completes through a program state.
