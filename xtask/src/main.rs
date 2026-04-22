use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

struct Example {
    name: &'static str,
    path: &'static str,
    state_machine: Option<&'static str>,
    runnable: bool,
}

const EXAMPLES: &[Example] = &[
    Example {
        name: "release-automation",
        path: "examples/release-automation.rhei.md",
        state_machine: None,
        runnable: false,
    },
    Example {
        name: "human-review-loop",
        path: "examples/human-review-loop.rhei.md",
        state_machine: None,
        runnable: false,
    },
    Example {
        name: "pm-onboarding-experiment",
        path: "examples/pm-onboarding-experiment.rhei.md",
        state_machine: None,
        runnable: false,
    },
    Example {
        name: "escaped-state-values",
        path: "examples/escaped-state-values.rhei.md",
        state_machine: Some("examples/states-with-spaces.yaml"),
        runnable: false,
    },
    Example {
        name: "claude-code",
        path: "examples/claude-code/plan.rhei.md",
        state_machine: Some("examples/claude-code/states.yaml"),
        runnable: false,
    },
    Example {
        name: "living-review-loop",
        path: "examples/living-review-loop",
        state_machine: Some("examples/living-review-loop/team-states.yaml"),
        runnable: true,
    },
    Example {
        name: "review-fix-visits",
        path: "examples/review-fix-visits",
        state_machine: Some("examples/review-fix-visits/states.yaml"),
        runnable: false,
    },
];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask crate must live in workspace root")
        .to_path_buf()
}

fn usage() {
    eprintln!("Usage: cargo xtask <command> [args]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  examples list                  List all known examples");
    eprintln!("  examples validate <name>       Validate one example");
    eprintln!("  examples validate --all        Validate every example");
    eprintln!("  examples run <name>            Run a runnable example in a tmp copy");
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let slice: Vec<&str> = args.iter().map(String::as_str).collect();
    match slice.as_slice() {
        ["examples", "list"] => {
            cmd_list();
            ExitCode::SUCCESS
        }
        ["examples", "validate", "--all"] => cmd_validate_all(),
        ["examples", "validate", name] => cmd_validate_one(name),
        ["examples", "run", name] => cmd_run(name),
        _ => {
            usage();
            ExitCode::from(2)
        }
    }
}

fn cmd_list() {
    let width = EXAMPLES.iter().map(|e| e.name.len()).max().unwrap_or(0);
    for ex in EXAMPLES {
        let tag = if ex.runnable {
            "validate, run"
        } else {
            "validate"
        };
        println!("  {:<width$}  {}", ex.name, tag, width = width);
    }
}

fn find(name: &str) -> Option<&'static Example> {
    EXAMPLES.iter().find(|e| e.name == name)
}

fn cmd_validate_all() -> ExitCode {
    let mut failed: Vec<&str> = Vec::new();
    for ex in EXAMPLES {
        println!("==> validate {}", ex.name);
        if !run_validate(ex) {
            failed.push(ex.name);
        }
    }
    if failed.is_empty() {
        println!("\nAll {} examples validated.", EXAMPLES.len());
        ExitCode::SUCCESS
    } else {
        eprintln!("\nFailed: {}", failed.join(", "));
        ExitCode::FAILURE
    }
}

fn cmd_validate_one(name: &str) -> ExitCode {
    let Some(ex) = find(name) else {
        eprintln!("unknown example: {name}");
        return ExitCode::from(2);
    };
    if run_validate(ex) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn run_validate(ex: &Example) -> bool {
    let root = workspace_root();
    let mut cmd = Command::new(cargo());
    cmd.current_dir(&root)
        .args(["run", "-q", "-p", "rhei-cli", "--"]);
    if let Some(sm) = ex.state_machine {
        cmd.args(["--state-machine", sm]);
    }
    cmd.args(["validate", ex.path]);
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

fn cmd_run(name: &str) -> ExitCode {
    let Some(ex) = find(name) else {
        eprintln!("unknown example: {name}");
        return ExitCode::from(2);
    };
    if !ex.runnable {
        eprintln!("example '{}' is not runnable (validate only)", ex.name);
        return ExitCode::from(2);
    }
    let root = workspace_root();
    let src = root.join(ex.path);
    if !src.is_dir() {
        eprintln!("expected directory for runnable example: {}", src.display());
        return ExitCode::FAILURE;
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = env::temp_dir().join(format!("rhei-xtask-{}-{stamp}", ex.name));
    let leaf = src.file_name().expect("example path has a file name");
    let dest = tmp.join(leaf);
    if let Err(err) = copy_dir_all(&src, &dest) {
        eprintln!("failed to copy example: {err}");
        return ExitCode::FAILURE;
    }
    let sm = ex
        .state_machine
        .expect("runnable examples must declare a state machine");
    let sm_rel = Path::new(sm)
        .strip_prefix(ex.path)
        .expect("runnable example's state machine must live under its directory");
    let sm_abs = dest.join(sm_rel);
    println!("==> running {} in {}", ex.name, dest.display());
    let status = Command::new(cargo())
        .current_dir(&root)
        .args(["run", "-q", "-p", "rhei-cli", "--"])
        .arg("--state-machine")
        .arg(&sm_abs)
        .arg("run")
        .arg(&dest)
        .status();
    match status {
        Ok(s) if s.success() => {
            println!("\nArtifacts left in {}", dest.display());
            ExitCode::SUCCESS
        }
        _ => ExitCode::FAILURE,
    }
}

fn cargo() -> PathBuf {
    env::var_os("CARGO")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("cargo"))
}

fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}
