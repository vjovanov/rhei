# GND-rhei-purpose: Agent Task Planning And Memory

Rhei exists to make agent task planning and memory dependable enough for real
work. It should scale from a one-step cleanup to a long-running, high-risk
workflow such as software security review, release hardening, incident response,
or multi-agent implementation.

## 1. The problem

Agent work is easy to start and hard to keep coherent. Context is scattered
across chats, scratch files, issue trackers, shell history, and generated
artifacts. Humans can lose the thread; agents can repeat old work, skip
prerequisites, overwrite each other, or continue from stale assumptions.

The problem gets worse when the work is important. Security reviews, production
changes, and large refactors need evidence, handoffs, approvals, reversibility,
and a clear account of what changed. Informal task lists are too weak for that,
while heavyweight workflow systems are too rigid for everyday agent work.

## 2. What this project does about it

Rhei makes the plan the shared memory. A Rhei is a human-editable Markdown
workflow with explicit task state, dependencies, artifacts, and transitions.
People and agents can inspect it, diff it, review it, repair it, and resume it
without needing private tool state.

The same model must support:

- simple checklists that stay pleasant to read and edit;
- structured plans with dependencies, hierarchy, and results;
- custom state machines for review loops, approvals, retries, and gates;
- deterministic program steps alongside agent-driven work;
- reusable flows that can be instantiated again with new inputs.

Rhei should be flexible in shape but predictable in execution. Plans remain
plain files, while validation and commands make progress mechanical enough to
trust.

## 3. Who it is for

Rhei is for teams that want agents to work in a form humans can govern. The
primary users are developers, reviewers, operators, security engineers, and
agents coordinating through the repository.

The system should make the right behavior cheap: easy to create a plan, easy to
understand current state, easy to review before and after execution, easy to
pause for a human, and easy to turn a successful plan into a repeatable Rhei.
