# Changelog

## Unreleased

- Show why a task is parked. A state that declares no artifacts of its own —
  typically a gating `needs-human` — now borrows the previous state's outputs
  in the inspector's artifacts section, labeled with the state they come from,
  in both the browser dashboard and the TUI. The TUI goes further: an artifact
  row whose path resolves to an existing workspace file is followed by a
  bounded head excerpt, so the report whose transition parked the node — the
  question a person must answer — is readable in place instead of requiring a
  trip through `runtime/`. TUI artifact paths also resolve `{state}` now.
  §FS-rhei-viz.4 §FS-rhei-run-tui.1.5.3

- Name the resolved path when required outputs are missing, and fix the
  `ci-heal` example that made the gap costly. The warning after a zero-exit
  with missing outputs listed artifact names only — `…: report` — while the
  resolved path sat unused in the same function, so a path that resolved
  somewhere unexpected read as a false negative. Each entry is now
  `name (resolved/path)`, and a path still holding a `{...}` template is marked
  `unresolved template`, which points straight at the cause. The cause was
  shipped in-tree: `examples/ci-heal` used `{task.id}`, `{visit}`, and
  `{task.metadata.branch}` — the shape of the callback JSON context object, not
  the template namespace — so paths resolved to literal `{task.id}.json` and the
  `BRANCH` env var handed to both program states was the string
  `{task.metadata.branch}`. Path resolution leaves unknown variables verbatim by
  design, and nothing else caught it, so the example validated clean. The
  example now uses `{task_id}`, `{visit_count}`, and `{meta.branch}`; its
  `branch` metadata moved from the task body, where nothing parses it, into the
  plan's `metadata:` block; and `push-fix` declares the `visits: 5` it needs to
  read a per-visit artifact, since `{visit_count}` is per-state and falls back
  to `1` wherever `visits` is absent. Issue #66. PR #67
  §FS-rhei-agents.3.2.1 §FS-rhei-states.4.1 §FS-rhei-states.4.2
- Give every failing `rhei` command a next action. Errors now carry a `help:`
  line with a runnable command; missing template inputs are reported all at once
  with a suggestion that echoes the arguments already supplied; unknown
  templates, inputs, agents, modes, models, and object properties suggest the
  closest match; filesystem failures say whether the path, its directory, or the
  permissions are the problem; and JSON error output carries the same help.
  Every command Rhei prints is now POSIX-quoted, so an execution target such as
  `agent='codex[yolo]:openai:gpt-5.5'` survives paste into zsh instead of dying
  on glob expansion. Template inputs may declare `format: execution-target`,
  which validates the selector where the user typed it instead of failing later
  against a rendered `states.yaml` the user never wrote. Rejected inputs batch
  the same way missing ones do, a repair example is keyed to the input it
  corrects so it pastes back, candidate lists name each id once and stop at
  eight, and `--agent` given a full selector says which flags carry the mode and
  the model instead of sending the user to define an agent named
  `claude-code:some-model`. A correction is offered in a form the CLI accepts:
  a scalar nested in an array is named through its enclosing input rather than
  as a `reviewers[0]=…` assignment that does not exist. Coverage is now enforced
  by a test rather than by review. PR #64
  §FS-rhei-errors.1 §FS-rhei-errors.2 §FS-rhei-errors.3.1 §FS-rhei-errors.4
  §FS-rhei-errors.5 §FS-rhei-errors.6 §FS-rhei-templates.3.1
- Fix three ways a state handoff could go missing or take the run down with it.
  A handoff artifact path that templates the execution identity (`{model}`,
  `{agent}`, `{target.slug}`) resolved under the *successor's* identity, so a
  handoff between states on different models looked for a file the producer
  never wrote; it now resolves under each identity the source state declares.
  An artifact that exists but is empty now counts as no handoff — `outputs:` is
  an existence contract, so an agent could satisfy it with a zero-byte file and
  hand its successor silence that looked exactly like success. And a prompt
  that cannot be composed now fails its own task instead of the whole pass,
  matching the required-tooling gate: `--continue-on-error` moves on to the
  next task, and without it the run still aborts. PR #49
  §FS-rhei-states.3.2 §FS-rhei-run.3
- Hand work product between tasks with `**Provides:**` and `**Consumes:**`. A
  task publishes a named export (`**Provides:** api-contract`), a dependent
  task reads it (`**Consumes:** auth.1:api-contract`), and Rhei injects the
  content into the consuming agent's prompt while telling the producing agent
  the path to write. Exports live at `runtime/exports/<task-id>/<name>.md`
  under the owning rhei's execution root, so a cross-rhei prior's export
  resolves where that rhei keeps it. This is a plan-level handoff on purpose:
  the dependency graph orders it, so producer and consumer need not share a
  state machine or a workflow phase — unlike state handoffs, which carry notes
  between the states of one task and are declared in `states.yaml`. Nothing
  validates the pairing yet: a `**Consumes:**` reference with no matching
  `**Provides:**`, a producer that is not a prior, and a declared export that
  was never written all read as a missing file and are silently skipped.
  PR #49 §FS-rhei-plan-language.3.12 §FS-rhei-agents.3

