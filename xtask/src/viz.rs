//! Generate an HTML visualization for a rhei plan (or workspace of plans).
//!
//! Dogfoods the `rhei viz` command specified in
//! [`docs/specs/rhei-viz.spec.md`](../../../docs/specs/rhei-viz.spec.md)
//! before the real subcommand ships. Keep the data shape and derivation
//! rules consistent with that spec so this implementation migrates cleanly.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const TEMPLATE: &str = include_str!("../assets/viz-template.html");
const DATA_PLACEHOLDER: &str = "/*__DATA__*/null";

#[derive(Debug)]
pub struct Plan {
    pub key: String,
    pub title: String,
    pub source: PathBuf,
    pub state: String,
    pub tasks: Vec<Task>,
}

#[derive(Debug)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub state: String,
    pub prior: Vec<String>,
    pub subtasks: Vec<Subtask>,
}

#[derive(Debug)]
pub struct Subtask {
    pub id: String,
    pub title: String,
    pub state: String,
    pub prior: Vec<String>,
}

pub fn parse_plan(path: &Path, key: String) -> io::Result<Plan> {
    let text = fs::read_to_string(path)?;
    let mut title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("plan")
        .to_string();
    let mut tasks: Vec<Task> = Vec::new();
    let mut cur_task: Option<Task> = None;
    // Track which element the next `**State:**` / `**Prior:**` annotates.
    let mut scope: Scope = Scope::None;

    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        if let Some(t) = line.strip_prefix("# ") {
            title = t.trim().to_string();
            continue;
        }
        if let Some((id, tt)) = parse_task_header(line) {
            if let Some(task) = cur_task.take() {
                tasks.push(task);
            }
            cur_task = Some(Task {
                id,
                title: tt,
                state: "pending".into(),
                prior: Vec::new(),
                subtasks: Vec::new(),
            });
            scope = Scope::Task;
            continue;
        }
        if let Some((id, tt)) = parse_subtask_header(line) {
            if let Some(task) = cur_task.as_mut() {
                task.subtasks.push(Subtask {
                    id,
                    title: tt,
                    state: "pending".into(),
                    prior: Vec::new(),
                });
            }
            scope = Scope::Subtask;
            continue;
        }
        if let Some(state) = parse_state_line(line) {
            apply_state(&mut cur_task, scope, &state);
        } else if let Some(prior) = parse_prior_line(line) {
            apply_prior(&mut cur_task, scope, prior);
        }
    }
    if let Some(task) = cur_task.take() {
        tasks.push(task);
    }

    let state = derive_plan_state(&tasks);
    Ok(Plan {
        key,
        title,
        source: path.to_path_buf(),
        state,
        tasks,
    })
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Scope {
    None,
    Task,
    Subtask,
}

fn apply_state(cur_task: &mut Option<Task>, scope: Scope, state: &str) {
    if let Some(task) = cur_task.as_mut() {
        match scope {
            Scope::Task => task.state = state.to_string(),
            Scope::Subtask => {
                if let Some(s) = task.subtasks.last_mut() {
                    s.state = state.to_string();
                }
            }
            Scope::None => {}
        }
    }
}

fn apply_prior(cur_task: &mut Option<Task>, scope: Scope, prior: Vec<String>) {
    if let Some(task) = cur_task.as_mut() {
        match scope {
            Scope::Task => task.prior = prior,
            Scope::Subtask => {
                if let Some(s) = task.subtasks.last_mut() {
                    s.prior = prior;
                }
            }
            Scope::None => {}
        }
    }
}

fn parse_task_header(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("### Task ")?;
    let (id, title) = rest.split_once(':')?;
    if id.contains('.') {
        return None;
    }
    Some((id.trim().to_string(), title.trim().to_string()))
}

fn parse_subtask_header(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("#### Task ")?;
    let (id, title) = rest.split_once(':')?;
    if !id.contains('.') {
        return None;
    }
    Some((id.trim().to_string(), title.trim().to_string()))
}

