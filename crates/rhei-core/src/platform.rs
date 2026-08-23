//! What the three supported platforms disagree about, in one place.
//!
//! A command line goes to a different shell on each, and a canonical path is
//! spelled differently on one of them. Both are facts about the host rather
//! than about plans, callbacks, or programs, and both were getting decided at
//! the point of use — which is how one of them stayed Unix-only and the other
//! leaked a spelling nothing else in the run used.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The platform's own shell, holding one command line.
///
/// A string-form command runs under `/bin/sh -c` on Unix and `cmd /c` on
/// Windows, and a `cli:` callback is the same kind of value — a command line,
/// not an argument vector — so it takes the same shell. Only the Unix half was
/// ever spawned, so on Windows every string-form program and every `cli:`
/// callback died looking for a program named `sh` before its first own
/// instruction.

// §FS-rhei-programs.1.1 §REQ-cross-platform.2
pub fn system_shell_command(command: &str) -> Command {
    #[cfg(windows)]
    {
        // `raw_arg`, not `arg`: `cmd /C` takes a command *line*, and the
        // escaping `arg` applies quotes the whole thing and rewrites every `"`
        // inside it as `\"` — which `cmd` does not understand, so the quotes
        // reached the program as characters and `python -c "…"` was handed a
        // string literal that never closed.
        use std::os::windows::process::CommandExt as _;
        let mut cmd = Command::new("cmd");
        cmd.arg("/C");
        cmd.raw_arg(command);
        cmd
    }
    #[cfg(not(windows))]
    {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command);
        cmd
    }
}

/// `path` without the `\\?\` verbatim prefix Windows canonicalization adds.
///
/// The verbatim form is a second spelling of one location, and it does not stay
/// inside the process that made it: `cmd.exe` refuses to start in one and
/// silently uses the Windows directory instead, a report prints it where every
/// other line prints the plain form, and a worker handed one writes its
/// artifacts where the engine does not look for them.

// §REQ-cross-platform.5
pub fn plain_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(text) = path.to_str() {
            if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
                return PathBuf::from(format!(r"\\{rest}"));
            }
            if let Some(rest) = text.strip_prefix(r"\\?\") {
                // A drive path only; `\\?\Volume{…}` has no plain spelling.
                if rest.as_bytes().get(1) == Some(&b':') {
                    return PathBuf::from(rest);
                }
            }
        }
    }
    path
}

/// [`Path::canonicalize`] with the verbatim prefix taken back off.
// §REQ-cross-platform.5
pub fn canonical_path(path: &Path) -> std::io::Result<PathBuf> {
    path.canonicalize().map(plain_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shell is chosen by platform, and both halves are pinned here — the
    /// Windows one included, because Linux runs the whole suite and Windows
    /// only exercises the branch it happens to take. §FS-rhei-programs.1.1
    #[test]
    fn a_string_command_goes_to_the_platforms_own_shell() {
        let cmd = system_shell_command("echo hi");
        let program = cmd.get_program().to_string_lossy().into_owned();
        let args: Vec<String> =
            cmd.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect();
        let switch = if cfg!(windows) { "/C" } else { "-c" };
        let shell = if cfg!(windows) { "cmd" } else { "sh" };
        assert_eq!(program, shell);
        assert_eq!(args, vec![switch.to_string(), "echo hi".to_string()]);
    }

    /// The verbatim prefix is stripped only where a plain spelling exists, and
    /// both halves are pinned on the platform that runs the whole suite.
    // §REQ-cross-platform.5
    #[test]
    fn a_canonical_windows_path_loses_its_verbatim_prefix() {
        let drive = plain_path(PathBuf::from(r"\\?\C:\work\plan.rhei.md"));
        let unc = plain_path(PathBuf::from(r"\\?\UNC\server\share\plan.rhei.md"));
        let volume = plain_path(PathBuf::from(r"\\?\Volume{2eca078d}\plan.rhei.md"));
        if cfg!(windows) {
            assert_eq!(drive, PathBuf::from(r"C:\work\plan.rhei.md"));
            assert_eq!(unc, PathBuf::from(r"\\server\share\plan.rhei.md"));
            // A volume GUID path has no plain form, so it is left alone.
            assert_eq!(volume, PathBuf::from(r"\\?\Volume{2eca078d}\plan.rhei.md"));
        } else {
            // Nothing to strip: these are ordinary relative names elsewhere.
            assert_eq!(drive, PathBuf::from(r"\\?\C:\work\plan.rhei.md"));
            assert_eq!(unc, PathBuf::from(r"\\?\UNC\server\share\plan.rhei.md"));
            assert_eq!(volume, PathBuf::from(r"\\?\Volume{2eca078d}\plan.rhei.md"));
        }
    }
}