- Rename the `multi-agent-deliberation` template to `agora`. The built-in
  template that splits a discussion into points, collects proposals and
  disagreements, and resolves each one carried the one name in the library
  taken from its activity rather than from the Greek lane the product is named
  in — `rhei`, `panta`, `basin`. It is now `agora`, the assembly place, across
  the template directory and id, the state-machine `name:` and the
  `**States:**` declarations that select it, the `deliberation-task` node kind,
  the `plan_title` and `runtime/agora` `output_dir` defaults, and
  `examples/agora-example/`. Instantiate it as `rhei instantiate agora`; the
  old name is gone. Prose that calls the activity a deliberation is unchanged,
  because the agora is where the deliberation happens — which is why the name
  was chosen. PR #56

- Measure Pi token usage. Pi was listed as an accounting-supported agent but
  was wired to the generic structured-capture contract, which stock Pi does not
  emit — so every Pi invocation recorded `no-usage-emitted` even though Pi
  reports measured usage in its JSONL. Pi now runs with `--mode json` and its
  assistant `message_end.message.usage` events feed the same normalized
  capture, invocation record, rollup, and monitoring path as Codex. The
  duplicate usage Pi repeats in `turn_end` and `agent_end` is ignored so each
  provider call is counted once, and live monitoring renders assistant text
  with a compact usage line instead of the full event stream — the complete
  JSONL stays in the agent log. PR #51 §FS-rhei-cost-accounting.4

- Compile the skills into the binary, so `rhei install-skills` works from an
  installed `rhei`. The command resolved skills by walking up from the binary
  for a repo root, which a `cargo install`ed or npm-installed `rhei` does not
  have — so the one command whose whole job is installing files failed with
  "could not find skill source directory", and failed that way even when run
  from inside a rhei checkout, because the current directory was never
  consulted. The skills now ship inside the CLI package and are embedded like
  the built-in templates; a checkout's copy still wins when there is one, so
  editing a skill and installing it installs the edit. `--link`, which cannot
  point at a temporary extraction, now says so and names the paths that would
  satisfy it instead of writing dangling symlinks.
  §FS-rhei-install-skills.4.3 §FS-rhei-install-skills.4.4
- Stop `install-skills` from deleting the lines after its own block in
  `CLAUDE.md`. Both the update and the uninstall path treated the `# rhei`
  section as running to the next heading, so anything a user kept in between —
  a blank line, another tool's `<!-- >>> ... >>> -->` marker, a paragraph under
  no heading — was silently removed from a file rhei does not own. Installing
  over an existing block ate the opening marker of the next tool's block,
  leaving that block unterminated. The generated section is contiguous, so it
  now ends at the first blank line (or the next heading of equal or higher
  level, whichever comes first) and everything past that boundary is left
  alone. §FS-rhei-install-skills.4.5
- Install `rhei-template-writer` by default. The skill shipped in the repo but
  was absent from the `--skills` default, from the registration block, and from
  shell completion, so it reached only users who knew its name and typed it —
  and `rhei install-skills` left every other user without the skill for
  authoring templates. The default is now every skill the binary carries, and a
  test fails when the two drift apart. A misspelled `--skills` name now lists
  the skills that do exist rather than reporting a missing directory.
  §FS-rhei-install-skills.2

- Make the state machine a property of the rhei, defaulted by the project.
  One machine governed a whole Panta project, so the second template a user
  instantiated was refused, a rhei that declared a different machine was a
  load error, and `rhei init` had to hoist a machine into the manifest to
  keep such a project loadable — three mechanisms enforcing a uniformity that
  contradicted the product's own pitch of composing routines. A rhei's
  `**States:**` declaration now overrides the project default, its
  `states.yaml` resolving from its own root first; every ticket is validated,
  listed, transitioned, and run under the machine of the rhei that owns it,
  and a cross-rhei prior is judged terminal under the machine of the rhei
  that owns the *prior*. Templates with different machines compose in one
  project; the refusal, the load error, and both adoption paths are gone.
  §DA-per-rhei-state-machines §FS-rhei-panta.6 §AR-rhei-panta.4
- Validate the node-kind keyword on `**Prior:**` references. The parser
  accepted any word as a kind prefix, so `**Prior:** Banana 1` validated green
  and a pasted task *title* — `**Prior:** Design schema` — failed as
  "depends on missing Task p.schema", an id nobody wrote, manufactured from the
  title's second word. A keyword must now match the referenced node's declared
  kind (`Task 2` naming a Bug is an error that says so), an undeclared keyword
  on an unresolvable reference leads with the title reading and points at
  referencing by id, and a `**Prior:**` list that names the same task twice is
  an error instead of silence — both constraints the spec already stated.
  §FS-rhei-plan-language.3.1
- Let `rhei complete` take the ticket id positionally. Every ticket surface
  prints bare ids, but `rhei complete launch.1` — the exact shape the generated
  agent note implies — failed wanting `--task` and `--result`, and silently
  read the id as a plan path. An id-shaped positional that names no existing
  path is now the ticket (`rhei complete auth.1 --result "…"` works as
  pasted); an existing path still means the plan, and `--task` keeps its
  meaning for scripts. §FS-rhei-complete.2.1
- Stop shipping the spec-review demo fixture into user projects. Instantiating
  `spec-review` wrote `specs/template-review-fixture.spec.md` — a file whose
  own text says it exists for the checked-in example — next to the user's real
  spec. The fixture is now example-owned; an instantiation reviews the spec the
  `spec` input names and ships no demo data.
