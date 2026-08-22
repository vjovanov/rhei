# FS-rhei-new: `rhei new`

Add a rhei or a ticket without knowing the plan format. Everything Rhei
executes is authored markdown, and that is a feature — a plan is readable,
diffable, and editable by hand. But it makes the *first* file a wall: before
`rhei new`, adding a rhei meant knowing that the file is `<id>.rhei.md`, that
its id comes from the file stem, that the heading is `# Rhei: <title>`, and
that a `## Tasks` section has to follow it. Adding a ticket meant knowing the
metadata field order and which state the rhei's machine starts in.

`rhei new` writes that file, or that ticket, from a title and flags. It is the
zero-friction "add a rhei" action Panta exists to make possible
(§FS-rhei-panta.2), extended to tickets because the same wall stands in front
of the second thing a user does.

The command is deliberately **complete**: every field the plan language lets an
author write on a new ticket has a flag, so `rhei new` is never the thing that
gets you started and then abandons you to an editor for `**Prior:**`. What it
does not do is *change* anything — it only creates. Editing an existing ticket
stays a file edit, and changing state stays `rhei transition`
(§FS-rhei-plan-language.1.4).

## 1. Usage

```bash
rhei new <TITLE> [--under <PARENT>] [options]
```

`TITLE` is the human-readable title. With no `--under`, `rhei new` creates a
**rhei** under Panta (§2). With `--under`, it creates a **ticket** inside the
named rhei or under the named ticket (§3). The presence of `--under` is what
selects the mode, and flags belonging to the other mode are a hard error
rather than a silent no-op (§5.3).

### 1.1. Shared options

| Flag                  | Default          | Description                                                       |
|-----------------------|------------------|-------------------------------------------------------------------|
| `--project <PATH>`    | inferred         | The project or plan to write into, resolved exactly as every other command resolves one: omitted, the enclosing project, workspace, or lone plan; named, a member rhei widens to the project it belongs to (§FS-rhei-panta.6) |
| `--id <ID>`           | derived          | Explicit id, replacing derivation from the title (§4)             |
| `--description <TEXT>`| empty            | Body content — the ticket's description, or the rhei's lead paragraph |
| `--description-file <PATH>` | —          | Read the description from a file; `-` reads standard input        |
| `--dry-run`           | off              | Print the target path and the markdown that would be written; write nothing |
| `--json`              | off              | Emit the created id, kind, path, and state as JSON                |
| `--keep-on-error`     | off              | Keep the write when validation fails, instead of rolling it back (§5.2) |

`TITLE` occupies the positional slot that every other command gives to a plan
path, which is why the plan is named with `--project` here. Creating something
is the one operation whose subject is a name rather than a file, and taking the
title positionally is what makes `rhei new "Authentication"` read as one
thought.

### 1.2. Options that create a rhei

| Flag                    | Default              | Description                                                    |
|-------------------------|----------------------|----------------------------------------------------------------|
| `--dir`                 | off                  | Create a Directory Workspace rhei instead of a single file (§2.1) |
| `--states <NAME>`       | project default      | Write a `**States:**` declaration, binding this rhei to its own state machine (§AR-rhei-panta.4) |
| `--max-levels <N>`      | unset                | Write `structure.maxLevels`                                    |
| `--node-kinds <K,...>`  | unset                | Write `structure.nodeKinds`                                    |

### 1.3. Options that create a ticket

| Flag                       | Default            | Description                                              |
|----------------------------|--------------------|----------------------------------------------------------|
| `--under <PARENT>`         | —                  | Owning rhei id (`auth`, `basin`) for a top-level ticket, or ticket id (`auth.1`) for a subtask |
| `--kind <KIND>`            | `task`             | Heading keyword, checked against `structure.nodeKinds` (§FS-rhei-plan-language.3.7) |
| `--state <STATE>`          | the machine's initial | `**State:**`, checked against the owning rhei's machine (§3.2) |
| `--prior <ID>`             | none               | `**Prior:**` entry; repeatable, and a comma-separated list is accepted |
| `--provides <NAME>`        | none               | `**Provides:**` entry; repeatable (§FS-rhei-plan-language.3.12) |
| `--consumes <ID:NAME>`     | none               | `**Consumes:**` entry; repeatable (§FS-rhei-plan-language.3.12) |
| `--assignee <WHO>`         | none               | `**Assignee:**`                                          |
| `--model <MODEL>`          | none               | `**Model:**` (§FS-rhei-plan-language.3.11)               |
| `--target <TARGET>`        | none               | `**Target:**` (§FS-rhei-plan-language.3.11)              |

