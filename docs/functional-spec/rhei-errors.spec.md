# FS-rhei-errors: CLI Errors and Guidance

Every failure Rhei reports must tell the user three things: what failed, why, and
what to run next. An error a user cannot act on without reading the source or
the spec is a defect, not a diagnostic.

This follows from [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals): execution is predictable only when the
tool says how to get unstuck, and simple work stays simple only when a wrong
first command teaches the right second command.

## 1. Anatomy of an Error

A CLI error has a **message** and, whenever the user can fix it, a **help**
line. Miette renders them as:

```
  × template 'analyze-and-dispatch' is missing 2 required inputs
  │   subject — What the coordinator analyzes ...
  │   analysis_brief — How the coordinator should analyze the subject ...
  help: rhei instantiate analyze-and-dispatch subject='<value>' analysis_brief='<value>'
        List every input with: rhei instantiate analyze-and-dispatch --list-inputs
```

### 1.1. Message

The message names the failing subject in quotes and states the failure in one
clause. It reports **all** instances of the same failure at once — one missing
input per invocation turns supplying inputs into a guessing loop where each
attempt buys exactly one more field name.

Values the user must go back and retype are one failure class, so a single run
reports every missing input together, and every supplied-but-rejected input
together: a wrong type, a failing `validate` pattern, and a failing `format`
all cost the same one round trip. A batched report keeps each entry's own
message and its own help rather than collapsing them into a summary.

### 1.2. Help

The help line carries the next action. In order of preference it is:

1. A complete, runnable command that fixes the failure.
2. A precise edit: the file, the key, and the value shape to write.
3. A command that reveals the information the user is missing
   (`--list-inputs`, `rhei states`, `rhei templates`).

A runnable command reproduces the invocation the user actually typed — the
arguments they already supplied plus the correction — so it can be pasted
without re-deriving anything.

A correction is offered in a form the CLI accepts. A value the user can assign
is shown as the assignment; a scalar nested inside an array or object has no
assignment syntax of its own, so the correction names the enclosing input and
shows the corrected value alone. A suggestion that pastes back as a *different*
error is worse than none, because it costs a round trip to discover that.

Errors that cannot be user-caused (broken internal invariants) carry help that
says so and asks for a bug report; they never invent a remedy.

### 1.3. Near Misses

When a name is not found and a declared set of valid names exists, the error
suggests the closest match and, when the set is small, lists it. Unknown agent
ids, states, template names, template inputs, task ids, and program names all
follow this rule.

A listing is a substitute for a near miss, not an inventory: past eight
candidates the error names the first few, says how many remain, and defers to
the command that lists them all. Each candidate appears once — registries are
built by merging built-in entries with user settings, and a name present in
both is still one name.

## 2. Copy-Paste Safety

Any command Rhei prints — in an error, a help line, or a success summary — must
survive being pasted into an interactive shell: every value parses as the single
argument Rhei meant, and no token is ever split across lines.

A command wider than the terminal still wraps, at a space. Rejoining two lines
is a visible, recoverable inconvenience; a path or selector broken mid-token is
neither, which is why the renderer breaks only at spaces. Suggestions are kept
short for the same reason — paths are printed relative to the working directory
when they sit beneath it.

Values are POSIX-quoted whenever they contain characters outside
`[A-Za-z0-9_.,:/@%+=-]`, and whenever they begin with `=`, which zsh expands to
a command path. This is not cosmetic: execution target selectors contain `[`
and `]`, and an unquoted `agent=codex[yolo]:openai:gpt-5.5` fails in zsh with
`no matches found` before Rhei is ever executed. Printed selectors are
therefore always quoted as `agent='codex[yolo]:openai:gpt-5.5'`.

The quoting is the platform's own, because the shell the command is pasted into
is: a value that needs quoting is wrapped in POSIX single quotes on Unix and in
`cmd`'s double quotes, with any embedded `"` doubled, on Windows.

In a `KEY=VALUE` argument only the value is quoted, so the key — which is what
the suggestion is teaching — stays readable.

The same rule applies to documentation and to `--list-inputs` output, which is
read as a source of copyable values. Where a default is shown as a multi-line
block for readability, the block is followed by a `copy:` line carrying the
same value as a single quoted assignment, because the block's own scalars are
bare YAML.

A repair example is keyed to the input it corrects. A suggestion built around a
guessed name is not a next action: pasted back, it fails on the name rather
than on the value.

## 3. Failing at the Input Boundary

A value is validated where the user supplied it, not where it is eventually
consumed. Template inputs are checked during `rhei instantiate` argument
resolution — before any file is rendered — so the error names the input the user
typed rather than the rendered artifact that failed to load.

### 3.1. Execution Target Inputs

A template input declared with `format: execution-target` is parsed as an
execution target selector ([§FS-rhei-agents](rhei-agents.spec.md#fs-rhei-agents-rhei-agents-specification)) at instantiation time. A malformed
value reports the input name, the offending value, the accepted shapes, and a
corrected, shell-quoted example built from the value the user supplied.

## 4. Paths in Errors

An error never points at a path the user cannot inspect. Temporary directories
used for `--dry-run` rendering, and output directories that instantiation
removes on failure, are not named as if they were user artifacts; the error
names the input or template that produced the bad content instead.

A filesystem failure while rendering into scratch space names the file by its
position inside the template — a path the user can open — and points the remedy
at `$TMPDIR` rather than at a directory that no longer exists.

## 5. Machine-Readable Errors

Commands with a JSON output mode emit errors as a single-line JSON object on
stderr. The object carries the help text alongside the message so machine
consumers see the same next action as humans:

```json
{"error":{"message":"...","help":"..."}}
```

`help` is omitted when the error carries none.

## 6. Coverage

The contract applies to every diagnostic `rhei` prints on a failing exit path.
Filesystem failures derive their help from the underlying error kind (missing
path, permission, already-exists) and always name the path.

Coverage is a property of the whole binary rather than of any one call site, so
it is enforced by a test that fails when a diagnostic is raised without help,
not left to review.

Help is derived from the failure, never assigned by the area of code it sits
in. A remedy that does not act on the reported failure — telling the user to
check a destination for a command that writes to stdout, or to edit the state
machine for a `waitpid` failure — is the same defect as no help at all, and the
honest answer for a cause the user cannot have created is §1.2's bug report.
Recurring categories share one wording so that improving a remedy improves
every site that reaches it.

## Related

- [Templates Specification](rhei-templates.spec.md) [§FS-rhei-templates](rhei-templates.spec.md#fs-rhei-templates-rhei-templates-specification) —
  instantiation inputs and `--list-inputs`
- [Agents Specification](rhei-agents.spec.md) [§FS-rhei-agents](rhei-agents.spec.md#fs-rhei-agents-rhei-agents-specification) — execution target
  selector grammar
- [Validate Specification](rhei-validate.spec.md) [§FS-rhei-validate](rhei-validate.spec.md#fs-rhei-validate-rhei-validate) — plan and
  state machine validation diagnostics
