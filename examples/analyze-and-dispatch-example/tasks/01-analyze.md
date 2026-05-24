### Task analyze: Analyze docs/functional-spec and dispatch work
**State:** analyze

Analyze `docs/functional-spec` against the brief below, then create one follow-up task
per work item you find (up to 6). Follow the `analyze` state
instructions for the exact task-file format and dependency wiring — new tasks
are added by writing `tasks/NN-<slug>.md` files into this workspace.

**Analysis brief:**

Read each *.spec.md file under the subject directory. For every spec that does
NOT already reference a runnable example or fixture, create one work item to
add or link an example for it. One task per spec; skip specs that already link
an example. Use the spec's base filename as the task slug.