- Put the init agent-discovery note where the repository's agent reads. In a
  repository whose instructions live only in `CLAUDE.md`, init created a fresh
  `AGENTS.md` for the note — a file that agent never opens. The note now goes
  into `CLAUDE.md` when it is the only instruction file, re-runs rewrite it in
  place wherever it landed, and init names the file it changed. Repositories
  with `AGENTS.md` (including the `CLAUDE.md → AGENTS.md` symlink) keep it as
  the target. §FS-rhei-init.4
- Say when a standalone workspace lands as tracked repository content. The
  escape hatch every placement error offers — `--output` beside the project —
  produced a workspace no init-managed ignore rule covers, silently
  contradicting the "planning state is working material" stance the same repo
  chose for `panta/`. Instantiation now notes the untracked workspace and the
  `.gitignore` entry that would ignore it; it never edits `.gitignore` itself,
  because committed workspaces (the checked-in examples) are the other
  legitimate use. §FS-rhei-templates.6.2
- Answer `rhei templates <name>` with the template. Naming a template after
  reading the list — the obvious next gesture — was an argument error that
  pointed nowhere. It now prints the template's detail: the input schema
  `--list-inputs` shows, the source tier, and an instantiation hint with every
  required input spelled out; `--json` emits the list's entry shape for one
  template, and a near-miss still gets the resolver's "did you mean".
  §FS-rhei-templates.6.3
- Render instantiation-report paths relative to the working directory. The
  summary echoed absolute paths everywhere — `Output:`, the follow-up
  `rhei run /tmp/…/panta/product-management`, the reproducible command's
  `--output` — where `rhei next panta/product-management` is what a reader
  actually types from the directory the report is read in. Paths outside the
  working directory stay absolute. §FS-rhei-templates.6.1.3
- Name the rhei id in merged-project render headings. A plan titled
  `# Rhei: Q3 Launch` in `design.rhei.md` owns tickets `design.N`, but the
  progress and GitHub renderers headed its block with the title alone, so
  nothing connected `design.2` in a list or error back to the block that
  explains it. Headings now read `Q3 Launch (design)`, staying bare when the
  title already names the id. §FS-rhei-render.3.4
- Make `rhei instantiate` reckon with the Panta project it is standing in. Run
  inside a project it wrote its workspace to the working directory, where
  discovery never looks — `rhei list` still reported an empty project after a
  successful instantiation. Pointed into `panta/` with `--output` it wrote a
  second state machine, and since one machine governs a whole project
  (§FS-rhei-panta.6) *every* project-scoped command then failed to load, while
  instantiate itself had printed "Validation succeeded" from validating the
  workspace in isolation. A template's output now defaults to the project, the
  project adopts the template's machine when it is the first rhei, a genuine
  collision is refused before anything is written and names the `--output` that
  works, and a member rhei is validated through its project. Output placed
  under a project but not directly next to `index.panta.md` — including every
  single-file template, which renders a plain directory — is warned about
  rather than silently unlisted. §FS-rhei-templates.6.2
- Hoist a template's bundled `settings.json` to the project it joins. Settings
  resolve once, at the root the plan loads from, so a copy left in a member
  workspace was read by nothing: the built-in `product-management` template
  failed validation on its *own* default targets as "unknown target mode
  'xhigh'", with the error pointing at the selectors rather than the registry
  that had gone missing. Existing project values win the merge, and both the
  added and the kept keys are reported. `rhei validate` now also warns when it
  finds a member rhei carrying settings the project ignores.
  §FS-rhei-templates.4 §FS-rhei-agents.1.1
- Stop `rhei reset` from destroying runtime state on an unattended invocation.
  With no terminal it skipped the confirmation *and* the damage preview and
  went ahead, so `echo n | rhei reset panta` deleted every result and ledger —
  material `rhei init` gitignores, with no VCS copy to recover from — and did
  it silently. It now prints the preview before every destructive reset,
  including `--yes`, and fails without a terminal unless `-y` states the
  intent. **Breaking:** scripts, CI, and agents that reset non-interactively
  must pass `-y`. §FS-rhei-reset.1.2
- Let `rhei list` survive a malformed plan. One unparseable `*.rhei.md` failed
  every project-scoped command — including `rhei list --rhei <a-healthy-one>`,
  which names a rhei that parses fine — leaving no way to see the rest of the
  project while fixing it. Listing now skips what it cannot load and warns per
  skipped rhei; validation, claiming, transitions, runs, and resets still stop,
  because a partial graph cannot decide readiness. §FS-rhei-panta.6
- Anchor the `AGENTS.md` agent-discovery note at the repository root. Written
  at the host, `rhei init <subdir> --here` buried it inside the adopted plans
  directory, where no coding agent reads it — precisely the mode documented for
  adopting a directory of existing, versioned plans. Init now names the path
  when the note lands outside the host, and a failed target resolution lists
  any project it finds below the invocation directory instead of only reporting
  that nothing was found above it. §FS-rhei-init.4 §FS-rhei-panta.6