Repeatable fields are written in the order given, comma-separated, on one line.
A `--prior` value is written through unchanged, so both authored forms work:
`--prior "Task 1"` keeps the node-kind keyword the plan language allows, and
`--prior auth.1` writes the bare cross-rhei reference. `rhei new` deliberately
does not invent a keyword the author did not ask for, and does not resolve the
reference itself — an unresolvable prior is a validation error with a code
frame (§FS-rhei-validate.4.1), which is a better report than anything the
create path could produce.

## 2. Creating a rhei

`rhei new "Authentication"` derives the id `authentication` from the title (§4)
and writes `authentication.rhei.md` next to `index.panta.md`:

```markdown
# Rhei: Authentication

## Tasks
```

That is the whole file. A rhei with no tickets is valid — the Directory
Workspace format has always accepted one, and as of this command the
single-file format does too (§FS-rhei-plan-language.1.1). Seeding a
placeholder ticket instead was rejected: a placeholder is indistinguishable
from real work to `rhei next` and `rhei run`, so the first thing a new project
would do is dispatch an agent onto a ticket nobody wrote.

With the optional header fields, the order is fixed by the plan language —
heading, `**States:**`, frontmatter, description, `## Tasks`:

```markdown
# Rhei: Billing
**States:** billing-review

---
structure:
  maxLevels: 3
  nodeKinds: [task, bug]
---

Everything invoice-related, including dunning.

## Tasks
```

`--states` only writes the declaration; it does not create the machine. A rhei
declaring a machine that no `states.yaml` provides is an error at the next
load, naming where the file is looked for (§AR-rhei-panta.4). This is the
honest order: the machine is authored, and the rhei points at it — so the
create is rolled back like any other invalid one (§5.2), and writing the rhei
*before* its machine takes `--keep-on-error`.

### 2.1. Layout

The default is a single file, because that is the format a rhei can be read in
one screen and converted out of later. `--dir` creates the Directory Workspace
shape instead (§FS-rhei-plan-language.1.2):

```text
billing/
  index.rhei.md      ← the header fields above, without `## Tasks`
  tasks/             ← empty; ticket files land here
```

The workspace index carries no `## Tasks` section: in that format tickets live
in `tasks/` files, and an empty `tasks/` directory is a valid empty rhei.

Creating a rhei requires a Panta project, and the target is resolved the way
every other command resolves one. Standing in a member rhei's directory — or
naming that member with `--project` — creates the new rhei in the project the
member belongs to, and the widening is announced exactly as `rhei validate`
announces it (§FS-rhei-validate.1.1). Only when the resolved target really is a
lone plan or a bare workspace does the command say so and point at `rhei init`
— a rhei is a *member* of a project, and there is nowhere to put a second one
otherwise.

## 3. Creating a ticket

`--under` names where the ticket goes:

```bash
rhei new "Rotate signing keys" --under auth       # top-level ticket in rhei auth
rhei new "Handle expiry" --under auth.3           # subtask of ticket auth.3
rhei new "Fix the typo in the footer" --under basin   # unfiled capture
```

A single-segment value naming a rhei is the owning rhei; any other value is
read as a ticket id and the new ticket becomes its child. A value that is
neither is an error listing the rhei ids in the project.

`--under basin` is how a ticket gets captured without choosing a domain rhei
first (§FS-rhei-panta.2). The basin directory is created on demand; nothing
else is generated, because the basin's manifest is synthetic by design
(§AR-rhei-panta.1). Filing it into a domain rhei later stays a file move.

The written ticket carries the fields that were asked for, in plan-language
order:

