//! Directory Workspace loader for multi-file Rhei plans.
//!
//! A Directory Workspace consists of:
//! - `index.rhei.md`: root configuration with title, states, and content sections.
//! - `tasks/`: a directory containing `.md` files, each with one or more task definitions.
//!
//! All tasks are merged into a single global task graph. Task IDs must be
//! unique across the entire `tasks/` directory.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::ast::{
    ContentSection, Rhei, Structure, Task, TaskId, TaskIdSegment, DEFAULT_MAX_LEVELS,
};
use crate::parser::{self, ParseError};

pub const PANTA_INDEX_FILE: &str = "index.panta.md";
pub const RHEIS_DIR: &str = "rheis";
pub const BASIN_RHEI_ID: &str = "basin";

/// A loaded directory workspace: the merged plan plus a map from each task ID
/// to the file it was parsed from (needed for targeted file rewrites during
/// transitions).
#[derive(Debug)]
pub struct Workspace {
    pub rhei: Rhei,
    /// Maps task ID (as string) → the file path that defines it.
    pub task_sources: HashMap<String, PathBuf>,
}

/// A loaded Panta project, flattened into a project-qualified task graph for
/// the existing task execution pipeline. §AR-rhei-panta.2 §AR-rhei-panta.3
#[derive(Debug)]
pub struct PantaProject {
    pub rhei: Rhei,
    /// Maps project-qualified task ID (`auth.1`) → the file path that defines it.
    pub task_sources: HashMap<String, PathBuf>,
    /// Rhei ids in presentation order; `basin` is always last when present.
    pub rhei_ids: Vec<String>,
}

/// Returns `true` if `path` is a directory workspace
/// (a directory containing `index.rhei.md`).
pub fn is_workspace(path: &Path) -> bool {
    path.is_dir() && path.join("index.rhei.md").is_file()
}

/// Returns `true` if `path` is a Panta project directory.
pub fn is_panta_project(path: &Path) -> bool {
    path.is_dir() && path.join(PANTA_INDEX_FILE).is_file()
}

/// Resolve a Panta project directory from either the project directory or its
/// `index.panta.md` manifest path. §FS-rhei-panta.6
pub fn panta_project_dir(path: &Path) -> Option<PathBuf> {
    if is_panta_project(path) {
        return Some(path.to_path_buf());
    }
    if path.is_file() && path.file_name().and_then(|n| n.to_str()) == Some(PANTA_INDEX_FILE) {
        return path.parent().map(Path::to_path_buf);
    }
    None
}

/// Resolve the workspace directory for `path`, accepting either:
/// - a workspace directory (containing `index.rhei.md`), or
/// - the `index.rhei.md` file itself, when its parent directory contains
///   a `tasks/` subdirectory.
///
/// Callers that need the workspace root regardless of which form the user
/// supplied should prefer this over `is_workspace`.
pub fn workspace_dir(path: &Path) -> Option<PathBuf> {
    if is_workspace(path) {
        return Some(path.to_path_buf());
    }
    if path.is_file() && path.file_name().and_then(|n| n.to_str()) == Some("index.rhei.md") {
        if let Some(parent) = path.parent() {
            if parent.join("tasks").is_dir() {
                return Some(parent.to_path_buf());
            }
        }
    }
    None
}

/// Discover workspace task files recursively in deterministic plan order.
pub fn discover_task_files(tasks_dir: &Path) -> parser::Result<Vec<PathBuf>> {
    fn is_hidden(path: &Path) -> bool {
        path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with('.'))
    }

    fn visit(dir: &Path, out: &mut Vec<PathBuf>) -> parser::Result<()> {
        let entries = std::fs::read_dir(dir)
            .map_err(|e| ParseError::new(format!("failed to read {}: {e}", dir.display()), None))?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                ParseError::new(format!("failed to read {}: {e}", dir.display()), None)
            })?;
            let path = entry.path();
            if is_hidden(&path) {
                continue;
            }
            let file_type = entry.file_type().map_err(|e| {
                ParseError::new(format!("failed to inspect {}: {e}", path.display()), None)
            })?;
            if file_type.is_dir() {
                visit(&path, out)?;
            } else if file_type.is_file()
                && path.extension().and_then(|ext| ext.to_str()) == Some("md")
            {
                out.push(path);
            }
        }

        Ok(())
    }

    let mut files = Vec::new();
    if tasks_dir.is_dir() {
        visit(tasks_dir, &mut files)?;
    }
    files.sort_by(|a, b| {
        let a_key = a.strip_prefix(tasks_dir).unwrap_or(a).to_string_lossy().replace('\\', "/");
        let b_key = b.strip_prefix(tasks_dir).unwrap_or(b).to_string_lossy().replace('\\', "/");
        a_key.cmp(&b_key)
    });
    Ok(files)
}

