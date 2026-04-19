use std::fs;
use std::path::Path;
use std::process::Command;

use super::{unique_temp_dir, CliRun};

/// Run `rhei install-skills` with a fake HOME and optional extra args.
fn run_install_skills(home: &Path, extra_args: &[&str]) -> CliRun {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rhei"));
    cmd.env("HOME", home);
    cmd.arg("install-skills");
    for arg in extra_args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("rhei command should run");
    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Run `rhei install-skills` from a specific working directory (for --local).
fn run_install_skills_in_dir(home: &Path, cwd: &Path, extra_args: &[&str]) -> CliRun {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rhei"));
    cmd.env("HOME", home);
    cmd.current_dir(cwd);
    cmd.arg("install-skills");
    for arg in extra_args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("rhei command should run");
    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

#[test]
fn global_install_copy_claude_code() {
    let home = unique_temp_dir("install-claude-code");

    let result = run_install_skills(&home, &["--agent", "claude-code"]);
    assert!(
        result.status.success(),
        "install should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // Verify skill directories were copied.
    assert!(home.join(".claude/skills/rhei-plan-writer/SKILL.md").exists());
    assert!(home.join(".claude/skills/rhei-plan-worker/SKILL.md").exists());
    assert!(home.join(".claude/skills/rhei-state-machine-writer/SKILL.md").exists());

    // Verify CLAUDE.md has registration block.
    let claude_md = fs::read_to_string(home.join(".claude/CLAUDE.md")).expect("read CLAUDE.md");
    assert!(claude_md.contains("# rhei"));
    assert!(claude_md.contains("rhei-plan-writer"));
    assert!(claude_md.contains("rhei-plan-worker"));
    assert!(claude_md.contains("rhei-state-machine-writer"));

    // Verify output format.
    assert!(result.stdout.contains("claude-code:"));
    assert!(result.stdout.contains("✓"));
    assert!(result.stdout.contains("registered 3 skills"));
    assert!(result.stdout.contains("Installed rhei skills for 1 agent."));
}

#[test]
fn local_install_cursor() {
    let home = unique_temp_dir("install-cursor-local");
    let project = unique_temp_dir("install-cursor-project");
    // Create a project marker so find_project_root works.
    fs::write(project.join("Cargo.toml"), "[package]\nname = \"test\"").expect("write marker");

    let result = run_install_skills_in_dir(&home, &project, &["--local", "--agent", "cursor"]);
    assert!(
        result.status.success(),
        "install should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // Verify .mdc files were created in the project.
    assert!(project.join(".cursor/rules/rhei-plan-writer.mdc").exists());
    assert!(project.join(".cursor/rules/rhei-plan-worker.mdc").exists());
    assert!(project.join(".cursor/rules/rhei-state-machine-writer.mdc").exists());

    // Verify MDC format.
    let mdc =
        fs::read_to_string(project.join(".cursor/rules/rhei-plan-writer.mdc")).expect("read mdc");
    assert!(mdc.starts_with("---\n"));
    assert!(mdc.contains("alwaysApply: false"));
}

#[test]
fn link_mode_creates_symlinks() {
    let home = unique_temp_dir("install-link");

    let result = run_install_skills(&home, &["--agent", "kilocode", "--link"]);
    assert!(
        result.status.success(),
        "install should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let skill_path = home.join(".kilocode/rules/rhei-plan-writer.md");
    assert!(skill_path.exists(), "skill file should exist");
    assert!(skill_path.symlink_metadata().unwrap().file_type().is_symlink(), "should be a symlink");
}

#[test]
fn uninstall_removes_files() {
    let home = unique_temp_dir("install-uninstall");

    // First install.
    let result = run_install_skills(&home, &["--agent", "claude-code"]);
    assert!(result.status.success());
    assert!(home.join(".claude/skills/rhei-plan-writer/SKILL.md").exists());

    // Then uninstall.
    let result = run_install_skills(&home, &["--agent", "claude-code", "--uninstall"]);
    assert!(
        result.status.success(),
        "uninstall should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // Skill directories should be removed.
    assert!(!home.join(".claude/skills/rhei-plan-writer").exists());
    assert!(!home.join(".claude/skills/rhei-plan-worker").exists());
    assert!(!home.join(".claude/skills/rhei-state-machine-writer").exists());

    assert!(result.stdout.contains("Uninstalled"));
}

#[test]
fn dry_run_does_not_create_files() {
    let home = unique_temp_dir("install-dryrun");

    let result = run_install_skills(&home, &["--agent", "claude-code", "--dry-run"]);
    assert!(
        result.status.success(),
        "dry-run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // Output should mention dry-run.
    assert!(result.stdout.contains("[dry-run]"));

    // No files should have been created (except the CLAUDE.md registration output line).
    assert!(!home.join(".claude/skills/rhei-plan-writer").exists());
}

#[test]
fn idempotent_install_shows_already_installed() {
    let home = unique_temp_dir("install-idempotent");

    // First install.
    let result = run_install_skills(&home, &["--agent", "claude-code"]);
    assert!(result.status.success());

    // Second install should detect already installed.
    let result = run_install_skills(&home, &["--agent", "claude-code"]);
    assert!(result.status.success());
    assert!(
        result.stdout.contains("already installed"),
        "second install should print 'already installed'\nstdout:\n{}",
        result.stdout
    );
}
