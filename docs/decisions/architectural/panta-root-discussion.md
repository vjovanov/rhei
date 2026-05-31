# Panta root — discussion notes and open questions

Working notes for the Panta proposal (§DA-panta-root, §FS-rhei-panta,
§AR-rhei-panta). This is **not normative** — it captures a skeptical review of
the current spec and a menu of proposed changes for the team to decide. Each
item explains *why* it matters in plain terms.

## What Panta is (one paragraph)

Panta is a single, invisible, per-project root that sits above all rheis and
tickets. It is the default home for new rheis and the one anchor for a
project-wide view. The model: `Panta → rhei → ticket`. The key reuse is that the
language already has a virtual, non-authored, rollup-only level-0 root (the
`rhei` root that "denotes the plan itself"); Panta is that same pattern promoted
one level, and rheis merge into one graph the way a Directory Workspace already
merges task files.

## Verdict

The framing (reuse the existing virtual-root pattern) is the strongest part.
But the spec has structural soft spots that should be settled before any
implementation, and it quietly delivered something different from the option
that was approved. Roughly 60/40 in favour of the direction, with real
reservations about proportionality.

## Concerns, ranked

1. **The approved option and the built design differ.** "Unified root node" was
   chosen over "registry by reference," and unified was described as *"rheis lose
   self-containment, large parser changes."* What is actually specified is a
   **hybrid**: rheis stay independent files, merged into one graph only at load.
   That is arguably better than either option offered, but it is not what was
   chosen — it should be re-decided with the real model on the table. The only
   self-containment actually lost is dependency-wise: a rhei with a cross-rhei
   `**Prior:**` no longer validates on its own.

2. **Filename-derived rhei ids are brittle.** `rheis/auth.rhei.md → auth`
   (§FS-rhei-plan-language, Panta Project). Renaming the file silently changes
   the rhei's identity — breaking every cross-rhei reference to it and orphaning
   its saved results, with no error. The repo's own workspace guidance recommends
   stable ids for exactly this reason.

3. **Two id systems are under-specified.** There is a rhei-local id (`1`) and a
   project-qualified id (`auth.1`), and the spec never clearly says which one a
   `**Prior:**` reference uses. "Sometimes this name, sometimes that name" is
   where confusion and parser bugs live.

4. **The actual ask is the thinnest part.** "The default place rheis are added"
   is `rhei new`, and it is barely specified: where the file lands, whether it
   lazily creates the project, and — most importantly — whether running it inside
   an existing single-file plan silently restructures the user's directory. That
   silent restructuring is the core UX moment and is currently hand-waved.

5. **Proportionality.** A "default location" grew into a new node kind, a grammar
   change, a `node_policy` tier, scope rules across eight commands, and a clean
   break that invalidates every existing plan. The unified view and cross-rhei
   dependencies are largely elaboration beyond the original ask. The cheaper
   registry option may deliver the core need without most of this.

6. **Destructive project-wide default + scope inferred from cwd is a footgun.**
   `rhei reset` wiping runtime across every rhei because of which directory you
   were standing in. "Report scope before acting" is a weak guardrail.

7. **Cross-machine "terminal" semantics are unsound.** Readiness is "terminal and
   not `cancelled`," but `cancelled` is a specific state *name*. A custom per-rhei
   machine naming its give-up state `wontfix` breaks the rule, so cross-rhei
   dependency satisfaction is undefined across heterogeneous machines.

8. **Smaller:** "invisible" yet the user must author the `# Panta:` manifest;
   "rheis are never claimable" contradicts "a profiled rhei may be run"; the AR
   layout shows three `runtime/` locations with no clear answer for where a
   ticket's artifacts go; and the other command specs still describe the old
   single-plan world, so the spec corpus is internally inconsistent until they
   are reconciled.

## Proposed changes (decide per item)

Format: **What** / **Why (plain)** / **Options** (recommended marked).

### Structural

**P1+P5 — Build a smaller v1; defer the fancy parts.**
- *What:* v1 = default home for new rheis + unified read-only view + derived
  rollup. Defer cross-rhei dependencies and per-rhei custom workflows to v2.
