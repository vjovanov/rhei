//! The isolated workspace the detached-run end-to-end tests drive.
//!
//! One `HOME` and one `XDG_STATE_HOME` per test, because the run registry is
//! machine-wide: without that, a test would see the developer's own runs and
//! the developer would see the test's.
// §FS-rhei-run-headless.2

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
#[cfg(unix)]
use std::time::{Duration, Instant};

use super::{fixture_command, unique_temp_dir, write_python_agent, TestDir};

/// A state machine whose only work is a program, so a run does real work with
/// no agent binary in sight. The command is absolute because a test workspace
/// lives wherever the temp directory is, not on any search path, and it is a
/// list rather than a string so the engine execs it instead of handing it to a
/// shell — which is also what keeps a Windows path's backslashes out of the
/// YAML.
pub fn program_machine(script: &Path) -> String {
    format!(
        r#"name: headless-e2e
version: 1
states:
  pending:
    initial: true
    description: Waiting to run
    program:
      command: {}
    program_timeout: 2m
  done:
    final: true
    description: Finished
transitions:
  - from: pending
    to: done
    exit_code: 0
"#,
        fixture_command(script)
    )
}

pub const TWO_TASK_PLAN: &str = r#"# Rhei: Headless Demo

## Tasks

### Task 1: First
**State:** pending

### Task 2: Second
**State:** pending
"#;

/// One isolated workspace: its own plan, machine, program, and `HOME`, so the
/// machine-wide run registry a test writes cannot be seen by any other test or
/// by the developer's own runs. §FS-rhei-run-headless.2
pub struct Workspace {
    pub root: TestDir,
    pub home: PathBuf,
}

impl Workspace {
    /// `work_seconds` sets how long each ticket's program runs, which is how a
    /// test buys a window in which the run is reliably still live.
    pub fn new(prefix: &str, work_seconds: u32) -> Self {
        let root = unique_temp_dir(prefix);
        let home = root.join(".home");
        fs::create_dir_all(&home).expect("isolated home");
        fs::write(root.join("plan.rhei.md"), TWO_TASK_PLAN).expect("plan");
        // The marker line is how a test sees whether the detached-child
        // environment variable leaked into supervised work.
        // §FS-rhei-run-headless.1.2
        let script_path = write_python_agent(
            &root,
            "work.py",
            &format!(
                r#"task = env("RHEI_TASK_ID")
print("starting " + task, flush=True)
print("headless-marker=" + env("RHEI_HEADLESS_CHILD", "unset"), flush=True)
time.sleep({work_seconds})
result("finished " + task + "\n")
print("done " + task, flush=True)
"#
            ),
        );
        fs::write(root.join("states.yaml"), program_machine(&script_path)).expect("machine");
        Self { root, home }
    }

    pub fn plan(&self) -> PathBuf {
        self.root.join("plan.rhei.md")
    }

    pub fn machine(&self) -> PathBuf {
        self.root.join("states.yaml")
    }

    pub fn descriptor(&self) -> Option<serde_json::Value> {
        let raw = fs::read_to_string(self.root.join("runtime/run.json")).ok()?;
        serde_json::from_str(&raw).ok()
    }

    // Only the detached-run cases use it, and those are Unix-only.
    // §FS-rhei-run-headless.1.3
    #[cfg(unix)]
    pub fn plan_text(&self) -> String {
        fs::read_to_string(self.plan()).expect("plan is readable")
    }

    /// A `rhei` command against this workspace's isolated `HOME`, not yet run —
    /// for the cases that need to signal it or watch it while it works.
    pub fn rhei_command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_rhei"));
        cmd.current_dir(&self.root);
        cmd.env("HOME", &self.home);
        // The registry lives under the state directory, so pinning HOME is not
        // enough on a machine that sets XDG_STATE_HOME.
        cmd.env("XDG_STATE_HOME", self.home.join("state"));
        cmd.args(args);
        cmd
    }

    /// Run a `rhei` subcommand against this workspace's isolated `HOME`.
    pub fn rhei(&self, args: &[&str]) -> Output {
        self.rhei_command(args).output().expect("rhei command runs")
    }

    /// The arguments that run this workspace's plan under its own machine.
    pub fn run_args(&self, extra: &[&str]) -> Vec<String> {
        let mut args = vec![
            "run".to_string(),
            "--state-machine".to_string(),
            self.machine().display().to_string(),
            self.plan().display().to_string(),
        ];
        args.extend(extra.iter().map(|arg| (*arg).to_string()));
        args
    }

    /// `rhei run` against this workspace, with the plan and machine named
    /// absolutely — a bare relative plan name has no parent directory, so it
    /// gives the run an empty workspace root to work in.
    pub fn run(&self, extra: &[&str]) -> Output {
        let args = self.run_args(extra);
        self.rhei(&args.iter().map(String::as_str).collect::<Vec<_>>())
    }

    /// Start a run detached and return its id.
    // Only the detached-run cases use it, and those are Unix-only.
    // §FS-rhei-run-headless.1.3
    #[cfg(unix)]
    pub fn launch_headless(&self) -> String {
        let out = self.run(&["--headless"]);
        assert!(
            out.status.success(),
            "headless launch failed:\nstdout: {}\nstderr: {}",
            stdout(&out),
            stderr(&out)
        );
        let text = stdout(&out);
        let id = text
            .lines()
            .find_map(|line| line.strip_prefix("Run ").and_then(|rest| rest.split(' ').next()))
            .unwrap_or_else(|| panic!("no run id in launcher output:\n{text}"))
            .to_string();
        assert_eq!(id.len(), 6, "the launcher prints the run id from {text}");
        id
    }

    /// Stop any live run and wait for it, so a test never leaves a detached
    /// process behind.
    pub fn stop_quietly(&self) {
        let _ = self.rhei(&["stop", "--wait"]);
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        self.stop_quietly();
    }
}

pub fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

pub fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Parse a JSONL stream, failing loudly on the first line that is not a record.
/// This is the assertion that matters for `--json`: not that the records are
/// right, but that *nothing else* shares the stream. §FS-rhei-run-json.1
pub fn parse_records(text: &str) -> Vec<serde_json::Value> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|err| panic!("stdout carried a non-record line ({err}): {line}"))
        })
        .collect()
}

pub fn kinds(records: &[serde_json::Value]) -> Vec<String> {
    records.iter().filter_map(|r| r["event"].as_str().map(str::to_string)).collect()
}

/// Poll `condition` until it holds or the deadline passes.
// Only the detached-run cases use it, and those are Unix-only.
// §FS-rhei-run-headless.1.3
#[cfg(unix)]
pub fn wait_until(what: &str, timeout: Duration, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("timed out waiting for {what}");
}