- Stop advertising `--state-machine` on the seven commands that ignore it. The
  flag was `global`, so it appeared in `--help` for `init`, `templates`,
  `install-skills`, `completions`, `version`, `cost`, and `intervene`, and was
  silently accepted there: `rhei init --state-machine ./my-states.yaml`
  reported success and used the default. It is now declared on the commands
  that read one, and still accepted before the subcommand.
- Say what belongs in a first plan. `init`, `list`, `next`, and `validate` all
  pointed at where a `<id>.rhei.md` file goes without saying what goes in it,
  and never mentioned `rhei templates` / `rhei instantiate` — the only route
  that does not require knowing the language first. All four now name both
  routes, and the roomier surfaces show a minimal plan skeleton.
- Report every missing template input at once, name `--list-inputs`, and stop
  tearing structured defaults apart. Inputs were reported one per run, turning
  supply into a guessing loop that bought one field name per attempt, and an
  array default rendered as raw multi-line YAML inside a single-line
  `(type, default=…)` parenthetical. §FS-rhei-templates.6.1.1
- List the agents and modes that *do* resolve when one does not. The built-in
  registry defines a single mode per agent, so `codex[high]` validates only
  where a settings file adds it, and nothing else lists what is there — unlike
  an invalid state, which has always named its allowed states.
  §FS-rhei-agents.1.1
- Finish the renamed-ticket hint. It named the `mv` for the result artifact but
  not the link text that also has to change, so following it exactly produced a
  second, differently-worded failure about a dangling target. It now spells out
  both halves. §FS-rhei-complete
- Smaller fixes: `rhei run` names the ready tickets it skipped because someone
  already holds them, rather than dropping them from the pass report;
  `rhei render --format` lists its values in the missing-argument error;
  progress output heads a project with `Panta:` rather than `Rhei:`, which put
  the same word on two levels; `rhei install-skills` names `--agent` after a
  default fan-out across all eight agents; and the README documents the child
  heading syntax, that `structure.nodeKinds` replaces rather than extends the
  default, and that the frontmatter block goes below the `# Rhei:` heading.

- Ship a built-in template library inside the binary. `rhei templates` printed
  "No templates found." on every fresh install: templates were project- and
  user-scoped only, so the ten shipped with this repo lived in a directory a
  `cargo install` never sees. They now compile into the binary as a third
  discovery tier below project and user, so a project or user template of the
  same name still shadows one. §FS-rhei-templates.1
- Add `rhei release`, the counterpart to the claim `rhei next` writes. A worker
  that crashed left `**Assignee:**` behind and `rhei next` refused to hand out
  work while it stood, with no way to drop it: the only escape was editing the
  markdown or `rhei reset`, which rewrites every ticket in scope and deletes the
  whole `runtime/` tree. Release drops the claim and nothing else, takes
  `--task <id>` or `--all`, and has `--dry-run`. §FS-rhei-release
- Render `rhei viz` for a Panta project as one merged graph. It drew a separate,
  disconnected plan per `*.rhei.md`, omitted cross-rhei dependency edges, and
  skipped Directory Workspace rheis entirely — on the layout `rhei init` creates
  by default, and on the same surface `rhei run` serves live. A member rhei
  renders that graph narrowed to itself, keeping the far end of its cross-rhei
  priors so an edge is scoped rather than erased. §FS-rhei-viz.7.3
- Load a rhei that belongs to a project *through* its project. Pointing a
  command at `panta/billing.rhei.md` read the file alone, so a legitimate
  cross-rhei `**Prior:**` had nothing to resolve against: a correct plan failed
  `rhei validate <file>` while passing `rhei validate` on its project — a false
  failure for any per-file CI or pre-commit check. The path now implies
  `--rhei <id>`; validation, which takes no `--rhei`, widens and says so.
  §FS-rhei-panta.6
- Report a `**Prior:**` under the id the author wrote. An unresolvable dotted
  reference was re-qualified with the citing rhei, so `c.1` surfaced everywhere
  — validation errors, `rhei list`, every renderer — as `a.c.1`, an id in no
  file. The previous release bolted an explanatory hint onto the mangled id;
  this fixes the mangling. The correction offered is now required to resolve to
  a real ticket other than the citing one, so a one-character rhei name can no
  longer suggest a task as its own prior. §AR-rhei-panta.3 §FS-rhei-validate.4.1
- Report every parse problem in a file when it is reached through its project.
  Project-scoped loading stopped at the first one and dropped the structural
  diagnostic that actually explains the mistake: a task heading authored under a
  content section reported only "Metadata field appears outside a task", on a
  line the author did not get wrong, while `rhei validate <that same file>`
  additionally named the `## Tasks` rule. Diagnostics also render the plan path
  relative to the invocation directory when that is shorter.
  §FS-rhei-validate.4.2
- Group project-scoped `rhei render` output by rhei. Both text formats printed
  every rhei's heading as a block and then one flat merged task list, leaving a
  rhei without a content section as a heading with nothing under it, and the
  GitHub format emitted runs of blank lines. Each rhei now renders as one block:
  title, its own sections, then its tickets. `--format progress` also leads with
  the completion summary it is named for. §FS-rhei-render.3.3 §FS-rhei-render.3.4
- Exclude parent tickets from `rhei list --ready`. `--ready` listed epics that
  `rhei next` will never claim — it claims leaves only — so any count taken from
  the listing overstated the available work. §FS-rhei-list.3.1