fn parse_state_line(line: &str) -> Option<String> {
    let rest = line.strip_prefix("**State:**")?.trim();
    let unbacked = rest.trim_matches('`').trim();
    if unbacked.is_empty() {
        None
    } else {
        Some(unbacked.to_string())
    }
}

fn parse_prior_line(line: &str) -> Option<Vec<String>> {
    let rest = line.strip_prefix("**Prior:**")?.trim();
    let parts: Vec<String> = rest
        .split(',')
        .map(|p| p.trim().trim_start_matches("Task ").trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    Some(parts)
}

/// Derive a level-0 plan state from the task states. See
/// `docs/specs/rhei-viz.spec.md#plan-level-state-derivation`.
fn derive_plan_state(tasks: &[Task]) -> String {
    if tasks.is_empty() {
        return "draft".into();
    }
    let all_draft = tasks.iter().all(|t| t.state == "draft");
    if all_draft {
        return "draft".into();
    }
    let all_completed = tasks.iter().all(|t| t.state == "completed");
    if all_completed {
        return "completed".into();
    }
    let all_terminal = tasks
        .iter()
        .all(|t| matches!(t.state.as_str(), "completed" | "cancelled" | "archived"));
    if all_terminal {
        return "archived".into();
    }
    let any_active = tasks.iter().any(|t| {
        matches!(
            t.state.as_str(),
            "in_progress"
                | "needs-review"
                | "review"
                | "prove"
                | "consolidate"
                | "agent-review"
                | "agent-review-fix"
                | "human-review"
        )
    });
    if any_active {
        "active".into()
    } else {
        "pending".into()
    }
}

// -----------------------------------------------------------------------------
// JSON emission — manual so xtask keeps zero external dependencies.

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

fn str_array(strs: &[String]) -> String {
    let parts: Vec<String> = strs.iter().map(|s| format!("\"{}\"", escape_json(s))).collect();
    format!("[{}]", parts.join(","))
}

fn subtask_json(s: &Subtask) -> String {
    format!(
        "{{\"id\":\"{}\",\"title\":\"{}\",\"state\":\"{}\",\"prior\":{}}}",
        escape_json(&s.id),
        escape_json(&s.title),
        escape_json(&s.state),
        str_array(&s.prior),
    )
}

fn task_json(t: &Task) -> String {
    let subs: Vec<String> = t.subtasks.iter().map(subtask_json).collect();
    format!(
        "{{\"id\":\"{}\",\"title\":\"{}\",\"state\":\"{}\",\"prior\":{},\"subtasks\":[{}]}}",
        escape_json(&t.id),
        escape_json(&t.title),
        escape_json(&t.state),
        str_array(&t.prior),
        subs.join(","),
    )
}

fn plan_json(p: &Plan) -> String {
    let tasks: Vec<String> = p.tasks.iter().map(task_json).collect();
    format!(
        "{{\"title\":\"{}\",\"source\":\"{}\",\"state\":\"{}\",\"tasks\":[{}]}}",
        escape_json(&p.title),
        escape_json(&p.source.to_string_lossy()),
        escape_json(&p.state),
        tasks.join(","),
    )
}

fn bundle_json(plans: &[Plan]) -> String {
    let entries: Vec<String> = plans
        .iter()
        .map(|p| format!("\"{}\":{}", escape_json(&p.key), plan_json(p)))
        .collect();
    format!("{{{}}}", entries.join(","))
}

// -----------------------------------------------------------------------------
// HTML rendering

pub fn render_html(plans: &[Plan]) -> String {
    let data = bundle_json(plans);
    TEMPLATE.replace(DATA_PLACEHOLDER, &data)
}

pub fn collect_plans(path: &Path, example_name: &str) -> io::Result<Vec<Plan>> {
    if path.is_file() {
        return Ok(vec![parse_plan(path, example_name.to_string())?]);
    }
    if !path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no such file or directory: {}", path.display()),
        ));
    }

    // Workspace-style example: merge `index.rhei.md` (for title/context) with
    // every task shard under `tasks/**/*.md` into a single synthesized plan.
    // Standalone `.rhei.md` siblings (not `index.rhei.md`) get their own plan.
    let mut rhei_files: Vec<PathBuf> = Vec::new();
    walk_matching(path, &mut rhei_files, |p| {
        p.file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.ends_with(".rhei.md"))
            .unwrap_or(false)
    })?;
    rhei_files.sort();

    let index = rhei_files
        .iter()
        .find(|p| p.file_name().map(|s| s == "index.rhei.md").unwrap_or(false))
        .cloned();

    let tasks_dir = path.join("tasks");
    let mut shards: Vec<PathBuf> = Vec::new();
    if tasks_dir.is_dir() {
        walk_matching(&tasks_dir, &mut shards, |p| {
            p.extension().and_then(|s| s.to_str()) == Some("md")
        })?;
        shards.sort();
    }

    let mut plans = Vec::new();
    if index.is_some() || !shards.is_empty() {
        let merged_path = index.clone().unwrap_or_else(|| path.to_path_buf());
        let mut buf = String::new();
        if let Some(idx) = &index {
            buf.push_str(&fs::read_to_string(idx)?);
            buf.push('\n');
        }
        // Ensure the parser sees a `## Tasks` header so shard content lands in
        // the task scope — task shards use `### Task …` headers directly.
        if !shards.is_empty() && !buf.contains("## Tasks") {
            buf.push_str("\n## Tasks\n\n");
        }
        for shard in &shards {
            buf.push_str(&fs::read_to_string(shard)?);
            buf.push('\n');
        }
        let synthetic = path.join(".rhei-viz-merged.md");
        let plan = parse_merged(&merged_path, &synthetic, &buf, example_name.to_string());
        plans.push(plan);
    }

    // Any additional standalone *.rhei.md siblings (not index) render separately.
    for f in rhei_files
        .iter()
        .filter(|p| p.file_name().map(|s| s != "index.rhei.md").unwrap_or(true))
    {
        let rel = f
            .strip_prefix(path)
            .unwrap_or(f)
            .to_string_lossy()
            .to_string();
        let key = format!("{}::{}", example_name, rel);
        plans.push(parse_plan(f, key)?);
    }

    Ok(plans)
}

