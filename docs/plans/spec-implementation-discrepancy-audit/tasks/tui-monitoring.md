### Task tui-monitoring: Audit run TUI, journal, and live monitoring behavior
**State:** scope-spec

Audit the monitoring and terminal UI surface for running plans.

Spec scope:

- `docs/specs/rhei-run-tui.spec.md`
- TUI and journal requirements referenced by `docs/specs/rhei-run.spec.md`

Implementation scope:

- `crates/rhei-tui/src/`
- TUI event emission and frontend selection in `crates/rhei-cli/src/main.rs`
- tests covering event capture, journal output, TUI rendering, and run output

Focus on event semantics, transition journal format, stdout compatibility,
agent traffic interception, lifecycle event preservation, log tailing, slot
layout, non-TTY behavior, `--tui` / `--no-tui`, and failure/timeout display.
