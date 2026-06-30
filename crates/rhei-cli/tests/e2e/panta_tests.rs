use std::fs;
use std::process::Command;

use super::*;

fn run_in(args: &[&str], cwd: &std::path::Path) -> CliRun {
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("rhei command should run");
    CliRun {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

#[test]
fn panta_add_appends_recipe_entries() {
    let dir = unique_temp_dir("panta-add");

    // A template source (directory name must match the manifest `name`).
    let tpl = dir.join("greeter");
    fs::create_dir_all(&tpl).expect("create template dir");
    write_fixture_file(
        &tpl,
        "template.yaml",
        "name: greeter\nversion: 1.0.0\ndescription: Greeting template\n",
    );
    write_fixture_file(
        &tpl,
        "plan.rhei.md",
        "# Rhei: Greet\n\n## Tasks\n\n### Task 1: Greet\n**State:** pending\n",
    );
    let tpl_arg = tpl.to_str().expect("template path");

    // A Panta project.
    let proj = dir.join("project");
    fs::create_dir_all(&proj).expect("create project dir");
    fs::write(proj.join("index.panta.md"), "# Panta: Test\n**States:** rhei\n")
        .expect("write manifest");

    // Add a first rhei with an input.
    let auth =
        run_in(&["panta", "add", "auth", "--template", tpl_arg, "--set", "spec=docs/a.md"], &proj);
    assert_success(&auth);

    // Add a second rhei that depends on the first.
    let billing =
        run_in(&["panta", "add", "billing", "--template", tpl_arg, "--depends-on", "auth"], &proj);
    assert_success(&billing);

    let manifest = fs::read_to_string(proj.join("index.panta.md")).expect("read manifest");
    assert!(manifest.starts_with("# Panta: Test"), "header preserved:\n{manifest}");
    assert!(manifest.contains("rheis:"), "recipe present:\n{manifest}");
    assert!(manifest.contains("id: auth"), "auth entry:\n{manifest}");
    assert!(manifest.contains("spec: docs/a.md"), "auth input:\n{manifest}");
    assert!(manifest.contains("id: billing"), "billing entry:\n{manifest}");
    assert!(manifest.contains("depends-on:"), "dependency recorded:\n{manifest}");

    // A duplicate id is rejected.
    let dup = run_in(&["panta", "add", "auth", "--template", tpl_arg], &proj);
    assert!(!dup.status.success(), "duplicate id should fail:\n{}", dup.stderr);

    // An unknown --depends-on target is rejected.
    let unknown =
        run_in(&["panta", "add", "x", "--template", tpl_arg, "--depends-on", "nope"], &proj);
    assert!(!unknown.status.success(), "unknown dep should fail:\n{}", unknown.stderr);

    // A reserved id is rejected.
    let reserved = run_in(&["panta", "add", "basin", "--template", tpl_arg], &proj);
    assert!(!reserved.status.success(), "reserved id should fail:\n{}", reserved.stderr);

    fs::remove_dir_all(dir).expect("cleanup");
}

fn write_greeter_template(dir: &std::path::Path) -> PathBuf {
    let tpl = dir.join("greeter");
    fs::create_dir_all(&tpl).expect("create template dir");
    write_fixture_file(
        &tpl,
        "template.yaml",
        "name: greeter\nversion: 1.0.0\ndescription: Greeting template\ninputs:\n  - name: who\n    description: target\n",
    );
    write_fixture_file(
        &tpl,
        "plan.rhei.md",
        "# Rhei: Greet {{who}}\n\n## Tasks\n\n### Task 1: Greet {{who}}\n**State:** pending\n",
    );
    tpl
}

#[test]
fn panta_run_dry_run_instantiates_in_dependency_order() {
    let dir = unique_temp_dir("panta-run");
    let tpl = write_greeter_template(&dir);
    let tpl_arg = tpl.to_str().expect("template path");

    let proj = dir.join("project");
    fs::create_dir_all(&proj).expect("create project dir");
    fs::write(proj.join("index.panta.md"), "# Panta: Run\n**States:** rhei\n")
        .expect("write manifest");

    assert_success(&run_in(
        &["panta", "add", "auth", "--template", tpl_arg, "--set", "who=Auth"],
        &proj,
    ));
    assert_success(&run_in(
        &[
            "panta",
            "add",
            "billing",
            "--template",
            tpl_arg,
            "--set",
            "who=Billing",
            "--depends-on",
            "auth",
        ],
        &proj,
    ));

    let run = run_in(&["panta", "run", "--dry-run"], &proj);
    assert_success(&run);

    // Each rhei is instantiated into runtime/panta-1/<id>/ with inputs rendered.
    let auth_plan = proj.join("runtime/panta-1/auth/plan.rhei.md");
    let billing_plan = proj.join("runtime/panta-1/billing/plan.rhei.md");
    assert!(auth_plan.exists(), "auth instance:\n{}", run.stdout);
    assert!(billing_plan.exists(), "billing instance:\n{}", run.stdout);
    assert!(fs::read_to_string(&auth_plan).expect("read auth").contains("Greet Auth"));
    assert!(fs::read_to_string(&billing_plan).expect("read billing").contains("Greet Billing"));

    // The dependency is honored: auth is reported before billing.
    let auth_at = run.stdout.find("rhei 'auth'").expect("auth reported");
    let billing_at = run.stdout.find("rhei 'billing'").expect("billing reported");
    assert!(auth_at < billing_at, "auth should run before billing:\n{}", run.stdout);

    // Re-running allocates a fresh run directory.
    assert_success(&run_in(&["panta", "run", "--dry-run"], &proj));
    assert!(proj.join("runtime/panta-2").exists(), "second run allocates panta-2");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn rhei_run_rejects_a_panta_project() {
    let dir = unique_temp_dir("panta-reject");
    let proj = dir.join("project");
    fs::create_dir_all(&proj).expect("create project dir");
    fs::write(proj.join("index.panta.md"), "# Panta: Reject\n**States:** rhei\n")
        .expect("write manifest");

    let result = run_in(&["run", proj.to_str().expect("path")], &dir);
    assert!(!result.status.success(), "rhei run on a panta project should fail");
    assert!(
        result.stderr.contains("panta run"),
        "error should point at `rhei panta run`; got:\n{}",
        result.stderr
    );

    fs::remove_dir_all(dir).expect("cleanup");
}
