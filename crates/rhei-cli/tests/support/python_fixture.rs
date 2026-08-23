// §REQ-cross-platform.4

// The mock agents, programs, callbacks, and redactors the CLI's test harnesses
// stand up.
//
// They were `#!/bin/sh` scripts, which is the single reason the e2e target ran
// on Unix only: a fixture that writes a result file and exits with a code has
// nothing Unix about it. Python is installed on every GitHub runner and this
// repository already depends on it (`scripts/*.py`, the pre-commit hook), so a
// fixture written in Python needs no shell, no `chmod`, and no `.exe`.
//
// Shared by both harnesses; the comments are `//` because one of them pulls
// this file in with `include!`, where an inner doc comment cannot open a file.

/// The interpreter every fixture runs under: `python3`, or `python` where that
/// is the only name (which is how Windows spells it). Probed once — a probe per
/// fixture would spend a process per test.
pub fn python_command() -> &'static str {
    static PYTHON: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    PYTHON.get_or_init(|| {
        // `python` first on Windows: `python3` there is often the Microsoft
        // Store's launcher stub rather than an interpreter.
        let candidates = if cfg!(windows) { ["python", "python3"] } else { ["python3", "python"] };
        for candidate in candidates {
            let runs = std::process::Command::new(candidate)
                .arg("-c")
                .arg("pass")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|status| status.success())
                .unwrap_or(false);
            if runs {
                return candidate;
            }
        }
        panic!("these tests run their fixtures under Python: put `python3` or `python` on PATH")
    })
}

/// What a fixture body may assume without importing anything.
///
/// `env` reads the agent environment contract, with `${VAR:-default}`
/// semantics. `write`, `append`, and `result` create the parent directory the
/// way `mkdir -p` used to. Every path is a `pathlib.Path` joined a segment at
/// a time, never a string with a separator in it, so a fixture cannot spell a
/// path in one platform's dialect.

// §FS-rhei-agents.4
const FIXTURE_PRELUDE: &str = r#"import os
import pathlib
import sys
import time

# Every stream a fixture writes is opened with `newline=''`, so a line ends with
# one `\n` on every platform. Python's text mode would otherwise translate it to
# `\r\n` on Windows, and a test comparing a fixture's output byte for byte would
# see two different files for one program.
if hasattr(sys.stdout, 'reconfigure'):
    sys.stdout.reconfigure(newline='\n')
    sys.stderr.reconfigure(newline='\n')


def env(name, default=''):
    """`${NAME:-default}`: an empty value counts as unset, as it does in sh."""
    value = os.environ.get(name)
    return value if value else default


def write(path, text):
    target = pathlib.Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    with target.open('w', encoding='utf-8', newline='') as handle:
        handle.write(text)


def append(path, text):
    target = pathlib.Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    with target.open('a', encoding='utf-8', newline='') as handle:
        handle.write(text)


def result(text):
    """The ticket's own account of where it ended. §FS-rhei-states.3.3

    Not every invocation is given one — `rhei snapshot continue` spawns an
    agent outside a run — so an unset path is nothing to write, not an error.
    """
    path = env('RHEI_RESULT_PATH')
    if path:
        write(path, text)


"#;

/// Write `body` as a fixture script called `name` under `dir`, and return its
/// path. Every fixture in the suite goes through here, so the prelude and the
/// interpreter are decided in one place.
///
/// Quote the strings inside `body` with `'…'`. A fixture writes markdown, so a
/// `"` followed by `#` is common — and that sequence closes the Rust raw
/// literal the body is written in.
pub fn write_python_agent(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, format!("{FIXTURE_PRELUDE}{body}"))
        .expect("fixture script should be written");
    path
}

/// The `["python3", "<script>"]` command array that runs `script`, already
/// encoded.
///
/// Both elements go through `serde_json`, because a Windows path is full of
/// backslashes and a settings file that spells one literally reads them as
/// escapes. JSON is a subset of YAML, so a state machine's `program.command:`
/// takes the same string unchanged.
pub fn fixture_command(script: &std::path::Path) -> String {
    fixture_command_with_args(script, &[])
}

/// [`fixture_command`] with fixed arguments after the script.
pub fn fixture_command_with_args(script: &std::path::Path, args: &[&str]) -> String {
    let mut command = vec![python_command().to_string(), script.display().to_string()];
    command.extend(args.iter().map(|arg| (*arg).to_string()));
    serde_json::to_string(&command).expect("fixture command should serialize")
}

/// The command *line* that runs `script`, rather than the argument vector.
///
/// Two places take one: a `cli:` callback and a string-form `program:`. Both go
/// to the platform's own shell. A caller embedding this in YAML must
/// single-quote it: a Windows path is full of backslashes, and a double-quoted
/// YAML scalar reads those as escapes.

// §FS-rhei-programs.1.1
pub fn fixture_command_line(script: &std::path::Path) -> String {
    format!("{} {}", python_command(), script.display())
}
