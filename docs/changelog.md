# Changelog

## Unreleased

- Windows CI now runs the test suite it can: every crate's unit tests and
  `rhei-core`'s integration tests, which together cover the parsers, the
  validator, workspace and Panta loading, qualified ids, the transition ledger,
  and every prompt renderer — the path-handling code where Windows differs
  most. Until now the Windows job stopped after `cargo build`, so Windows
  proved compilation and nothing about behaviour. The CLI's e2e suite, which
  drives `sh` mock agents, is still Linux and macOS only. §AR-ci-release.1

- CI now runs as two parallel jobs instead of one: `test` (fmt, clippy,
  build, test on three platforms) and `lint` (grund, fissile, lychee,
  attribution, changelog). The Ubuntu job used to carry every repository gate
  on top of the test run — compiling `lychee`, `fissile`, and `grund` from
  source on every run, then `pre-commit run --all-files`, which ran fmt,
  clippy, build, and the whole test suite a second time — and took ~9 minutes
  against ~3 for macOS, so every pull request and every release waited on it.
  The gate binaries are now cached by their pinned versions, and the cargo
  hooks are skipped in the lint job because the test job has just run them.
  §AR-ci-release.1 (PR #93)

## 2. [0.3.0] - 2026-08-23

- Give a cold invocation the project's **mid-term memory**. Under `rhei run`
  every agent starts knowing nothing but its prompt, so the prompt now
  *reconstitutes* what the plan files, result files, exports, briefs, logs, and
  the transition ledger already hold — by a fixed algorithm, at a bounded cost
  in tokens. Four sections join the prompt: **`## Position`** (the chain from
  the Panta through the rhei and every ancestor down to this ticket, its
  siblings with the ones that wait on it marked, the parent's body in full, and
  the standing content sections of the rhei and of the project),
  **`## Plan History`** (every terminal task of the owning rhei and every
  transitive prior, one line each, oldest first, plus `### In Flight` and
  `### Dependents`), **`## Previous Visits`** (this task's own trail through the
  ledger, every verdict recorded against it — the engine's timeout entries
  included — and the path of the previous visit's log), and two sub-sections
  inside `## Rhei Commands`: **`### Reading the rhei`**, the map naming every
  rhei's execution root so no finished task in the project is unreachable, and
  **`### Leaving a trail`**, what a result file and a task body are worth to
  whoever reads them next.

  Composition is a **pure function** of the merged graph, the `runtime/` trees,
  the machines, the settings, the invocation, and the invocations of the same
  run still in flight: no summarization, no ranking, no selection. A summary is
  a fixed slice of a result file, an order is a stated order, a cap is a stated
  number, and a truncation leaves a literal overflow line naming the command or
  file that holds the rest. No run id, timestamp, or pid appears anywhere, so
  the same inputs compose the same bytes — with the in-flight set the one input
  that is not on disk, which makes `### In Flight` reproducible only under
  `--parallel 1`. Every section is omitted when it has nothing to
  say, so a one-task plan under the built-in machine gains a few lines, not a
  page, and a result already pasted in full under `## Prior Task Results`,
  `## Child Task Results`, or `## Checkpoints` is referred to with `see above`
  rather than repeated. §FS-rhei-memory

  A ticket finished **before ids were qualified** still has its account read.
  Its result was written under the rhei-local name and linked from its body,
  and the qualified file the sections look for was never written — so when
  `runtime/results/<qualified-id>.md` is missing and the ticket's body carries
  a `> **Result:**` block, the file that block links is read instead, resolved
  against the owning rhei's execution root. `## Prior Task Results`,
  `## Child Task Results`, `## Checkpoints`, `## Plan History`, and
  `## Previous Visits` all read it, and the overflow line of
  `## Previous Visits` names whichever file that was. A block whose target is
  absolute or climbs out of that root names no artifact of the rhei and is
  ignored. §FS-rhei-plan-language.3.8

  `rhei next` renders the same four sections from the same renderers — after the
  instructions in text, and under `--json` as the string fields `position`,
  `plan_history`, `previous_visits`, and `navigation`, each present exactly when
  its section is. §FS-rhei-memory.5

  Every pasted body in a prompt is now **fenced**, `## Prior Task Results`,
  `## Consumed Exports`, and `## Handoff from …` included: a pasted `## Result`
  used to outrank the heading it was pasted under, so everything after it read
  as a new top-level section. The fence is one backtick longer than the longest
  run the body contains — previously it was one longer than the *count* of
  backticks in it, which gave a body quoting a lot of inline code an absurd
  fence. §FS-rhei-memory.4.5

  A **content section is now parsed verbatim** so `### Rhei Context` and
  `### Project Context` can paste the plan writer's own bytes: its interior
  blank lines survive, and a fenced body before `## Tasks` is no longer dropped
  on the floor. The same text reaches every other surface that prints it, so
  `rhei render --format json` and `--format github` and `rhei viz`'s `about`
  field now carry those blank lines and fenced bodies too.
  §FS-rhei-memory.4.2

  Because those bytes are now visible to it, `rhei validate` **no longer checks
  markdown links inside code**: a link in a fenced block or an inline code span
  is an illustration of the format, not a reference to a file. This covers task
  bodies as well as content sections — a plan that documents how to write a
  task used to fail on the example links in its own instructions.
  §FS-rhei-plan-language.3.6 (PR #89)

- `rhei templates --json` now carries the **whole** input schema for every
  input, not the subset the human table prints: `format`, `positional`,
  `items` (the element schema of an array), and `properties` (the field
  schemas of a record) join `type`, `required`, `default` and `validate`, and
  `items`/`properties` nest the same shape recursively
  (§FS-rhei-templates.6.3.1). This is what another program needs to build an
  **input form** for a template — `format: execution-target` says a value is an
  agent selector rather than free text, and `items.format` says the same of
  every element of an array such as `changeset-review`'s `review_targets`.
  Until now the only way to learn either was to open `template.yaml` and read
  the YAML, which a caller cannot do at all for a built-in template: those ship
  inside the binary and have no directory on disk. Keys an input does not
  declare are present and `null`, so a caller tests values rather than testing
  for the absence of keys. (PR #90)

- Add `rhei new`, so the first rhei and every ticket after it can be created
  without knowing the plan format. `rhei new "Authentication"` writes the rhei
  next to `index.panta.md`; `rhei new "Rotate keys" --under auth` writes a
  ticket inside it, and `--under auth.3` makes that ticket a subtask. Adding
  work no longer starts with remembering that the file is `<id>.rhei.md`, that
  its id is the file stem, or which state the rhei's machine starts in.

  The ticket side is deliberately **complete**: `--kind`, `--state`, `--prior`,
  `--provides`, `--consumes`, `--assignee`, `--model`, `--target`, and
  `--description` cover every field the plan language lets an author write on a
  new node, so `new` is never the command that gets you started and then hands
  you back to an editor. The rhei side takes `--dir` for a Directory Workspace,
  `--states` to bind a state machine, and `--max-levels` / `--node-kinds` for
  the structure block. `--under basin` is the capture path for a ticket with no
  owning rhei, creating `basin/` on first use.

  A create is verified rather than assumed. It never writes over a file that
  already exists, it validates the project it landed in, and it reloads the
  plan to confirm the new id actually reads back out of it — a block that
  landed in dead text is a failure, not a green exit. When something is wrong
  it **rolls itself back**, so a mistyped `--model` or a `--prior` naming
  nothing leaves nothing behind to clean up, and `--keep-on-error` opts out.

  Crucially, a create answers only for the errors it *introduced*. The
  validation pass runs before the write as well as after it, and a project that
  was already failing keeps the create with a warning instead of refusing it:
  a half-broken project is exactly the one someone is adding work to, and a
  command that refuses until everything else is fixed refuses when it is needed
  most. `--dry-run` prints the exact markdown, `--json` reports the created id
  for scripts — including under `--dry-run`, where the object carries
  `"dry_run": true` and the block under `"markdown"`.

  Two rules changed to make this honest. A `## Tasks` section with no tickets
  is now **valid** in a single-file rhei, matching what the Directory Workspace
  format always allowed — a new rhei is genuinely empty rather than seeded with
  a placeholder that `rhei next` would hand to an agent. And `--under` is a
  *ticket* selector: the earlier sketch of `rhei new "Billing" --under auth`
  nesting one rhei inside another described something the hierarchy forbids,
  since a rhei id is a single segment and discovery never descends past the
  project directory's immediate children. §FS-rhei-new §FS-rhei-panta.2 (PR #88)

- Give a parent a way to look after its subtree *while* it runs instead of only
  integrating it at the end. A state that declares
  **`execute_on: <scope>-<event>`** turns the task holding it into a
  **supervisor**: the orchestrator wakes it at *checkpoints* — after every
  finished child (`child-terminal`), every child transition
  (`child-transition`), every finished descendant (`descendant-terminal`), or
  every descendant transition (`descendant-transition`) — with the same agent
  session continued from its previous visit, and holds the rest of the subtree
  while it decides how to steer. The scope says whose moves reach the
  supervisor, the event says which of them do; because a non-leaf child is
  terminal only once its own subtree is, `child-terminal` wakes it exactly once
  per finished child *subtree*, which is how supervision is layered one level of
  decomposition at a time. Continuity needs an agent with session support, which today
  means `pi`: with the built-in `claude-code` profile the supervisor runs each
  visit cold, carried by its checkpoints and its briefs rather than by a
  transcript. A review/fix chain authored as four children no longer runs
  unattended to the end with the parent's context out of the room.

  The supervisor is a **barrier over its subtree**, and the rule is one rule:
  entry holds, the supervisor's **self-loop releases**, a checkpoint holds
  again, and it is ready once nothing beneath it is in flight. So a supervisor
  and one of its descendants are still never worked at the same time — the
  guarantee the non-leaf task model already made, extended to a parent that runs
  many times — and a supervisor that changes nothing changes nothing: it is not
  ready again until a descendant produces one. Under `--parallel` a checkpoint
  is a drain: siblings already running finish, nothing new starts, and one visit
  sees every checkpoint they produced.

  Checkpoints are **post-transition** and reach exactly one task, the *nearest
  in-scope* supervising ancestor: a `child-*` supervisor declines a
  grandchild's move and the event climbs to the next ancestor whose scope
  reaches that deep, or to nobody. Scope narrows what wakes a supervisor, never
  what it holds — a `child-*` supervisor is still the barrier over its whole
  subtree, and the descendants it does not hear about simply run freely between
  its visits. A poll retry is not a checkpoint, a supervisor's own release edge
  is not one for its own ancestors, and neither is a move the supervisor made
  itself during its visit. The phase and the pending checkpoints live in plan
  frontmatter beside `stateVisits`, written on the shared transition path — so
  `rhei run`'s auto-advance, `rhei transition`, `rhei complete`, and a callback
  redirect all maintain the barrier identically, a run stopped between a
  checkpoint and the visit resumes exactly where it was, and a manual worker
  sees the same state the orchestrator would. `rhei reset` clears it.

  The supervisor steers with the levers that already exist. It writes a
  **brief** at `runtime/supervise/<task-id>.md` (or
  `runtime/supervise/<task-id>/<state>.md` for one state only) that the next
  step reads under **`## Supervisor Brief`** — direction, bounded by that state's
  own instructions and artifact contract. It appends children by editing its own
  task file and cancels ones the results made unnecessary with `rhei
  transition`, both of which the orchestrator sees on re-read. Its own prompt
  gains **`## Checkpoints`** — what moved since its last visit, each carrying
  the result or the source state's outputs — and an unsupervised parent finally
  gains **`## Child Task Results`**, the result of every terminal child, which
  it never saw before.

  Transition conditions gain **`openDescendants`**, the number of non-terminal
  descendants of the transitioning task, evaluated against the plan as re-read
  after the subprocess exits. It is how a machine *selects* a parent's terminal
  edge once its subtree closes; the descendants-first guard still decides
  whether that edge may be taken. Self-loops on non-poll agent states are now
  general **loop-back re-entries** rather than a polling-only construct.

  That generalization is a **behaviour change** for machines that already had
  such a self-loop: the engine now counts visits of every non-poll state a
  self-loop is declared from, so a `condition: visitCount >= N` exit on that
  loop fires where it previously compared against `0` and spun forever. The
  visible side effect is that a task which merely *enters* such a state gets
  `stateVisits.<state>: 1` in frontmatter even if it never loops. How the state
  is spelled does not change: `**State:**` takes its `-<n>` suffix only where
  `visits:` is declared. `rhei validate` now warns when a non-poll self-loop
  declares neither `visits:` nor a `visitCount`-bounded exit — nothing ends that
  loop.

  Cancelling a step no longer has to satisfy that step's own `outputs:`. This
  is a deliberate **behaviour change** on every verb, not only under a
  supervisor: a transition whose effective target is `cancelled` skips the
  source state's required outputs, because cancellation abandons the work and
  the contract of a step nobody is finishing is moot. Without it a supervisor
  could not drop a pending `review` child whose state declares a `findings`
  output — the whole point of cancelling it is that nobody will write one.
  Nothing else on the path changes: the descendants-first guard, the target's
  `inputs:`, the callbacks, and the terminal-result obligation all still apply,
  so a cancel still needs `--result "<why>"`, and the supervisor's prompt now
  says so.

  `cancelled` is now a **reserved state name**, and `canceled` is accepted as
  the same name. Four rules read it as "the work was abandoned" — a cancelled
  prior does not satisfy a dependency, `rhei complete` never selects it, the run
  report marks it apart from success, and a transition into it waives the
  source state's outputs — and each used to spell the test itself, so an
  American-spelled machine got cancellation semantics in one surface out of
  four. A machine that names its abandon state anything else keeps the ordinary
  outputs check, and the refusal on a transition into a `final: true` state now
  names the state that skips it. §FS-rhei-states.1.4

  A supervisor that leaves its supervising state for a **human gate keeps its
  subtree held**. The `supervision` block, not the state, is the hold: an exit
  into a `gating: true` state keeps the block, every other non-self-loop exit
  removes it, and a human moving the parked ticket on is what releases the
  subtree. Exhausting a visit budget used to silently un-supervise everything
  beneath the supervisor, which is the opposite of what a budget is for. The run
  says so at that transition and the report gives the parked ticket a row naming
  the subtree it still holds.

  Every surface that explains readiness gained the reason: `rhei next` refuses a
  held descendant by naming its supervisor — or the worker holding that visit,
  or the human at the gate — `rhei list --ready` excludes it and
  admits a supervisor whose subtree is still open, the run report gives held
  tickets a **Waiting** section of their own rather than diluting Attention, and
  `rhei run --dry-run` names the barrier per pass and renders the release
  self-loop as `(release)`. `rhei run` also prints the machine's validation
  warnings once at start, so a machine whose supervisor has no `openDescendants`
  exit is called out before the run spends the whole subtree proving it — and if
  it halts there, the halt names the missing transition line.

  `examples/subtree-supervision/` runs the whole chain with a committed mock
  agent and no credentials. §FS-rhei-supervision §DF-subtree-supervision
  Issue #86. PR #87 §FS-rhei-supervision §DF-subtree-supervision

- A new built-in template, **`supervised-delivery`**, is the workflow
  `execute_on:` was for: one supervising task that reads a spec, sends the
  implementer, sends a code review and a product review of the same round
  together, sends the fixer, and then decides from the resolutions it just read
  whether the next round happens or gets cancelled so coverage can start. Two
  things make the supervisor the one that decides. The **brief is the release
  gate** — every child state declares a required input at
  `runtime/supervise/{task_id}.md`, so no step is dispatched until the
  supervisor has written that step's brief. And the channel between steps is
  **plan exports** (`**Provides:**` / `**Consumes:**`), each one file holding a
  single fenced `json` block, declared as that state's `outputs:` so a step
  cannot finish without publishing it; the schemas ship as `prompt_templates/`
  fragments the reviewer states share. Run it with `--parallel 2` or more so the
  two reviews of a round overlap. The rendered example is
  `examples/supervised-delivery-example/`. §FS-rhei-supervision §FS-rhei-templates
  PR #87 §FS-rhei-templates

- `rhei instantiate`'s template environment documents `range()`, arithmetic, and
  `~`, which it already supported. `{% for k in range(1, rounds + 1) %}` is how
  a template unrolls a counted structure into one task per round — the shape to
  reach for when each round needs its own `**Prior:**`, exports, or title, which
  are per-task metadata a counted `visits:` loop has nowhere to put. The same
  arithmetic sizes a state's budget from the inputs that shaped the plan.
  §FS-rhei-templates.5
  PR #87 §FS-rhei-templates.5

## 3. Older releases

- [0.2.0](changelog/0.2.0.md) - 2026-08-22: - Separate a run from the surface that watches it.
- [0.1.0](changelog/0.1.0.md) - 2026-05-21: - Initial alpha release line for the Rhei CLI, Rust crates, npm wrappers, and PyPI wrappers.
