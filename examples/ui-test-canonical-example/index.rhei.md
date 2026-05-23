# Rhei: Rhei UI Canonical Test
**States:** ui-test-canonical

---
structure:
  maxLevels: 3
  nodeKinds: [task]
---

## Overview

This workspace is a deterministic UI exercise for `dashboard checkout flow`. It uses
mock agents and mock scripts only, so the workflow can be run repeatedly while
the Rhei UI renders live slots, logs, artifacts, dependency blocking, generated
follow-up work, counted visits, polling, human gates, terminal rows, and a
three-level task tree.

## Runtime Shape

- Mock agents are defined by `.rhei/settings.json` and implemented by
  `bin/mock-agent.sh`.
- Mock programs and callbacks live in `bin/mock-program.sh` and
  `bin/mock-transition.sh`.
- Runtime outputs are written under `runtime/`, with per-task artifact folders,
  review fan-out files, aggregate reports, snapshots, and transition logs.