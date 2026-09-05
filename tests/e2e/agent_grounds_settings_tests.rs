//! Project settings resolve from `.agent-grounds/rhei/settings.json` first and
//! from the deprecated `.agents/rhei/settings.json` second, and everything rhei
//! writes lands in the new home. §FS-rhei-agents.1.1
//!
//! The observable is `rhei validate`: it composes the merged settings and then
//! resolves every agent a state machine names, so a machine naming an agent
//! that only one settings file declares says which file was read.
//!
//! The `.agents/` cases here are permanent coverage of the deprecated path.

use std::fs;
use std::path::{Path, PathBuf};

use super::agent_grounds_support::{
    assert_deprecation_warning, assert_silent_about_the_deprecated_home, run_in, DEPRECATED,
    GROUNDS,
};
use super::*;

fn write_plan(dir: &Path) -> PathBuf {
    write_fixture_file(
        dir,
        "plan.rhei.md",
        r#"# Rhei: Settings home fixture

## Tasks

### Task 1: Work
**State:** work
"#,
    )
}

/// A machine whose only outside reference is one agent id, so validation
/// passes exactly when the settings file declaring that id was the one read.
fn write_machine(dir: &Path, file_name: &str, agent: &str) -> PathBuf {
    write_fixture_file(
        dir,
        file_name,
        &format!(
            r#"name: settings-home
version: 1
states:
  work:
    initial: true
    agent: {agent}
    description: Do the work
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#
        ),
    )
}

fn write_settings(dir: &Path, home: &str, agent: &str) {
    let settings_dir = dir.join(home);
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    write_fixture_file(
        &settings_dir,
        "settings.json",
        &format!(
            r#"{{
  "agents": {{
    "{agent}": {{ "command": ["{agent}"], "modes": {{ "yolo": [] }} }}
  }}
}}
"#
        ),
    );
}

fn assert_unknown_agent(result: &CliRun, agent: &str) {
    assert!(
        !result.status.success(),
        "the agent must not resolve; stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let output = format!("{}{}", result.stdout, result.stderr);
    assert!(
        output.contains("unknown agent") && output.contains(agent),
        "expected an unknown-agent error naming '{agent}'; output was:\n{output}"
    );
}

/// §FS-rhei-agents.1.1: the new home is the project settings file, and reading
/// it is silent. §FS-rhei-templates.1.3
#[test]
fn project_settings_resolve_from_agent_grounds() {
    let dir = unique_temp_dir("grounds-settings-project");
    let plan = write_plan(&dir);
    let machine = write_machine(&dir, "states.yaml", "ground-agent");
    write_settings(&dir, GROUNDS, "ground-agent");

    let result = run_cli("validate", &plan, &machine, &[]);
    assert_success(&result);
    assert_silent_about_the_deprecated_home(&result);
}

/// §FS-rhei-agents.1.1: the deprecated home is still read, and warns, naming
/// both paths. §FS-rhei-templates.1.3
#[test]
fn project_settings_fall_back_to_the_deprecated_home_with_a_warning() {
    let dir = unique_temp_dir("grounds-settings-fallback");
    let plan = write_plan(&dir);
    let machine = write_machine(&dir, "states.yaml", "legacy-agent");
    write_settings(&dir, DEPRECATED, "legacy-agent");

    let result = run_cli("validate", &plan, &machine, &[]);
    assert_success(&result);
    assert_deprecation_warning(
        &result,
        &dir,
        &format!("{DEPRECATED}/settings.json"),
        &format!("{GROUNDS}/settings.json"),
    );
}

/// §FS-rhei-agents.1.1: first match wins and the two project files are never
/// merged. Merging would invent a precedence tier between global and project
/// that nothing else has, and would leave the deprecation with no end.
#[test]
fn project_settings_files_are_never_merged() {
    let dir = unique_temp_dir("grounds-settings-no-merge");
    let plan = write_plan(&dir);
    write_settings(&dir, GROUNDS, "ground-agent");
    write_settings(&dir, DEPRECATED, "legacy-agent");

    let chosen = write_machine(&dir, "chosen.yaml", "ground-agent");
    let chosen_result = run_cli("validate", &plan, &chosen, &[]);
    assert_success(&chosen_result);
    assert_silent_about_the_deprecated_home(&chosen_result);

    let shadowed = write_machine(&dir, "shadowed.yaml", "legacy-agent");
    assert_unknown_agent(&run_cli("validate", &plan, &shadowed, &[]), "legacy-agent");
}

/// §FS-rhei-templates.1.1: rhei never writes to the deprecated home, so a
/// template's root `settings.json` lands in the new one on instantiation.
#[test]
fn instantiation_writes_root_settings_into_agent_grounds() {
    let dir = unique_temp_dir("grounds-instantiate-settings");
    let template_dir = dir.join("settings-template");
    fs::create_dir_all(&template_dir).expect("create template directory");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        "name: settings-template\nversion: 1.0.0\ndescription: Bundles project settings\ninputs: []\n",
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Bundled settings

## Tasks

### Task 1: Work
**State:** pending
"#,
    );
    write_fixture_file(
        &template_dir,
        "settings.json",
        r#"{
  "agents": {
    "bundled-agent": { "command": ["bundled-agent"], "modes": { "yolo": [] } }
  }
}
"#,
    );

    let output_dir = dir.join("output");
    let result = run_in(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &dir,
        &dir.join("home"),
    );
    assert_success(&result);

    let written = fs::read_to_string(output_dir.join(GROUNDS).join("settings.json"))
        .expect("bundled settings should be written to the new home");
    assert!(
        written.contains("bundled-agent"),
        "the template's registry should land here:\n{written}"
    );
    assert!(
        !output_dir.join(DEPRECATED).join("settings.json").exists(),
        "rhei's own output must never be a path rhei then warns about"
    );
}