- Name the fix in the unknown-node-kind error. It reported the declared kinds
  but not where kinds are declared, leaving `### Bug 3:` a dead end; it now
  points at `structure.nodeKinds` in the plan's frontmatter and shows the line
  to write. §FS-rhei-plan-language.3
- Name the artifact rename in the result-link error. Renaming a ticket id is a
  two-file edit — the link and the artifact it points at — and nothing said so.
- Distinguish ticket depth from project level in the plan language spec.
  `structure.maxLevels` counts `###` as depth 1 while the Plan Root Model counts
  Panta as level 0, so `maxLevels: 2` read two ways in one section.
  §FS-rhei-plan-language.3
- Let basin tickets execute. `rhei validate` and `rhei list` accepted them
  while `rhei transition`, `rhei next`, and `rhei run` all failed with
  "Metadata field appears outside a task": the synthetic basin is
  workspace-shaped but has no authored `index.rhei.md`, so its bare task files
  were parsed as whole plans. Basin tickets now keep their runtime metadata in
  `index.panta.md` under their project-qualified ids. §FS-rhei-panta.6.1
- Resolve the plan's own state machine in `rhei states`. It was the only
  command that took no plan argument and never auto-discovered, so inside a
  project declaring `**States:**` it printed the built-in default — naming
  every state wrong for the machine actually in force. It now accepts a plan,
  infers one like every other command, and opens with a `Source:` line.
  §FS-rhei-states-cmd.3
- Return a usage error from `rhei <group>` with no subcommand. `rhei snapshot`
  printed the *root* help to stdout and exited 0, so a malformed invocation was
  indistinguishable from success. A bare `rhei` still prints the root help.
- Preview and confirm `rhei reset`. It destroyed every result and ledger with
  no confirmation and no preview, inside a `panta/` directory `rhei init`
  gitignores by default — so there was usually no copy to recover. Adds
  `--dry-run` and `--yes`, and prompts on an interactive terminal.
  §FS-rhei-reset.1.2
- Reject an unrecognized `**Field:**` in a task's metadata block instead of
  swallowing it as content. A mistyped `**Priorr:**` silently dropped the
  dependency, leaving a plan that validated green and executed in the wrong
  order. Bold text past a blank line is still content.
  §FS-rhei-plan-language.2
- Keep line numbers and code frames on parse errors inside a project. The same
  file reported `path: message` with no line when validated through its project
  and a full code frame when named directly — and the project form is the one
  `rhei init` steers new authors toward. §FS-rhei-panta.6
- Name the unknown rhei behind a missing cross-rhei `**Prior:**`. An unknown
  rhei id is qualified with the citing rhei, so `onbaording.3` surfaced as
  `billing.onbaording.3` — an id the author never wrote. The error now names
  the unknown rhei, lists the project's rheis, and suggests the near miss.
- Report a duplicate task id declared twice in one file as such, with the line
  of the redeclaration. It read "defined in both X and X" and carried no line.
- Explain a ticket id passed in the plan-path slot. `rhei complete auth.1` took
  the id as a path and failed downstream; it now says to use `--task`, and a
  non-existent plan path is reported as such.

- Refuse to complete a ticket whose `**Prior:**` dependencies are unsatisfied,
  and report the same condition from `rhei validate` as a warning. `rhei
  complete` and `rhei transition` both advanced a ticket past its unfinished
  prerequisites silently; once terminal, the ticket left readiness and `rhei
  list --blocked`, so nothing ever revealed that the plan contradicted its own
  dependencies. `rhei complete` now names every blocking prior and its state.
  `rhei transition` keeps advancing without the check — it is the explicit
  human-initiated primitive and so the deliberate override — and validation
  reports the resulting inconsistency instead of letting it pass.
  §FS-rhei-complete.4 §FS-rhei-validate.4 §FS-rhei-transition-cmd.3
- Fail loudly on an authored `index.rhei.md` under `basin/` instead of skipping
  it. The basin's manifest is synthetic, so such a file could never load; it
  was dropped without a word, leaving `rhei validate` green and the tickets
  invisible — the exact disappearance the basin exists to prevent.
  §AR-rhei-panta.1
- Treat a workspace rhei with no task files as a valid, empty rhei rather than
  a load error. One freshly created directory used to fail `rhei list` and
  `rhei validate` for the whole project, with a message that named no file, and
  it masked the duplicate-rhei-id diagnostic. `rhei validate` now warns and
  names the empty rhei, so a mistyped `tasks/` is still caught.
  §FS-rhei-plan-language.1.2
- Report manual-only tickets from `rhei run --dry-run` instead of aborting on
  the first one. Under the built-in machine, whose initial state is
  manual-only, the flag failed before printing anything; the scan now continues
  and lists every blocked ticket alongside the transitions that would run,
  still exiting non-zero. §FS-rhei-run.4
- Accept a bare task id in `**Prior:**` (`**Prior:** auth.2`) alongside the
  kind-prefixed form. Every surface that prints a dependency prints the bare id
  — `rhei list` renders `(prior: auth.2)` — so pasting it back was met with a
  parse error that merely restated the grammar. Malformed references now quote
  what was written. §FS-rhei-plan-language.5
