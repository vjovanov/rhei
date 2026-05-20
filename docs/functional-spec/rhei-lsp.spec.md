# FS-rhei-lsp: Rhei Language Server

Rhei provides a Language Server Protocol (LSP) server so editors can give plan
authors immediate feedback while they write `.rhei.md` plans. The server runs
over stdio from the installed `rhei` binary and reuses the same parser, state
machine resolution, and semantic validator that power `rhei validate`.
§GOAL-rhei-outcomes §FS-rhei-validate

## 1. Usage

```bash
rhei lsp
rhei --state-machine <PATH> lsp
```

`rhei lsp` reads LSP messages from stdin and writes protocol responses and
notifications to stdout. Human logs and fatal startup errors go to stderr so
stdout remains a valid LSP transport stream. §FS-rhei-completions

## 2. Document Synchronization

The server advertises full text synchronization. It tracks the latest text for
each open document from `textDocument/didOpen`, `textDocument/didChange`, and
`textDocument/didClose`. Closed documents are removed from memory and receive an
empty `textDocument/publishDiagnostics` notification.

The v1 server supports file-backed single-plan documents. Untitled or unsaved
documents still receive parser diagnostics and built-in state-machine checks,
but filesystem-dependent validation is best-effort because no workspace root is
available.

## 3. Diagnostics

On open, change, and save, the server publishes diagnostics for the changed
document. Parser diagnostics use the parser's source line when available.
Semantic validation diagnostics use document-level ranges when the validator
does not provide a narrower source span. Errors use LSP severity `Error`;
warnings use LSP severity `Warning`. §FS-rhei-plan-language §FS-rhei-validate

The LSP server must not mutate plans, run callbacks, spawn agents, write runtime
artifacts, or perform execution-time snapshot/accounting maintenance while
validating an editor buffer. It is an authoring surface, not an execution
surface. §FS-rhei-authoring

## 4. Completion

The server offers completions that are safe to compute from the current buffer
and resolved state machine:

- `**State:**` values complete state names from the resolved state machine.
- `**Prior:**` values complete task IDs found in the current document.
- Node headings complete the default node kind `Task` when the context is a
  Rhei node heading prefix.

Completions are advisory and must not change command semantics or plan parsing.
They complement, but are separate from, shell completion scripts.
§FS-rhei-completions

## 5. Navigation And Hover

The server exposes document symbols for the plan heading and task headings,
hover text for resolved state names, and definition lookup for task IDs by
jumping to the matching task heading in the same document. Navigation is
best-effort: malformed documents should still return whatever symbols and task
locations can be scanned from the open buffer.

## 6. Product Boundary

This command is the launchable language-server product surface. Editor
extensions and editor-specific setup docs may wrap it, but the stable contract
is the stdio command above. Future editor packages should launch this binary
rather than reimplementing Rhei parsing or validation.
