# Rhei: Add Health Check

## Context

Add a small HTTP health endpoint and keep verification scoped to the changed service.

## Tasks

### Task 1: Inspect service routing
**State:** draft

Find the existing route registration point, identify the service's test pattern, and note any conventions the implementation must follow.

### Task 2: Implement health endpoint
**State:** draft
**Prior:** Task 1

Add a `GET /healthz` endpoint that returns a successful status and a compact JSON payload suitable for monitoring.

### Task 3: Verify endpoint behavior
**State:** draft
**Prior:** Task 2

Add or update focused tests for the new route and run the smallest command that exercises the changed behavior.