/// Discover rhei entries under a Panta project's `rheis/` directory.
///
/// Entries are single-file rheis (`*.rhei.md`) or Directory Workspace roots.
/// Non-hidden paths are walked recursively in normalized lexical order; once a
/// workspace root is found, its own task files are not considered rhei entries.
pub fn discover_rhei_entries(rheis_dir: &Path) -> parser::Result<Vec<PathBuf>> {
    fn is_hidden(path: &Path) -> bool {
        path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with('.'))
    }

    fn is_single_file_rhei(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".rhei.md"))
    }

    fn visit(dir: &Path, out: &mut Vec<PathBuf>) -> parser::Result<()> {
        let entries = std::fs::read_dir(dir)
            .map_err(|e| ParseError::new(format!("failed to read {}: {e}", dir.display()), None))?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                ParseError::new(format!("failed to read {}: {e}", dir.display()), None)
            })?;
            let path = entry.path();
            if is_hidden(&path) {
                continue;
            }
            let file_type = entry.file_type().map_err(|e| {
                ParseError::new(format!("failed to inspect {}: {e}", path.display()), None)
            })?;
            if file_type.is_dir() {
                if is_workspace(&path) {
                    out.push(path);
                } else {
                    visit(&path, out)?;
                }
            } else if file_type.is_file() && is_single_file_rhei(&path) {
                out.push(path);
            }
        }

        Ok(())
    }

    let mut entries = Vec::new();
    if rheis_dir.is_dir() {
        visit(rheis_dir, &mut entries)?;
    }
    entries.sort_by(|a, b| {
        let a_key = a.strip_prefix(rheis_dir).unwrap_or(a).to_string_lossy().replace('\\', "/");
        let b_key = b.strip_prefix(rheis_dir).unwrap_or(b).to_string_lossy().replace('\\', "/");
        a_key.cmp(&b_key)
    });
    Ok(entries)
}

/// Load a Panta project, merging all contained rheis into one graph with
/// project-qualified task ids. §AR-rhei-panta.2 §AR-rhei-panta.3
pub fn load_panta_project(dir: &Path) -> parser::Result<PantaProject> {
    let manifest_path = dir.join(PANTA_INDEX_FILE);
    let manifest_content = std::fs::read_to_string(&manifest_path).map_err(|e| {
        ParseError::new(format!("failed to read {}: {e}", manifest_path.display()), None)
    })?;
    let manifest = parser::parse_panta_manifest(&manifest_content).map_err(|e| {
        ParseError::new(format!("{}: {}", manifest_path.display(), e.message), e.line)
    })?;

    let mut rheis = Vec::new();
    let mut seen_ids = HashSet::new();
    let entries = discover_rhei_entries(&dir.join(RHEIS_DIR))?;
    for entry in entries {
        let id = rhei_id_for_entry(&entry)?;
        validate_rhei_id(&id, &entry)?;
        if id == BASIN_RHEI_ID {
            return Err(ParseError::new(
                format!(
                    "`{}` is reserved for the synthetic basin rhei and cannot be used by {}",
                    BASIN_RHEI_ID,
                    entry.display()
                ),
                None,
            ));
        }
        if !seen_ids.insert(id.clone()) {
            return Err(ParseError::new(
                format!("duplicate rhei id '{id}' in Panta project"),
                None,
            ));
        }
        let loaded = load_rhei_entry(&entry)?;
        validate_panta_rhei_states(&id, &loaded.rhei, &manifest.states, manifest.states_declared)?;
        rheis.push((id, loaded.rhei, loaded.task_sources));
    }

    let basin_dir = dir.join(BASIN_RHEI_ID);
    if basin_dir.is_dir() {
        if !seen_ids.insert(BASIN_RHEI_ID.to_string()) {
            return Err(ParseError::new("duplicate synthetic basin rhei id", None));
        }
        let loaded = load_basin_rhei(&basin_dir, &manifest.structure, &manifest.states)?;
        rheis.push((BASIN_RHEI_ID.to_string(), loaded.rhei, loaded.task_sources));
    }

    let rhei_ids: Vec<String> = rheis.iter().map(|(id, _, _)| id.clone()).collect();
    let mut all_tasks = Vec::new();
    let mut task_sources = HashMap::new();
    let mut merged_structure = manifest.structure.clone();
    let mut content_sections = manifest.content_sections.clone();
    for (rhei_id, mut rhei, sources) in rheis {
        merge_structure(&mut merged_structure, &rhei.structure);
        content_sections.push(ContentSection {
            title: format!("Rhei {rhei_id}: {}", rhei.title),
            content: String::new(),
        });
        qualify_tasks(&mut rhei.tasks, &rhei_id, &rhei_ids);
        for task in &rhei.tasks {
            collect_task_sources(
                task,
                source_for_task(&sources, task).as_path(),
                &mut task_sources,
            )?;
        }
        all_tasks.extend(rhei.tasks);
    }

    if all_tasks.is_empty() {
        return Err(ParseError::new(
            "Panta project contains no tasks (rheis/ and basin/ are empty or missing)",
            None,
        ));
    }

    Ok(PantaProject {
        rhei: Rhei {
            title: manifest.title,
            states: manifest.states,
            states_declared: manifest.states_declared,
            structure: merged_structure,
            metadata: manifest.metadata,
            content_sections,
            tasks: all_tasks,
        },
        task_sources,
        rhei_ids,
    })
}

