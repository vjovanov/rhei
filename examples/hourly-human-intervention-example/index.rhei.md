# Rhei: Hourly Human Intervention Sweep
**States:** hourly-human-intervention

## Overview
Run this workspace once per hour to drain `human-intervention` work from
`oracle/graalvm-reachability-metadata`.

The sweep has two entry tasks:

- open GitHub issues carrying `human-intervention`
- open GitHub pull requests carrying `human-intervention`

Each entry task fetches the full queue, classifies items into root-cause
classes, and appends one child task per class. CI failure items are split into
one child task per issue or PR so each can be restarted, rechecked, and either
completed as transient/infra or routed into the fix pipeline. Each non-CI class
task then runs the shared fix state machine:

1. deep cause analysis
2. routing decision
3. one of:
   - CI failure triage, restart/rerun, and completion or routing
   - human GitHub issue/comment handoff
   - Forge fix, review, fix, review, publish/add reviewers
   - GraalVM proposal, human review, fix, review, fix, review, publish/add reviewers

Use `/home/vjovanov/c/rhei/master` for Forge and reachability-metadata fixes. Use
`/home/vjovanov/c/rhei/graalvm/ce` and `/home/vjovanov/c/rhei/graalvm/ee` for GraalVM code
changes; `/home/vjovanov/c/rhei/graalvm` is only the coordination directory containing
those checkouts. Refresh the local GraalVM from
`/home/vjovanov/c/rhei/graalvm/ee/vm-enterprise` with `mxb ee` before validations that
depend on the GraalVM build.