- Stop breaking file paths across lines in diagnostics. miette's defaults
  offered a wrap opportunity at every `/` and `-`, so no path in any error was
  copy-pasteable, clickable, or greppable.
- Surround the `> **Result:**` block written by `rhei complete` with exactly one
  blank line on each side. It landed with two blank lines above and none below,
  butting the following heading against the blockquote and degrading the
  human-reviewed plan file a little more with every completion.
- Name the host files `rhei init` writes or changes outside the project it
  creates, and state that `panta/` is gitignored. Init edits `.gitignore` and
  `AGENTS.md` in someone else's repository; doing it silently is how a team
  discovers weeks later that no plan was ever committed. §FS-rhei-init.5
- Render `rhei viz`'s not-Panta-aware caveat into the generated page, naming
  the rheis missing from it. The warning went to stderr only, while the HTML it
  describes is opened and trusted long after the terminal scrolled.
  §FS-rhei-viz.7.3
- List `rhei init` in `rhei --help`. The top-level command list is a
  hand-maintained template and `init` was never added to it, leaving the first
  command a new user needs invisible; a test now fails when any subcommand is
  missing from it.
- Align the runtime commands on the project/ticket vocabulary the docs use, and
  give the no-work and unknown-ticket messages the next step `rhei list`
  already offers.
- Start every generated completion script with a comment header that shows how
  to enable it: the source-in-current-shell one-liner, the rc file for
  permanent use, and the `--install` alternative; for zsh the `#compdef rhei`
  directive stays on the first line. PR #61 §FS-rhei-completions.5

- Make the shell argument to `rhei completions` optional: detect it from
  `$SHELL` when omitted, and on detection failure list the supported shells
  with a copy-pasteable example instead of clap's bare missing-argument
  error. PR #58 §FS-rhei-completions.2

- Add `rhei init [DIR]`: set up a Panta project in a gitignored `panta/`
  folder inside the host directory — planning is working state by default,
  with its own `.gitignore` inside so un-ignoring the plans later never
  commits runtime output. `--here` makes the host itself the project (the
  adoption mode for versioned plans); over a host that already holds bare
  rheis, default mode refuses and names both fixes. Init writes the minimal
  `index.panta.md` (title derived from the host name), adds a marked
  agent-discovery note to `AGENTS.md` naming where the project lives
  (skippable with `--no-agents`), and reports the rheis discovery now sees.
  When the adopted rheis unanimously declare one state machine, init writes
  it as the project default, since a bare manifest would make that project
  unloadable under the one-machine-per-project rule. An existing project is
  refused untouched; `--force` re-initializes it, overwriting the manifest
  and updating the idempotent companion files in place. `--here` refuses a
  host that already holds a default-mode project at `panta/` — the host
  manifest would shadow it — even under `--force`. The omitted-target
  resolution probes the `panta/` child, so bare commands work from the whole
  host repository. The project machine may keep living in a rhei's own root:
  a unique name match resolves it; several matching rhei-root files are an
  ambiguity error, and an unloadable candidate file is an error rather than
  a silent non-match. The AGENTS.md note names only the worker surface (`list`,
  `next`, `complete`, `validate`) and marks `rhei run` as human-initiated —
  orchestration is never started by an agent. PR #54 §FS-rhei-init
  §FS-rhei-panta.6
- Treat an empty Panta project — the state `rhei init` leaves — as valid:
  loading succeeds with zero tickets, `rhei list` reports how to grow the
  project (or `[]` under `--json`) instead of erroring, and `rhei reset`
  reports a no-op instead of failing. `rhei validate` still
  warns that discovery found no rheis, so a misnamed or misplaced plan is
  never silently invisible behind a green validation. PR #54 §FS-rhei-panta.6
- Resolve an omitted plan target from the current directory: every plan-taking
  command except `rhei reset` walks up to the nearest `index.panta.md`
  project, workspace `index.rhei.md`, or — in the invocation directory only —
  lone rhei (a `*.rhei.md` file or workspace directory, counted like project
  discovery), so `rhei list` inside a project just works. `rhei reset` keeps
  requiring an explicit target: it destroys runtime state, so it never infers
  one. A directory holding several bare rheis without a manifest is an
  error naming the candidates and pointing at `rhei init`; no plan anywhere up
  the tree is an error that says what was searched for. PR #54
  §FS-rhei-panta.6

**Breaking: ticket ids are now project-qualified.** Every load yields a
Panta-rooted graph, so a ticket that used to be `1` is now `auth.1` — named
for the rhei it lives in. This changes ids in command output, result artifact
filenames (`runtime/results/auth.1.md`), ledgers, and logs. Plan files are not
rewritten: task headings stay rhei-local, and a plan completed before this
change keeps its rhei-local result links and artifacts, which keep validating.
A single-file rhei must now be named `<id>.rhei.md`, since the file stem is
where its id comes from. PR #45

- Make Panta the default execution model: a bare rhei — a `.rhei.md` file or a
  Directory Workspace — loads as the single rhei of an implicit Panta, so there
  is one loader and one graph shape whether or not an `index.panta.md` exists.
  §AR-rhei-panta.2 §AR-rhei-panta.3