```markdown
### Task 4: Rotate signing keys
**State:** pending
**Prior:** Task 3
**Assignee:** vj

Rotate the JWT signing keys and publish the new JWKS.
```

### 3.1. Where it is written

In a single-file rhei the ticket is appended inside `## Tasks` — at the end for
a top-level ticket, and immediately after the parent's existing subtree for a
subtask, so the file stays in id order. In a Directory Workspace, a top-level
ticket becomes a new file in `tasks/`, while a subtask is appended to the file
that already holds its parent: a task file owns a subtree, and splitting one
across files would put a parent and its child in different diffs.

Nothing else in the file is rewritten. `rhei new` inserts its own block and
leaves every other byte — including hand-authored spacing and comments — as it
found it. The block is written with the line terminator the file already uses,
so a CRLF file stays CRLF; existing blank lines are never removed, and a single
blank separator is emitted only where the line before the insertion point is
not already blank. A `git diff` after a create shows the added lines and
nothing else, which is the whole point: a create the author cannot review as a
three-line diff is a create they have to re-read the file to trust.

### 3.2. State

`--state` is omitted in the normal case and the ticket starts in the initial
state of the machine that governs it — resolved through the *owning rhei*
(§AR-rhei-panta.4), and through the node profile matching the new ticket's kind
and depth when the machine declares profiles (§FS-rhei-states.9.2). A ticket
created in rhei `billing` starts in `billing`'s initial state even when the
project default machine starts somewhere else.

`--state` names a different starting state, and is checked against that same
machine. This is initial authoring, which is the one moment the plan language
permits a state to be written directly; every change after it goes through
`rhei transition` and its compare-and-swap, callbacks, and ledger
(§FS-rhei-plan-language.1.4). `rhei new` does not run `on_enter` callbacks for
the state it writes, for the same reason authoring a plan by hand does not: the
ticket is being created in that state, not transitioning into it.

### 3.3. Depth and kind

A subtask deeper than the rhei's `structure.maxLevels` is refused before
anything is written, naming the limit and where it is declared. `--kind` is
checked against `structure.nodeKinds` the same way (§FS-rhei-plan-language.3.7),
so a mistyped kind is a message about the kinds this rhei declares rather than
a parse error on the next command.

## 4. Ids

A rhei id is derived from the title: lowercased, with every run of characters
outside `[a-z0-9_-]` replaced by a single `-`, trimmed, and with any leading
non-letter dropped — `"Authentication"` → `authentication`, `"Billing & Dunning"`
→ `billing-dunning`. A title that derives nothing usable is an error asking for
`--id`.

A ticket id is the next free number among its siblings: one more than the
highest numeric sibling, starting at 1. Named ids are not derived — a title
never becomes a named ticket id, because ticket ids are referenced by hand in
every `**Prior:**` and a generated name is a worse identifier than a number.
`--id fix-cache` writes one explicitly.

`--id` bypasses derivation in both modes and is validated, not sanitized: an
id that is not legal is refused with the reason, never quietly repaired.

Three ids are refused outright:

- one that **collides** with an existing rhei or sibling ticket, naming the
  holder and pointing at `--id`;
- `basin` as a *rhei* id, which is permanently reserved for the synthetic
  basin rhei (§FS-rhei-panta.2) — the refusal is at create time rather than at
  the next load, where it would arrive as a broken project;
- an id that is not a legal single-segment rhei id (§AR-rhei-panta.3) or a
  legal ticket id segment.

Two concurrent `rhei new` invocations against one rhei can derive the same
ticket number. The second write is then a duplicate id, which validation
catches and reports at once (§FS-rhei-plan-language.3.5); `rhei new` does not
take a lock for a race that costs one re-run.

## 5. Write, validate, report

### 5.1. A create is verified, not assumed

`rhei new` never writes over a file that is already there. The destination of a
new task file is derived from the id and the title, so a file already sitting
at that name holds someone else's work — the refusal names the path and points
at `--id`, before anything is written. Creating is not editing (§6), and an
unconditional write is editing with the diff thrown away.

