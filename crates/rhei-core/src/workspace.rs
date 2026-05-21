//! Directory Workspace loader for multi-file Rhei plans.
//!
//! A Directory Workspace consists of:
//! - `index.rhei.md`: root configuration with title, states, and content sections.
//! - `tasks/`: a directory containing `.md` files, each with one or more task definitions.
//!
//! All tasks are merged into a single global task graph. Task IDs must be
//! unique across the entire `tasks/` directory.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ast::{Rhei, Task};
use crate::parser::{self, ParseError};

/// A loaded directory workspace: the merged plan plus a map from each task ID
/// to the file it was parsed from (needed for targeted file rewrites during
/// transitions).
#[derive(Debug)]
pub struct Workspace {
    pub rhei: Rhei,
    /// Maps task ID (as string) → the file path that defines it.
    pub task_sources: HashMap<String, PathBuf>,
}

/// Returns `true` if `path` is a directory workspace
/// (a directory containing `index.rhei.md`).
pub fn is_workspace(path: &Path) -> bool {
    path.is_dir() && path.join("index.rhei.md").is_file()
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

            let tasks = parser::parse_workspace_tasks(&content).map_err(|e| {
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