fn load_rhei_entry(path: &Path) -> parser::Result<Workspace> {
    if let Some(ws_dir) = workspace_dir(path) {
        load_workspace(&ws_dir)
    } else {
        let content = std::fs::read_to_string(path).map_err(|e| {
            ParseError::new(format!("failed to read {}: {e}", path.display()), None)
        })?;
        let rhei = parser::parse(&content)
            .map_err(|e| ParseError::new(format!("{}: {}", path.display(), e.message), e.line))?;
        let mut task_sources = HashMap::new();
        for task in &rhei.tasks {
            collect_task_sources(task, path, &mut task_sources)?;
        }
        Ok(Workspace { rhei, task_sources })
    }
}

fn load_basin_rhei(dir: &Path, structure: &Structure, states: &str) -> parser::Result<Workspace> {
    let mut tasks = Vec::new();
    let mut task_sources = HashMap::new();
    for path in discover_task_files(dir)? {
        let content = std::fs::read_to_string(&path).map_err(|e| {
            ParseError::new(format!("failed to read {}: {e}", path.display()), None)
        })?;
        let parsed = parser::parse_workspace_tasks_with_structure(&content, structure)
            .map_err(|e| ParseError::new(format!("{}: {}", path.display(), e.message), e.line))?;
        for task in &parsed {
            collect_task_sources(task, &path, &mut task_sources)?;
        }
        tasks.extend(parsed);
    }
    Ok(Workspace {
        rhei: Rhei {
            title: "Basin".to_string(),
            states: states.to_string(),
            states_declared: false,
            structure: structure.clone(),
            metadata: None,
            content_sections: Vec::new(),
            tasks,
        },
        task_sources,
    })
}

fn validate_panta_rhei_states(
    id: &str,
    rhei: &Rhei,
    manifest_states: &str,
    manifest_states_declared: bool,
) -> parser::Result<()> {
    if !rhei.states_declared {
        return Ok(());
    }
    let effective_project_states = if manifest_states_declared { manifest_states } else { "rhei" };
    if rhei.states.trim() == effective_project_states {
        return Ok(());
    }
    Err(ParseError::new(
        format!(
            "Panta rhei '{id}' declares state machine '{}', but the current flattened loader supports only the project-wide state machine '{}'",
            rhei.states.trim(),
            effective_project_states
        ),
        None,
    ))
}

fn rhei_id_for_entry(path: &Path) -> parser::Result<String> {
    if path.is_dir() {
        return path
            .file_name()
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned)
            .ok_or_else(|| ParseError::new(format!("invalid rhei path {}", path.display()), None));
    }

    let name = path.file_name().and_then(|name| name.to_str()).ok_or_else(|| {
        ParseError::new(format!("invalid rhei filename {}", path.display()), None)
    })?;
    let Some(stem) = name.strip_suffix(".rhei.md") else {
        return Err(ParseError::new(format!("invalid rhei filename {}", path.display()), None));
    };
    Ok(stem.to_string())
}

fn validate_rhei_id(id: &str, path: &Path) -> parser::Result<()> {
    let valid = id.bytes().next().is_some_and(|b| b.is_ascii_alphabetic())
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if valid {
        Ok(())
    } else {
        Err(ParseError::new(
            format!("rhei id '{id}' from {} is not a valid IDENTIFIER", path.display()),
            None,
        ))
    }
}

