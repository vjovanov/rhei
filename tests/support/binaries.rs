// The `rhei` binary the two test homes drive as a subprocess. Cargo names a
// package's own binaries to its tests and nobody else's, so a test living
// outside `crates/rhei-cli` finds it beside itself in the profile directory it
// was built into. Before either harness first uses it, Cargo verifies that the
// binary matches the current checkout and rebuilds it when needed. The result
// is shared for the rest of that harness process.
//
// Shared by both homes: one pulls it in with `#[path]` into its module tree,
// the other with `include!` into its flat module — so this comment is `//`
// rather than `//!`, matching `support/test_dir.rs` and
// `support/python_fixture.rs`, since an inner doc comment cannot open an
// included file.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

#[derive(Debug, PartialEq, Eq)]
enum BinaryVerificationError {
    BuildFailed,
    MissingOutput,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `target/<profile>/`, read off the running test binary's own location.
fn profile_dir() -> PathBuf {
    let exe = std::env::current_exe().expect("current test binary");
    let deps = exe.parent().expect("deps dir");
    assert_eq!(
        deps.file_name().and_then(|name| name.to_str()),
        Some("deps"),
        "unexpected test binary location {}",
        exe.display()
    );
    deps.parent().expect("profile dir").to_path_buf()
}

fn verify_rhei_binary(
    path: &Path,
    build: impl FnOnce() -> bool,
) -> Result<(), BinaryVerificationError> {
    if !build() {
        return Err(BinaryVerificationError::BuildFailed);
    }
    if !path.is_file() {
        return Err(BinaryVerificationError::MissingOutput);
    }
    Ok(())
}

// §AR-ci-release.1
pub fn rhei_binary() -> PathBuf {
    static RHEI_BINARY: OnceLock<PathBuf> = OnceLock::new();
    RHEI_BINARY
        .get_or_init(|| {
            let dir = profile_dir();
            let path = dir.join(format!("rhei{}", std::env::consts::EXE_SUFFIX));
            let result = verify_rhei_binary(&path, || {
                let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
                let mut build = Command::new(cargo);
                build.args(["build", "-p", "rhei-cli", "--locked"]).current_dir(repo_root());
                if dir.file_name().and_then(|name| name.to_str()) == Some("release") {
                    build.arg("--release");
                }
                build
                    .status()
                    .unwrap_or_else(|err| panic!("run cargo build -p rhei-cli: {err}"))
                    .success()
            });
            match result {
                Ok(()) => path,
                Err(BinaryVerificationError::BuildFailed) => {
                    panic!("cargo build -p rhei-cli failed")
                }
                Err(BinaryVerificationError::MissingOutput) => {
                    panic!("no rhei binary at {}", path.display())
                }
            }
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::{verify_rhei_binary, BinaryVerificationError};

    #[test]
    fn existing_binary_still_runs_freshness_build() {
        let existing = std::env::current_exe().expect("current test binary");
        let mut builds = 0;

        let result = verify_rhei_binary(&existing, || {
            builds += 1;
            true
        });

        assert_eq!(result, Ok(()));
        assert_eq!(builds, 1);
    }

    #[test]
    fn failed_freshness_build_rejects_existing_binary() {
        let existing = std::env::current_exe().expect("current test binary");

        let result = verify_rhei_binary(&existing, || false);

        assert_eq!(result, Err(BinaryVerificationError::BuildFailed));
    }
}
