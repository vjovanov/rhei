//! Collect [`VizModel`]s from a plan file or a Directory Workspace, resolving
//! each plan's machine (built-in default, sibling `states.yaml`, or override).
//! The static-path collection shared by `rhei viz` and the `xtask` dogfood. §FS-rhei-viz.7.2

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rhei_core::ast::Rhei;
use rhei_core::{parse, workspace};
use rhei_validator::StateMachine;
use rhei_viz_model::VizModel;

use crate::build;

/// A keyed bundle of plans in the shape `rhei-viz-model::render_static` inlines
/// and the asset's plan selector reads.
pub type Bundle = BTreeMap<String, VizModel>;

/// Collect every plan reachable from `path` into a render-ready bundle.
///
/// - a file → one plan, keyed by `key`;
/// - a Directory Workspace → the merged `index.rhei.md` (keyed by `key`) plus
///   each standalone `*.rhei.md` (keyed `key::<relative-path>`).
///
/// `machine_override`, when set, resolves every plan against that YAML file;
/// otherwise each plan resolves the built-in default (`**States:** rhei`) or a
/// sibling `states.yaml`.
pub fn collect_plans(
    path: &Path,
    key: &str,
    machine_override: Option<&Path>,
) -> io::Result<Bundle> {
    let mut plans = Bundle::new();

    if path.is_file() {
        let model = load_plan_file(path, machine_override)?;
        plans.insert(key.to_string(), model);
        return Ok(plans);
    }
    if !path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no such file or directory: {}", path.display()),
        ));
    }

    if workspace::is_workspace(path) {
        let loaded = workspace::load_workspace(path).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to load workspace {}: {}", path.display(), err.message),
            )
        })?;
        let machine = resolve_machine(path, machine_override, &loaded.rhei)?;
        plans.insert(key.to_string(), build(&loaded.rhei, &machine));
    }

    for plan_path in standalone_plan_files(path)? {
        if plan_path.file_name().and_then(|name| name.to_str()) == Some("index.rhei.md") {
            continue;
        }
        let rel = plan_path.strip_prefix(path).unwrap_or(&plan_path).to_string_lossy().to_string();
        let plan_key = format!("{key}::{rel}");
        plans.insert(plan_key, load_plan_file(&plan_path, machine_override)?);
    }

    Ok(plans)
}

fn load_plan_file(path: &Path, machine_override: Option<&Path>) -> io::Result<VizModel> {
    let text = fs::read_to_string(path)?;
    let rhei = parse(&text).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse {}: {}", path.display(), err.message),
        )
    })?;
    let machine = resolve_machine(path, machine_override, &rhei)?;
    Ok(build(&rhei, &machine))
}

/// Resolve the state machine for a plan: an explicit `--states` override wins,
/// then a matching sibling `states.yaml` next to the plan (or workspace root),
/// then the built-in default for `**States:** rhei`.
fn resolve_machine(
    plan_or_dir: &Path,
    machine_override: Option<&Path>,
    rhei: &Rhei,
) -> io::Result<StateMachine> {
    if let Some(machine_path) = machine_override {
        return load_machine(machine_path);
    }
    let dir = if plan_or_dir.is_dir() {
        plan_or_dir.to_path_buf()
    } else {
        plan_or_dir.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
    };
    let candidate = dir.join("states.yaml");
    if candidate.is_file() {
        // Static viz mirrors CLI state-machine discovery: a matching local file
        // overrides the built-in `rhei` machine. §FS-rhei-viz.8
        let machine = load_machine(&candidate)?;
        if machine.name == rhei.states {
            return Ok(machine);
        }
        let builtin = StateMachine::builtin_default();
        if rhei.states != builtin.name {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "plan declares state machine '{}', but auto-discovered states file '{}' declares '{}'",
                    rhei.states,
                    candidate.display(),
                    machine.name
                ),
            ));
        }
        return Ok(builtin);
    }
    if rhei.states == StateMachine::builtin_default().name {
        return Ok(StateMachine::builtin_default());
    }
    load_machine(&candidate)
}

fn load_machine(machine_path: &Path) -> io::Result<StateMachine> {
    StateMachine::from_yaml_file(machine_path).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to load state machine {}: {err}", machine_path.display()),
        )
    })
}

fn standalone_plan_files(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.ends_with(".rhei.md"))
                .unwrap_or(false)
        })
        .collect();
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
            let path = std::env::temp_dir().join(format!("rhei-viz-collect-{prefix}-{stamp}"));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
        fn path(&self) -> &Path {
            &self.path
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn merges_workspace_and_standalone_plans() {
        let temp = TempDir::new("ws");
        fs::write(
            temp.path().join("index.rhei.md"),
            "# Rhei: Workspace\n**States:** rhei\n\n## Overview\nDemo.\n",
        )
        .unwrap();
        fs::create_dir_all(temp.path().join("tasks")).unwrap();
        fs::write(
            temp.path().join("tasks/alpha.rhei.md"),
            "### Task 1: Alpha\n**State:** pending\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("extra.rhei.md"),
            "# Rhei: Extra\n**States:** rhei\n\n## Tasks\n\n### Task 1: Extra\n**State:** completed\n",
        )
        .unwrap();

        let plans = collect_plans(temp.path(), "demo", None).expect("collect");
        assert_eq!(plans.len(), 2);
        assert_eq!(plans["demo"].plan_title.as_deref(), Some("Workspace"));
        assert_eq!(plans["demo::extra.rhei.md"].plan_title.as_deref(), Some("Extra"));
    }

    #[test]
    fn single_file_resolves_builtin_default() {
        let temp = TempDir::new("file");
        let plan = temp.path().join("plan.rhei.md");
        fs::write(
            &plan,
            "# Rhei: Solo\n**States:** rhei\n\n## Tasks\n\n### Task 1: A\n**State:** in-progress\n",
        )
        .unwrap();
        let plans = collect_plans(&plan, "solo", None).expect("collect");
        assert_eq!(plans.len(), 1);
        assert_eq!(plans["solo"].plan_state.as_deref(), Some("active"));
        assert!(!plans["solo"].machine.states.is_empty());
    }

    #[test]
    fn sibling_states_named_rhei_overrides_builtin_default() {
        let temp = TempDir::new("local-rhei");
        let plan = temp.path().join("plan.rhei.md");
        fs::write(
            &plan,
            "# Rhei: Local\n**States:** rhei\n\n## Tasks\n\n### Task 1: A\n**State:** local-work\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("states.yaml"),
            r#"
name: rhei
version: 1.0
states:
  local-work:
    initial: true
    instructions: "Use the local machine."
  completed:
    final: true
"#,
        )
        .unwrap();

        let plans = collect_plans(&plan, "local", None).expect("collect");
        let local = plans["local"]
            .machine
            .states
            .iter()
            .find(|state| state.name == "local-work")
            .expect("local state is rendered");
        assert_eq!(local.instructions.as_deref(), Some("Use the local machine."));
    }
}
