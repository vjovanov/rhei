# Rhei: {{plan_title}}
**States:** ui-test-canonical

---
structure:
  maxLevels: 4
  nodeKinds: [task]
metadata:
  tasks:
    terminal-completed:
      note: "seeded terminal example, pre-set in frontmatter metadata"
---

## Overview

This workspace is a deterministic UI exercise for `{{scenario_name}}`. It uses
mock agents and mock scripts only, so the workflow can be run repeatedly while
the Rhei UI renders live slots, logs, artifacts, dependency blocking, generated
follow-up work, counted visits, polling, live failures, human gates, terminal
rows, and a four-level task tree. Each task is named after the Rhei feature it
exercises; see the template README for the task-to-feature coverage matrix.

## Runtime Shape

- Mock agents are defined by `.agents/rhei/settings.json` and implemented by
  `bin/mock-agent.sh`.
- Mock programs and callbacks live in `bin/mock-program.sh` and
  `bin/mock-transition.sh`.
- Runtime outputs are written under `runtime/`, with per-task artifact folders,
  review fan-out files, aggregate reports, snapshots, and transition logs.