After writing, `rhei new` loads and validates the project the way
`rhei validate` does, and then *reloads the plan and looks up the id it just
created*. Both halves are needed, and neither implies the other. Validation
says the project still parses; the reload says the new node is actually in it.
A block appended after an unterminated code fence, or spliced where the parser
ends a section earlier than the writer assumed, leaves the project valid and
the ticket absent — and reported as success, that is a file the author has to
debug by hand, one id short and with the next create about to reuse the number.
When the id does not come back from the file it was written to, the create has
failed like any other, names the file, and is rolled back (§5.2).

A create that leaves the project unloadable has likewise not succeeded, and
reporting it at the next unrelated command is how a small mistake — a mistyped
`--model`, a `--prior` naming nothing — becomes a confusing one.

### 5.2. A create answers for the errors it introduced

The validation pass runs twice: once *before* the write and once after it. What
decides the outcome is the difference between them.

- Errors the create introduced — a `--prior` naming nothing, a `--states`
  naming a machine no `states.yaml` provides — undo the write: a created file
  is removed, and a modified file is restored byte-for-byte. The report lists
  only those new errors, with the validator's own code frames.
- Errors that were already there do not. When the post-write errors are the
  ones the pre-write pass already found, the write is **kept** and the command
  succeeds, with a warning saying the project was already failing validation
  and that the failure is not this create's.

The second rule is the point of running the pass twice. `rhei new` is the
on-ramp: a project with one broken rhei is exactly the project someone is
trying to add a working rhei to, and a create that refuses until everything
else is fixed refuses precisely when it is most needed. It is also simply
untrue to say "nothing was written because the project would not validate with
it" about a create whose own output is fine — the command would be blaming
itself for the state it found.

Rolling back a create's *own* errors is still the right default, because that
failure is nearly always in the flags rather than in the file: re-running with a
fixed flag is the fix, and a half-created ticket in the way of that re-run is
pure friction. `--keep-on-error` keeps the write for inspection, and then says
that the project is left failing validation.

### 5.3. Mode confusion is an error

`--dir`, `--states`, `--max-levels`, and `--node-kinds` create a rhei; every
flag in §1.3 creates a ticket. Passing one with the other mode's selector is
refused, naming both flags. A flag silently ignored because the *other* flag
decided the mode is the worst outcome available: the command reports success
and the field the user asked for is not there.

### 5.4. What it prints

The default output names what was created and where, then the one command that
follows:

```text
Created rhei "Authentication" as `authentication` at panta/authentication.rhei.md
Next: `rhei new "<first ticket>" --under authentication`
```

```text
Created ticket auth.4 "Rotate signing keys" [pending] in panta/auth.rhei.md
```

`--json` emits the same facts as an object (`kind`, `id`, `title`, `path`, and
`state` for a ticket) for scripts that create tickets in bulk. `--dry-run`
prints the target path and the exact markdown block, and touches nothing.

Together, `--dry-run --json` emits that same object with `"dry_run": true` and
the block under `"markdown"`, so a script can preview a bulk create the same
way it reads a real one. A flag that selects the output format has to keep
working under a flag that only selects whether the write happens: silently
handing prose to a caller that asked for JSON is the §5.3 failure again, in the
one place a caller cannot notice it.

## 6. What `rhei new` does not do

- It does not edit or move anything that already exists. No re-titling, no
  re-parenting, no state changes — `rhei transition` and `rhei complete` own
  state, and everything else is a file edit.
- It does not scaffold from a template. `rhei instantiate` writes a rhei
  complete with its tickets and its own state machine (§FS-rhei-templates);
  `rhei new` writes an empty one. They are separate because a blank rhei should
  not require a template system to exist.
- It does not create a project. That is `rhei init` (§FS-rhei-init), and the
  two compose: `rhei init && rhei new "Auth"`.
- It does not write a `states.yaml`. `--states` declares which machine a rhei
  runs under; authoring the machine is `rhei-state-machine-writer` territory
  (§FS-rhei-state-machine-writer).
- It does not check that a `**Consumes:**` reference resolves to a declared
  `**Provides:**` — nothing does yet (§FS-rhei-plan-language.3.12).
