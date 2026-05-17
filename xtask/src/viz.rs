//! Generate an HTML visualization for a rhei plan (or workspace of plans).
//!
//! Dogfoods the `rhei viz` command specified in
//! [`docs/functional-spec/rhei-viz.spec.md`](../../../docs/functional-spec/rhei-viz.spec.md)
//! before the real subcommand ships. Keep the data shape and derivation
//! rules consistent with that spec so this implementation migrates cleanly.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rhei_core::ast::{Rhei, Task as AstTask};
use rhei_core::{parse, workspace};
use rhei_validator::{parse_task_state, StateMachine};
use serde::Serialize;

const TEMPLATE: &str = include_str!("../assets/viz-template.html");
const DATA_PLACEHOLDER: &str = "/*__DATA__*/null";

#[derive(Debug, Serialize)]
pub struct Plan {
    #[serde(skip_serializing)]
    pub key: String,
    pub title: String,
    pub source: PathBuf,
    pub state: String,
    pub tasks: Vec<Task>,
}

#[derive(Debug, Serialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub state: String,
    pub prior: Vec<String>,
    pub subtasks: Vec<Subtask>,
}

#[derive(Debug, Serialize)]
pub struct Subtask {
    pub id: String,
    pub title: String,
    pub state: String,
    pub prior: Vec<String>,
}

pub fn render_html(plans: &[Plan]) -> String {
    let bundle: BTreeMap<&str, &Plan> =
        plans.iter().map(|plan| (plan.key.as_str(), plan)).collect();
    let data = serde_json::to_string(&bundle).expect("plan bundle should always serialize");
    TEMPLATE.replace(DATA_PLACEHOLDER, &escape_json_for_html_script(&data))
}

pub fn collect_plans(
    path: &Path,
    example_name: &str,
    machine_override: Option<&Path>,
) -> io::Result<Vec<Plan>> {
    if path.is_file() {
        return Ok(vec![load_plan_file(path, example_name.to_string(), machine_override)?]);
    }
    if !path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no such file or directory: {}", path.display()),
        ));
    }

    let mut plans = Vec::new();

    if workspace::is_workspace(path) {
        let loaded = workspace::load_workspace(path).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to load workspace {}: {}", path.display(), err.message),
            )
        })?;
        let machine = resolve_machine_for_workspace(path, machine_override, &loaded.rhei)?;
        plans.push(plan_from_rhei(
            path.join("index.rhei.md"),
            example_name.to_string(),
            &loaded.rhei,
            &machine,
        ));
    }

    for plan_path in standalone_plan_files(path)? {
        if plan_path.file_name().and_then(|name| name.to_str()) == Some("index.rhei.md") {
            continue;
        }
        let rel = plan_path.strip_prefix(path).unwrap_or(&plan_path).to_string_lossy().to_string();
        let key = format!("{example_name}::{rel}");
        plans.push(load_plan_file(&plan_path, key, machine_override)?);
    }

    Ok(plans)
}

fn load_plan_file(path: &Path, key: String, machine_override: Option<&Path>) -> io::Result<Plan> {
    let text = fs::read_to_string(path)?;
    let rhei = parse(&text).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse {}: {}", path.display(), err.message),
        )
    })?;
    let machine = resolve_machine_for_plan(path, machine_override, &rhei)?;
    Ok(plan_from_rhei(path.to_path_buf(), key, &rhei, &machine))
}

fn plan_from_rhei(source: PathBuf, key: String, rhei: &Rhei, machine: &StateMachine) -> Plan {
    let tasks: Vec<Task> =
        rhei.tasks.iter().map(|task| top_level_task_from_ast(task, machine)).collect();
    let state = derive_plan_state(&tasks, machine);
    Plan { key, title: rhei.title.clone(), source, state, tasks }
}

fn top_level_task_from_ast(task: &AstTask, machine: &StateMachine) -> Task {
    let mut subtasks = Vec::new();
    collect_descendants(&task.children, machine, &mut subtasks);
    Task {
        id: task.id.to_string(),
        title: task.title.clone(),
        state: normalize_state(&task.state, machine),
        prior: task.prior.iter().map(ToString::to_string).collect(),
        subtasks,
    }
}

fn collect_descendants(children: &[AstTask], machine: &StateMachine, out: &mut Vec<Subtask>) {
    for child in children {
        out.push(Subtask {
            id: child.id.to_string(),
            title: child.title.clone(),
            state: normalize_state(&child.state, machine),
            prior: child.prior.iter().map(ToString::to_string).collect(),
        });
        collect_descendants(&child.children, machine, out);
    }
}

fn normalize_state(raw_state: &str, machine: &StateMachine) -> String {
    parse_task_state(raw_state, machine).state
}

/// Derive a level-0 plan state from the task states. See
/// `docs/functional-spec/rhei-viz.spec.md#plan-level-state-derivation`.
fn derive_plan_state(tasks: &[Task], machine: &StateMachine) -> String {
    if tasks.is_empty() {
        return "draft".into();
    }

    let states: Vec<&str> = tasks.iter().map(|task| task.state.as_str()).collect();
    if states.iter().all(|state| *state == "draft") {
        return "draft".into();
    }
    if states.iter().all(|state| *state == "completed") {
        return "completed".into();
    }

    let terminal_states: HashSet<&str> = machine
        .states
        .iter()
        .filter_map(|(name, def)| def.terminal.then_some(name.as_str()))
        .collect();
    if states.iter().all(|state| terminal_states.contains(*state)) {
        let first = states[0];
        if states.iter().all(|state| *state == first) && first != "cancelled" && first != "archived"
        {
            return "completed".into();
        }
        return "archived".into();
    }

    let initial_state =
        machine.states.iter().find_map(|(name, def)| def.initial.then_some(name.as_str()));
    let any_active = states.iter().any(|state| {
        !terminal_states.contains(*state) && *state != "draft" && Some(*state) != initial_state
    });

    if any_active {
        "active".into()
    } else {
        "pending".into()
    }
}

