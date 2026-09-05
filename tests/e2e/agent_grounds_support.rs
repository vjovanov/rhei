//! Shared fixtures for the two homes of rhei's project-local material:
//! `.agent-grounds/rhei/` and the deprecated `.agents/rhei/`.
//! §FS-rhei-templates.1

use std::path::Path;

use super::*;

/// The new home, and the one rhei writes. §FS-rhei-templates.1.1
pub const GROUNDS: &str = ".agent-grounds/rhei";
/// The deprecated home, still read and warned about. §FS-rhei-templates.1.1
pub const DEPRECATED: &str = ".agents/rhei";

pub fn run_in(args: &[&str], cwd: &Path, home: &Path) -> CliRun {
    let output =
        rhei_command(home).current_dir(cwd).args(args).output().expect("rhei command should run");
    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Separators differ per platform and macOS resolves `/tmp` through a symlink,
/// so match the tail the child process cannot rewrite: the unique directory
/// name plus the relative path under it.
pub fn assert_names_path(stderr: &str, dir: &Path, relative: &str) {
    let leaf = dir.file_name().expect("test directory has a name").to_string_lossy().into_owned();
    let tail = format!("{leaf}/{relative}");
    let seen = stderr.replace('\\', "/");
    assert!(seen.contains(&tail), "warning should name '{tail}'; stderr was:\n{seen}");
}

/// The warning names the path it read and the path to move the material to, on
/// stderr, so a reader is not left hunting and `--json` stays parseable.
/// §FS-rhei-templates.1.3
pub fn assert_deprecation_warning(result: &CliRun, dir: &Path, read: &str, move_to: &str) {
    assert!(
        result.stderr.to_lowercase().contains("deprecated"),
        "the fallback must be reported as deprecated; stderr was:\n{}",
        result.stderr
    );
    assert_names_path(&result.stderr, dir, read);
    assert_names_path(&result.stderr, dir, move_to);
    assert!(
        !result.stdout.to_lowercase().contains("deprecated"),
        "the warning belongs on stderr, or it corrupts --json consumers; stdout was:\n{}",
        result.stdout
    );
}

/// Nothing was read from the deprecated home, so nothing may be warned about.
/// §FS-rhei-templates.1.3
pub fn assert_silent_about_the_deprecated_home(result: &CliRun) {
    assert!(
        !result.stderr.contains(".agents/rhei"),
        "rhei must be silent when the deprecated home is not the path read; stderr was:\n{}",
        result.stderr
    );
}
