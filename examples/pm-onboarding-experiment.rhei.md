# Rhei: Improve New User Activation

## Overview

This plan models how a product manager might coordinate a lightweight activation
experiment for new users who finish signup but do not complete a first project.

## Success Metrics

- Raise day-1 project creation rate from 34% to 42%.
- Keep onboarding drop-off below the current baseline.
- Produce a launch recommendation backed by experiment data.

## Tasks

### Task 1: Define experiment scope and guardrails
**State:** draft

Write the experiment brief for a first-session onboarding nudge. Capture the
target segment, primary metric, secondary guardrails, rollout size, and launch
decision thresholds.

#### Subtask 1.1: Define the target segment
**State:** draft

Describe which newly signed-up users should enter the experiment and which
traffic should be excluded.

#### Subtask 1.2: Lock the decision criteria
**State:** draft

Document the success threshold, guardrail thresholds, and the minimum sample
size needed before making a launch decision.

### Task 2: Prepare instrumentation and experiment design
**State:** draft
**Prior:** Task 1

Specify the events, properties, and experiment branches needed to measure the
new onboarding nudge end to end.

#### Subtask 2.1: Define exposure and conversion events
**State:** draft

List the exposure, click, dismiss, and project-created events required for the
analysis.

#### Subtask 2.2: Define experiment variants
**State:** draft

Describe the control experience, treatment copy, and any rollout constraints
for the first version of the test.

### Task 3: Align implementation and launch readiness
**State:** draft
**Prior:** Task 2

Coordinate with design and engineering so the team can implement the experiment
without open product questions.

#### Subtask 3.1: Review handoff with design
**State:** draft

Confirm the entry point, copy, and dismissal behavior that will be shown to the
user.

#### Subtask 3.2: Review handoff with engineering
**State:** draft

Confirm the event names, rollout flag, and any dependency on existing
onboarding services.

### Task 4: Evaluate results and recommend next step
**State:** draft
**Prior:** Task 3

Review experiment results, summarize the observed lift and guardrail impact,
and recommend whether to ship, iterate, or stop.

#### Subtask 4.1: Summarize experiment performance
**State:** draft

Compare treatment versus control for the primary metric and each guardrail.

#### Subtask 4.2: Write the launch recommendation
**State:** draft

Produce a concise recommendation with rationale, open questions, and follow-up
actions.