fn walk_matching<F: Fn(&Path) -> bool + Copy>(
    dir: &Path,
    out: &mut Vec<PathBuf>,
    pred: F,
) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            walk_matching(&p, out, pred)?;
        } else if pred(&p) {
            out.push(p);
        }
    }
    Ok(())
}

fn parse_merged(source: &Path, _synthetic: &Path, text: &str, key: String) -> Plan {
    let mut title = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("plan")
        .to_string();
    let mut tasks: Vec<Task> = Vec::new();
    let mut cur_task: Option<Task> = None;
    let mut scope: Scope = Scope::None;
    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        if let Some(t) = line.strip_prefix("# ") {
            if title == source.file_stem().and_then(|s| s.to_str()).unwrap_or("plan") {
                title = t.trim().to_string();
            }
            continue;
        }
        if let Some((id, tt)) = parse_task_header(line) {
            if let Some(task) = cur_task.take() {
                tasks.push(task);
            }
            cur_task = Some(Task {
                id,
                title: tt,
                state: "pending".into(),
                prior: Vec::new(),
                subtasks: Vec::new(),
            });
            scope = Scope::Task;
            continue;
        }
        if let Some((id, tt)) = parse_subtask_header(line) {
            if let Some(task) = cur_task.as_mut() {
                task.subtasks.push(Subtask {
                    id,
                    title: tt,
                    state: "pending".into(),
                    prior: Vec::new(),
                });
            }
            scope = Scope::Subtask;
            continue;
        }
        if let Some(state) = parse_state_line(line) {
            apply_state(&mut cur_task, scope, &state);
        } else if let Some(prior) = parse_prior_line(line) {
            apply_prior(&mut cur_task, scope, prior);
        }
    }
    if let Some(task) = cur_task.take() {
        tasks.push(task);
    }
    let state = derive_plan_state(&tasks);
    Plan {
        key,
        title,
        source: source.to_path_buf(),
        state,
        tasks,
    }
}