/// §FS-rhei-templates.1.1: the settings hoist writes the new home too. The
/// template deliberately sits in the deprecated home, so discovery keeps
/// exercising the fallback and the hoist target is the only thing under test.
#[test]
fn the_settings_hoist_targets_agent_grounds() {
    let dir = unique_temp_dir("grounds-hoist-settings");
    let template_dir = dir.join(DEPRECATED).join("templates/audit");
    fs::create_dir_all(template_dir.join("tasks")).expect("create template directory");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        "name: audit\nversion: 1.0.0\ndescription: Bundles project settings\ninputs: []\n",
    );
    write_fixture_file(
        &template_dir,
        "states.yaml",
        r#"name: audit
version: 1
states:
  review:
    initial: true
    description: Look at it
  done:
    final: true
    description: Finished
transitions:
  - from: review
    to: done
"#,
    );
    write_fixture_file(&template_dir, "index.rhei.md", "# Rhei: audit\n**States:** audit\n");
    write_fixture_file(
        &template_dir,
        "tasks/01-review.md",
        "### Task 1: Review\n**State:** review\n",
    );
    write_fixture_file(
        &template_dir,
        "settings.json",
        r#"{
  "agents": {
    "hoisted-agent": { "command": ["hoisted-agent"], "modes": { "yolo": [] } }
  }
}
"#,
    );

    fs::create_dir_all(dir.join(".git")).expect("mark the repository root");
    let home = dir.join("home");
    assert_success(&run_in(&["init", "--here"], &dir, &home));
    assert_success(&run_in(&["instantiate", "audit"], &dir, &home));

    let hoisted = fs::read_to_string(dir.join(GROUNDS).join("settings.json"))
        .expect("hoisted settings should land in the new home");
    assert!(
        hoisted.contains("hoisted-agent"),
        "the template's registry should land here:\n{hoisted}"
    );
    assert!(
        !dir.join(DEPRECATED).join("settings.json").exists(),
        "rhei's own output must never be a path rhei then warns about"
    );
}