fn merge_structure(into: &mut Structure, from: &Structure) {
    into.max_levels = into.max_levels.max(from.max_levels).max(DEFAULT_MAX_LEVELS);
    for kind in &from.node_kinds {
        if !into.node_kinds.iter().any(|existing| existing.eq_ignore_ascii_case(kind)) {
            into.node_kinds.push(kind.clone());
        }
    }
}

fn qualify_tasks(tasks: &mut [Task], rhei_id: &str, rhei_ids: &[String]) {
    for task in tasks {
        qualify_task(task, rhei_id, rhei_ids);
    }
}

fn qualify_task(task: &mut Task, rhei_id: &str, rhei_ids: &[String]) {
    task.id = qualify_local_id(&task.id, rhei_id);
    task.profile_depth_offset = task.profile_depth_offset.saturating_add(1);
    for prior in &mut task.prior {
        if !is_project_qualified(prior, rhei_ids) {
            *prior = qualify_local_id(prior, rhei_id);
        }
    }
    for child in &mut task.children {
        qualify_task(child, rhei_id, rhei_ids);
    }
}

fn qualify_local_id(id: &TaskId, rhei_id: &str) -> TaskId {
    let mut segments = Vec::with_capacity(id.segments.len() + 1);
    segments.push(TaskIdSegment::Named(rhei_id.to_string()));
    segments.extend(id.segments.clone());
    TaskId::from_segments(segments)
}

fn is_project_qualified(id: &TaskId, rhei_ids: &[String]) -> bool {
    let Some(TaskIdSegment::Named(first)) = id.segments.first() else {
        return false;
    };
    id.segments.len() > 1 && rhei_ids.iter().any(|rhei_id| rhei_id == first)
}

fn source_for_task(sources: &HashMap<String, PathBuf>, task: &Task) -> PathBuf {
    let local = TaskId::from_segments(task.id.segments.iter().skip(1).cloned().collect());
    sources.get(&local.to_string()).cloned().unwrap_or_default()
}

/// Load a directory workspace, merging all task files into a single plan.
///
/// Reads `index.rhei.md` for plan metadata, then discovers and parses every
/// `.md` file inside the `tasks/` subdirectory. Reports duplicate task IDs
/// across files and missing structure.
pub fn load_workspace(dir: &Path) -> parser::Result<Workspace> {
    let index_path = dir.join("index.rhei.md");
    let index_content = std::fs::read_to_string(&index_path).map_err(|e| {
        ParseError::new(format!("failed to read {}: {e}", index_path.display()), None)
    })?;

    let index = parser::parse_workspace_index(&index_content)
        .map_err(|e| ParseError::new(format!("{}: {}", index_path.display(), e.message), e.line))?;

    let tasks_dir = dir.join("tasks");
    let mut all_tasks: Vec<Task> = Vec::new();
    let mut task_sources: HashMap<String, PathBuf> = HashMap::new();

    if tasks_dir.is_dir() {
        for path in discover_task_files(&tasks_dir)? {
            let content = std::fs::read_to_string(&path).map_err(|e| {
                ParseError::new(format!("failed to read {}: {e}", path.display()), None)
            })?;

            let tasks = parser::parse_workspace_tasks_with_structure(&content, &index.structure)
                .map_err(|e| {
                    ParseError::new(format!("{}: {}", path.display(), e.message), e.line)
                })?;

            for task in &tasks {
                collect_task_sources(task, &path, &mut task_sources)?;
            }

            all_tasks.extend(tasks);
        }
    }

    if all_tasks.is_empty() {
        return Err(ParseError::new(
            "workspace contains no tasks (tasks/ directory is empty or missing)",
            None,
        ));
    }

    Ok(Workspace {
        rhei: Rhei {
            title: index.title,
            states: index.states,
            states_declared: index.states_declared,
            structure: index.structure,
            metadata: index.metadata,
            content_sections: index.content_sections,
            tasks: all_tasks,
        },
        task_sources,
    })
}

fn collect_task_sources(
    task: &Task,
    path: &Path,
    task_sources: &mut HashMap<String, PathBuf>,
) -> parser::Result<()> {
    let id_str = task.id.to_string();
    if let Some(existing) = task_sources.get(&id_str) {
        return Err(ParseError::new(
            format!(
                "duplicate task ID '{}': defined in both {} and {}",
                id_str,
                existing.display(),
                path.display()
            ),
            None,
        ));
    }
    task_sources.insert(id_str, path.to_path_buf());

    for child in &task.children {
        collect_task_sources(child, path, task_sources)?;
    }

    Ok(())
}
