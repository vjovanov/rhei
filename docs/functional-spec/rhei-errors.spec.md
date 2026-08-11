# FS-rhei-errors: CLI Errors and Guidance

Every failure Rhei reports must tell the user three things: what failed, why, and
what to run next. An error a user cannot act on without reading the source or
the spec is a defect, not a diagnostic.

This follows from §GOAL-rhei-outcomes: execution is predictable only when the
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

### 1.2. Help

The help line carries the next action. In order of preference it is:

1. A complete, runnable command that fixes the failure.
2. A precise edit: the file, the key, and the value shape to write.
3. A command that reveals the information the user is missing
   (`--list-inputs`, `rhei states`, `rhei templates`).

A runnable command reproduces the invocation the user actually typed — the
arguments they already supplied plus the correction — so it can be pasted
without re-deriving anything.

Errors that cannot be user-caused (broken internal invariants) carry help that
says so and asks for a bug report; they never invent a remedy.

### 1.3. Near Misses

When a name is not found and a declared set of valid names exists, the error
suggests the closest match and, when the set is small, lists it. Unknown agent
ids, states, template names, template inputs, task ids, and program names all
follow this rule.

## 2. Copy-Paste Safety

Any command Rhei prints — in an error, a help line, or a success summary — must
survive being pasted into an interactive shell.

Values are POSIX-quoted whenever they contain characters outside
`[A-Za-z0-9_.,:/@%+=-]`. This is not cosmetic: execution target selectors
contain `[` and `]`, and an unquoted `agent=codex[yolo]:openai:gpt-5.5` fails in
zsh with `no matches found` before Rhei is ever executed. Printed selectors are
therefore always quoted as `agent='codex[yolo]:openai:gpt-5.5'`.

The same rule applies to documentation and to `--list-inputs` output, which is
read as a source of copyable values.

## 3. Failing at the Input Boundary

A value is validated where the user supplied it, not where it is eventually
consumed. Template inputs are checked during `rhei instantiate` argument
resolution — before any file is rendered — so the error names the input the user
typed rather than the rendered artifact that failed to load.

### 3.1. Execution Target Inputs

A template input declared with `format: execution-target` is parsed as an
execution target selector (§FS-rhei-agents) at instantiation time. A malformed
value reports the input name, the offending value, the accepted shapes, and a
corrected, shell-quoted example built from the value the user supplied.

## 4. Paths in Errors

An error never points at a path the user cannot inspect. Temporary directories
used for `--dry-run` rendering, and output directories that instantiation
removes on failure, are not named as if they were user artifacts; the error
names the input or template that produced the bad content instead.

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

## Related

- [Templates Specification](rhei-templates.spec.md) §FS-rhei-templates —
  instantiation inputs and `--list-inputs`
- [Agents Specification](rhei-agents.spec.md) §FS-rhei-agents — execution target
  selector grammar
- [Validate Specification](rhei-validate.spec.md) §FS-rhei-validate — plan and
  state machine validation diagnostics