- Mutate project-wide. `rhei run`, `next`, `transition`, `complete`, and `reset`
  operate across the project, routing every state, assignee, result, and runtime
  rewrite back to the owning rhei file. The previous staged boundary — which
  rejected mutating commands on a project — is gone. A run locks every involved
  execution root (the project's and each member rhei's), so a project-level run
  and a direct run of one of its rheis are mutually exclusive. `--parallel > 1`
  counts only top-level tickets per plan file when warning about same-file
  concurrency, and falls back to sequential when every ticket lives in one
  file. §FS-rhei-panta.6 §FS-rhei-run.2.5 §FS-rhei-run.2.6
- Accept rhei-local shorthand for CLI ticket targets: `rhei complete 1` resolves
  when exactly one in-scope rhei has that ticket, and names the qualified
  candidates when more than one does. §FS-rhei-panta.6
- Add `--rhei <id>` (repeatable) to narrow `run`, `next`, `reset`, and `list` to
  named rheis. It selects candidates without narrowing where their priors
  resolve, so a candidate may still be blocked by a prior outside the scope —
  and the no-work diagnostic now names that prior as out of scope instead of
  reporting out-of-scope work. `run` and `reset` report their resolved scope
  before acting; a one-rhei project has no fan-out to report and stays quiet.
  §FS-rhei-panta.6 §FS-rhei-panta.6.1 §FS-rhei-panta.6.4 §FS-rhei-run.2.5
  §FS-rhei-next.2.2 §FS-rhei-list.2 §FS-rhei-reset.1.1
- Scope a narrowed `rhei reset` to everything keyed by an in-scope ticket —
  result file, logs, declared artifact-contract paths, snapshot sessions,
  worktree refs, accounting captures, its lines in the transition ledger, and
  its runtime visit/poll metadata in the owning workspace rhei's index —
  instead of results and logs alone, and report the run-scoped output it
  deliberately keeps. A stale declared output could otherwise satisfy a
  required input on the next run, and a stale visit count would resume a
  counted loop mid-flight. Legacy pre-qualification records (rhei-local keys)
  are swept too when every rhei at the execution root is in scope.
  §FS-rhei-reset.2.1 §FS-rhei-panta.6.4
- Validate result links as a pair: link text and target must describe the same
  ticket, both qualified or both rhei-local. §FS-rhei-panta.6.3
  §FS-rhei-plan-language.3.8
- Warn when `rhei viz` is pointed at a Panta project: it is not Panta-aware, so
  the page is one disconnected plan per `*.rhei.md`, not the merged project
  graph. §FS-rhei-viz.7.3
- Unify subprocess ids: `RHEI_TASK_ID` is the project-qualified ticket id for
  agents, programs, *and* transition callbacks (callbacks previously received
  the rhei-local id). `RHEI_TASK_ID_LOCAL` and the `{task_id_local}` /
  `{rhei_id}` template variables carry the rhei-local form for scripts and
  instructions that edit or grep the plan file, and the callback context JSON
  gains `task.localId`. §FS-rhei-panta.6 §AR-rhei-panta.3

  Two limits ship with this change, both tracked on the roadmap: one state
  machine still governs a whole project — a rhei declaring a machine different
  from the project default is a load error — and `rhei viz` is not yet
  Panta-aware.

  **Upgrading a pre-qualification workspace.** Nothing is rewritten for you;
  these are the sharp edges and what to do about them:

  - *Plan filenames.* A single-file rhei must be `<id>.rhei.md`, where `<id>`
    starts with a letter and uses only letters, digits, `_`, or `-`. Rename
    files like `My Plan.rhei.md` or `2026-roadmap.rhei.md`; the load error
    suggests a legal name. The same rule applies to Directory Workspace
    directory names.
  - *Scripts and JSON consumers.* Command output, `rhei next` JSON
    (`task_id`), `rhei list --json` (`id`, `prior`, `parent`), `{task_id}`,
    and `RHEI_TASK_ID` all carry qualified ids now. `rhei list --json`'s
    `depth` counts within the owning rhei (a top-level ticket is `1`).
    Scripts that match on heading ids should switch to `RHEI_TASK_ID_LOCAL`.
  - *Mid-flight runtime artifacts.* Artifacts produced under rhei-local names
    (`runtime/results/1.md`, `runtime/worktree-refs/1.yaml`, ledger lines
    `1 pending@…`, `runtime/accounting/tasks/1.json`) are not migrated. State
    history in `rhei viz` and the run dashboard falls back to the rhei-local
    key when a ticket has no qualified records, so an executed plan keeps its
    history after upgrading, and a narrowed reset sweeps rhei-local keys when
    every rhei at the execution root is in scope. Other readers only match
    qualified keys: when a required input exists only under its
    pre-qualification name, the missing-artifact error names that file and
    the rename that fixes it. Finish or reset in-flight tickets before
    upgrading if you want a clean ledger; a full `rhei reset` still clears
    the whole `runtime/` tree.
  - *Snapshot caches.* `.rhei/cache/snapshots/` is keyed by ticket id, so
    caches produced before this change no longer resolve: `rhei snapshot
    list --orphaned` shows them and `rhei snapshot gc` prunes them.
  - *Completed plans.* Legacy rhei-local result links keep validating and are
    left alone; only re-completing a ticket refreshes its link to the
    qualified form. §FS-rhei-panta.6.3
