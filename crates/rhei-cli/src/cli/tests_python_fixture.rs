// §REQ-cross-platform.4

// The interpreter the unit tests' fake agents, programs, and redactors run
// under, and the one place that writes one.
//
// A fixture standing in for an agent has nothing platform-specific about it —
// it prints, reads stdin, writes a file, and exits with a code — so it is
// written once, in Python, and named in the profile's `command` as
// `[python, script]`. No shebang, no `chmod`, and nothing for `cmd.exe` to
// fail to understand.

/// `python3`, or `python` where that is the only name.
///
/// Probed once: a probe per fixture would spend a process per test. `python`
/// comes first on Windows, where `python3` is often the Microsoft Store's
/// launcher stub rather than an interpreter.
fn test_python_command() -> &'static str {
    static PYTHON: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    PYTHON.get_or_init(|| {
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

/// Write `body` as `dir/name.py` and return the `command` that runs it.
///
/// Quote the strings inside `body` with `'…'`: a fixture writes markdown, so a
/// `"` followed by `#` is common, and that sequence closes the Rust raw literal
/// the body is written in.
fn python_fixture_command(dir: &Path, name: &str, body: &str) -> Vec<String> {
    let script = dir.join(format!("{name}.py"));
    fs::write(&script, body).expect("write python fixture");
    vec![test_python_command().to_string(), script.display().to_string()]
}

/// The interpreter's own absolute path.
///
/// A bare name needs `PATH`, and the one place that runs a fixture without one
/// is the snapshot redactor: it is spawned with `env_clear()` and a handful of
/// `RHEI_*` variables, so a `.cmd` shim naming `python` finds nothing.
fn test_python_executable() -> &'static str {
    static PYTHON_PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    PYTHON_PATH.get_or_init(|| {
        let output = std::process::Command::new(test_python_command())
            .arg("-c")
            .arg("import sys;sys.stdout.write(sys.executable)")
            .output()
            .expect("python reports its own path");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    })
}
