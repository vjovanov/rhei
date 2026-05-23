### Task feature: Execute the primary mock feature pipeline
**State:** collect-inputs
**Prior:** Task intake.warning

Run the full mock agent/script/review/fix pipeline after an intake smoke check
reaches a terminal state.

#### Task feature.api: Build API fixture branch
**State:** script-check
**Prior:** Task intake.context

Exercise a nested program task that depends on a sibling branch from the intake task.

##### Task feature.api.contract: Verify API contract smoke output
**State:** script-check

Complete a grandchild check so hierarchy rendering reaches three levels.

#### Task feature.ui: Build UI fixture branch
**State:** script-check
**Prior:** Task feature.api

Exercise dependency blocking between nested task branches with a deterministic check.

##### Task feature.ui.copy: Verify UI copy fixture
**State:** script-check

Run a grandchild check under the UI branch.