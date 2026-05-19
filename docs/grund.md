# GND-rhei-purpose: Governed Agent Work

Rhei exists because useful agent work must outlive the private session that
started it. Agents can move fast, but real work also needs memory, sequencing,
review, evidence, and handoff. Rhei turns agent plans into repository state:
plain-text workflows that humans and agents can inspect, edit, validate, run,
pause, resume, and reuse.

## 1. The problem

Agent work is easy to begin and easy to lose. Intent lives in chat history,
task state lives in scratch notes, evidence lives in generated artifacts, and
the real source of truth is often whichever context window is currently active.
That makes progress fragile. Agents can repeat work, skip prerequisites, act on
stale assumptions, overwrite each other, or finish without leaving enough record
for a human to trust what happened.

Informal checklists help only at small scale. They usually do not encode
dependencies, legal state changes, required outputs, review gates, or recovery
points. Heavy workflow systems solve some of those problems, but they often make
everyday agent work feel like operating infrastructure instead of editing a
plan. The missing middle is a format that stays as approachable as Markdown
while being strict enough for predictable execution.

## 2. What this project does about it

Rhei makes the plan the shared memory and control surface. A Rhei is a
human-editable Markdown workflow with explicit task state, dependencies,
hierarchy, artifacts, and transitions. The file is the thing people review,
agents follow, commands validate, and automation advances.

The same model must support:

- one-step tasks that remain pleasant to write by hand;
- multi-step plans with dependency-aware ready-work selection;
- child tasks, reusable templates, and repeatable workflows;
- custom state machines for approvals, review loops, retries, and gates;
- deterministic program steps alongside agent-driven steps;
- execution records and artifacts that make completed work auditable;
- multi-agent handoffs without relying on private session state.

Rhei should be flexible in shape and predictable in execution. Plans remain
plain files that can be reviewed in normal repository workflows, while the CLI
provides enough validation, orchestration, transition control, and monitoring to
make progress mechanical when the work matters.

## 3. Who it is for

Rhei is for teams that want agents to work in a form humans can govern. The
primary users are developers, reviewers, operators, security engineers, release
owners, and agents coordinating through the repository.

The system should make the correct path cheap: easy to create a plan, easy to
see what is ready, easy to understand what is blocked, easy to review before and
after execution, easy to pause for human judgment, and easy to turn a successful
workflow into a repeatable Rhei.