- Add durable task state history to Flow/dashboard and the `rhei run` TUI,
  including the `state history` surroundings section, prompt-focused inspector
  navigation, a global Machine legend with process-kind styling, and links-only
  shared chrome. PR #48 §FS-rhei-viz.4 §FS-rhei-run-tui.1.5
- Add Codex token accounting from `turn.completed` JSON usage, persisted runtime
  accounting artifacts, and live/run surfaces in the TUI, Flow dashboard, run
  report, and `rhei cost`. PR #44 §FS-rhei-cost-accounting.1
  §FS-rhei-cost-accounting.2 §FS-rhei-cost-accounting.4
- Add reusable state prompt templates loaded from sibling
  `prompt_templates/*.md`, with per-state `prompt_template.values` placeholder
  binding, runtime variable resolution inside values, escaped-brace literals,
  and existing inline `personality` / `instructions` preserved. A template
  contributes instructions only; `personality` stays per-state. `rhei validate
  --watch` picks up a `prompt_templates/` directory created after the watch
  starts. PR #46 §FS-rhei-states.4.4
- Run program states in the same live `--parallel` worker pool as agent states,
  so a long-running program consumes one slot while other ready independent work
  continues to be scheduled. PR #43 §FS-rhei-run.5 §FS-rhei-programs.6.3
- Add the Flow-style interactive `rhei run` TUI surface with shared Flow, Machine,
  Cost, Journal, and Tasks views; cross-view filtering; task state filtering;
  custom terminal-state readiness; and human-gate liveness for both agent and
  callback runs. PR #42 §FS-rhei-run-tui.1.5 §FS-rhei-run-tui.1.5.2
- Write a durable per-run Markdown report at the end of every `rhei run` to
  `runtime/run-report.md` (latest) and `runtime/run-reports/<timestamp>-<run-id>.md`
  (history): header, outcome strip, attention list, transition ledger, source-order
  task final states, and spawned invocations with relative log links. The non-TTY
  path now prints a greppable `Report:` pointer, and a run that advanced tasks
  without spawning any agent or program is called out so reused-output advances are
  not mistaken for fast work. The report is also written for runs that abort with
  an error mid-execution; a `--dry-run` stays side-effect-free and writes nothing.
  PR #41 §FS-rhei-run-report.1 §FS-rhei-run-report.4
- Add task-level execution overrides with `**Model:**` and `**Target:**`,
  including validation, agent resolution precedence, transition artifact checks,
  and canonical example coverage. PR #40 §FS-rhei-plan-language.3.11
- Print a console-first end-of-run summary when `rhei run` exits on an
  interactive terminal: a result line, a state-distribution bar, run counts, an
  attention list of gated/blocked tasks, and a source-order task tree with
  per-task driver, duration, and final-state markers. Non-TTY output is
  unchanged so scripts and CI keep matching it. PR #39 §FS-rhei-run-report.3
- Detect when an agent-created commit leaves tracked Rhei-owned plan/result
  state uncommitted after `rhei run` applies its orchestrator transition, and
  report a clear error instead of silently reporting durable success. PR #38
- Run agents from checkout roots so repository `AGENTS.md` files and task
  worktrees are visible while Rhei artifacts stay rooted at the plan workspace.
  PR #35
- Fix `rhei run` auto-advance for nested agent tasks after required output
  artifacts are written. PR #33
- Clear stale Flow dashboard running indicators after the live loopback server
  stops answering, so closed runs do not leave browser tabs spinning forever.
  PR #31
- Simplify the built-in state machine to the manual `pending` -> `completed`
  flow, preserve durable manual claims from `rhei next`, and make `rhei run`
  refuse to auto-complete default manual tasks. PR #30
- Clarify the first-run example path, Panta's current read-only project support
  boundary, and runnable example discovery; fix `xtask` example copying for
  fixtures that contain snapshot symlinks. PR #28
- Fix stale template-author guidance, Flow inspector wording, and local Claude
  registration ignore handling after the settings-path and runtime-slot
  changes. PR #26
- Fix Flow running-now and running summary counts to use active runtime slots
  instead of persisted active-like task states. PR #23
- Fix Claude Code live intervention transport by using stream-json stdin with
  verbose print output when `intervene_stdin` is enabled. PR #25
- Move project settings from `.rhei/settings.json` to
  `.agents/rhei/settings.json`, including template instantiation output. PR #22
- Tighten `rhei-template-writer` skill guidance for editing existing templates
  and validating rendered `**Prior:**` metadata. PR #21
- Improve `rhei instantiate` template discovery help by listing templates when
  no template is provided and suggesting close matches for missing named
  templates. PR #20
- Remove the `rhei lsp` language-server product surface. PR #18
- Add product workflow templates and examples for agent discussion,
  analyze-and-dispatch, parallel worktrees, multi-model analysis, and spec
  review. PR #17
- Add live dashboard controls for explicit human-gate transitions. PR #16
- Add GitHub Actions CI, pre-commit hooks, and PGO release automation modeled on
  Grund's release flow. PR #15

## 1. [0.1.0] - 2026-05-21

- Initial alpha release line for the Rhei CLI, Rust crates, npm wrappers, and
  PyPI wrappers.

## 2. Older releases