- *Why:* the current spec touches the grammar, the state machine, and every
  command, and breaks every existing plan — all for what began as "a default
  folder for rheis." Shipping the small version first delivers the real ask
  quickly, breaks almost nothing, and only pays for complex features if they
  prove needed. Remodel a shelf, not the whole house.
- *Options:* **(a) small v1, defer the rest [recommended]** · (b) full design now.

**P2 — Stable, authored rhei ids.**
- *What:* write the id in the heading — `# Rhei auth: Authentication` — instead
  of deriving it from the filename.
- *Why:* a name guessed from the filename changes the instant someone renames the
  file, silently breaking every link to that rhei and orphaning its results.
  Writing the name inside the file makes renames harmless. Bonus: rheis then look
  like tasks, which already carry their id this way.
- *Options:* **(a) id in `# Rhei <id>:` heading [recommended]** · (b) id in
  frontmatter · (c) keep filename-derived.

**P3 — One id/reference model.**
- *What:* ids are globally unique across the project and used everywhere — exactly
  how multi-file workspaces already work. Drop the local-vs-qualified split.
- *Why:* one name for one thing is boring and safe; two names for the same ticket
  is where people and parsers get confused. Cost: the id no longer visually
  encodes which rhei it belongs to, but the tool tracks that separately.
- *Options:* **(a) flat global ids like workspaces [recommended]** · (b) keep the
  two-id scheme with a strict disambiguation rule.

### The verb actually requested

**P4 — Define `rhei new` / add `rhei init`; never restructure files silently.**
- *What:* `rhei new` inside a project writes a file under `rheis/`. In a plain
  folder with no project, it stops and tells the user to run `rhei init` first,
  rather than quietly converting the folder. `rhei init` is the explicit
  "make this a project" step.
- *Why:* a command that silently moves your files and changes your setup is hard
  to forgive and hard to undo. Making project creation explicit means nothing
  moves unless asked.
- *Options:* **(a) require explicit `rhei init`, never auto-convert
  [recommended]** · (b) auto-create the project on first `rhei new`.

### Safety & correctness

**P6 — Confirm destructive ops when scope is guessed.**
- *What:* `rhei reset` run from inside a project (scope guessed from cwd) asks for
  confirmation or needs `--all`; an explicitly named project proceeds.
- *Why:* "throw everything away, and decide what 'everything' is from whatever
  folder I'm in" is how a project gets nuked by accident. A confirmation only when
  the tool is guessing costs nothing on purpose.
- *Options:* **(a) confirm when scope is inferred [recommended]** · (b) leave as
  just-do-it.

**P7 — Flag-based terminal outcome (only if cross-rhei deps stay in v1).**
- *What:* terminal states declare `success: true|false`; a dependency is satisfied
  only if it finished with `success: true`.
- *Why:* the current rule keys off the literal word `cancelled`, so any workflow
  that names its give-up state differently breaks it, and a ticket could start
  work based on a dependency that was actually abandoned. A yes/no flag works
  regardless of state names.
- *Options:* **(a) add a `success` flag [recommended]** · (b) keep the word-based
  rule · (c) moot under P1a.

### Smaller cleanups (apply unless objected)

- **P8a — `index.panta.md` optional;** a single-rhei project needs no manifest.
  *Why:* forcing a manifest for an "invisible" thing with one plan is busywork.
- **P8b — rheis are pure rollups; never directly worked.** *Why:* remove the
  self-contradiction so nobody implements the confusion.
- **P8c — one rule for where a ticket's artifacts live.** *Why:* the current
  three-`runtime/` diagram leaves "where does `auth.1` write?" unanswered.
- **P8d — one-line Panta pointer in each affected command spec.** *Why:* the docs
  currently disagree with each other until the full per-command edits land.

## Suggested fast path

`P1a + P2a + P3a + P4a + P6a` plus the four cleanups gives a simpler, safer,
self-consistent v1 and lets P7 wait.