fn resolve_machine_for_plan(
    plan_path: &Path,
    machine_override: Option<&Path>,
    rhei: &Rhei,
) -> io::Result<StateMachine> {
    if let Some(machine_path) = machine_override {
        return load_machine(machine_path);
    }
    if rhei.states == "rhei" {
        return Ok(StateMachine::builtin_default());
    }
    let machine_path = plan_path.parent().unwrap_or_else(|| Path::new(".")).join("states.yaml");
    load_machine(&machine_path)
}

fn resolve_machine_for_workspace(
    workspace_dir: &Path,
    machine_override: Option<&Path>,
    rhei: &Rhei,
) -> io::Result<StateMachine> {
    if let Some(machine_path) = machine_override {
        return load_machine(machine_path);
    }
    if rhei.states == "rhei" {
        return Ok(StateMachine::builtin_default());
    }
    load_machine(&workspace_dir.join("states.yaml"))
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

fn escape_json_for_html_script(data: &str) -> String {
    let mut out = String::with_capacity(data.len());
    for ch in data.chars() {
        match ch {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            _ => out.push(ch),
        }
    }
    out
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
            let path = std::env::temp_dir().join(format!("rhei-xtask-viz-{prefix}-{stamp}"));
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
    fn render_html_escapes_script_breakouts() {
        let plans = vec![Plan {
            key: "demo".into(),
            title: "</script><script>alert(1)</script>".into(),
            source: PathBuf::from("/tmp/demo.rhei.md"),
            state: "pending".into(),
            tasks: vec![],
        }];

        let html = render_html(&plans);
        assert!(!html.contains("</script><script>alert(1)</script>"));
        assert!(
            html.contains("\\u003c/script\\u003e\\u003cscript\\u003ealert(1)\\u003c/script\\u003e")
        );
    }

    #[test]
    fn collect_plans_merges_workspace_and_skips_task_shards_as_standalone() {
        let temp = TempDir::new("workspace");
        fs::write(
            temp.path().join("index.rhei.md"),
            "# Rhei: Workspace\n**States:** rhei\n\n## Overview\nDemo.\n",
        )
        .expect("write index");
        fs::create_dir_all(temp.path().join("tasks")).expect("create tasks dir");
        fs::write(
            temp.path().join("tasks/alpha.rhei.md"),
            "### Task 1: Alpha\n**State:** pending\n",
        )
        .expect("write shard");
        fs::write(
            temp.path().join("extra.rhei.md"),
            "# Rhei: Extra\n**States:** rhei\n\n## Tasks\n\n### Task 1: Extra\n**State:** completed\n",
        )
        .expect("write standalone plan");

        let plans = collect_plans(temp.path(), "demo", None).expect("collect plans");

        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].title, "Workspace");
        assert_eq!(plans[1].title, "Extra");
    }

    #[test]
    fn collect_plans_preserves_non_task_descendants() {
        let temp = TempDir::new("deep-plan");
        let plan_path = temp.path().join("plan.rhei.md");
        fs::write(
            &plan_path,
            "# Rhei: Deep\n**States:** rhei\n---\nstructure:\n  maxLevels: 4\n  nodeKinds: [task, bug]\n---\n\n## Tasks\n\n### Task api: Build API\n**State:** pending\n\n#### Bug api.cache: Cache issue\n**State:** review\n\n##### Task api.cache.1: Fix cache key\n**State:** in-progress\n",
        )
        .expect("write plan");

        let plans = collect_plans(&plan_path, "demo", None).expect("collect plans");

        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].tasks[0].subtasks.len(), 2);
        assert_eq!(plans[0].tasks[0].subtasks[0].id, "api.cache");
        assert_eq!(plans[0].tasks[0].subtasks[1].id, "api.cache.1");
        assert_eq!(plans[0].tasks[0].subtasks[1].state, "in-progress");
    }

    #[test]
    fn derive_plan_state_uses_machine_normalization_and_terminals() {
        let temp = TempDir::new("machine");
        let plan_path = temp.path().join("plan.rhei.md");
        let machine_path = temp.path().join("states.yaml");
        fs::write(
            &plan_path,
            "# Rhei: Custom\n**States:** custom\n\n## Tasks\n\n### Task 1: Alpha\n**State:** fix\n",
        )
        .expect("write plan");
        fs::write(
            &machine_path,
            "name: custom\nversion: 1\nstates:\n  pending:\n    initial: true\n    description: Ready\n  fix:\n    description: Fixing\n  done:\n    description: Done\n    final: true\ntransitions:\n  - from: pending\n    to: fix\n  - from: fix\n    to: done\n",
        )
        .expect("write state machine");

        let active_plans = collect_plans(&plan_path, "demo", None).expect("collect active plan");
        assert_eq!(active_plans[0].state, "active");

        fs::write(
            &plan_path,
            "# Rhei: Custom\n**States:** custom\n\n## Tasks\n\n### Task 1: Alpha\n**State:** done\n",
        )
        .expect("rewrite plan");
        let completed_plans =
            collect_plans(&plan_path, "demo", None).expect("collect completed plan");
        assert_eq!(completed_plans[0].state, "completed");
    }
}
