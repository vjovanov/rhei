// The `rhei` binary the two test homes drive as a subprocess. Cargo names a
// package's own binaries to its tests and nobody else's, so a test living
// outside `crates/rhei-cli` finds it beside itself in the profile directory it
// was built into, and builds it on demand when a partial invocation (`cargo
// test -p rhei-e2e-tests` or `-p rhei-integration-tests` on a fresh tree) has
// not produced it yet. `cargo test --workspace --all-targets` always has.
//
// Shared by both homes: one pulls it in with `#[path]` into its module tree,
// the other with `include!` into its flat module — so this comment is `//`
// rather than `//!`, matching `support/test_dir.rs` and
// `support/python_fixture.rs`, since an inner doc comment cannot open an
// included file.
#![allow(dead_code)]

use std::path::PathBuf;
use std::process::Command;

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

pub fn rhei_binary() -> PathBuf {
    let dir = profile_dir();
    let path = dir.join(format!("rhei{}", std::env::consts::EXE_SUFFIX));
    if !path.is_file() {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let mut build = Command::new(cargo);
        build.args(["build", "-p", "rhei-cli", "--locked"]).current_dir(repo_root());
        if dir.file_name().and_then(|name| name.to_str()) == Some("release") {
            build.arg("--release");
        }
        let status =
            build.status().unwrap_or_else(|err| panic!("run cargo build -p rhei-cli: {err}"));
        assert!(status.success(), "cargo build -p rhei-cli failed");
    }
    assert!(path.is_file(), "no rhei binary at {}", path.display());
    path
}
