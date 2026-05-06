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
        let mut entries: Vec<_> = std::fs::read_dir(&tasks_dir)
            .map_err(|e| ParseError::new(format!("failed to read tasks/ directory: {e}"), None))?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
            .collect();

        // Sort by filename for deterministic ordering.
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let path = entry.path();
            let content = std::fs::read_to_string(&path).map_err(|e| {
                ParseError::new(format!("failed to read {}: {e}", path.display()), None)
            })?;

            let tasks = parser::parse_workspace_tasks(&content).map_err(|e| {
                ParseError::new(format!("{}: {}", path.display(), e.message), e.line)
            })?;

            for task in &tasks {
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
                task_sources.insert(id_str, path.clone());
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
            structure: index.structure,
            metadata: index.metadata,
            content_sections: index.content_sections,
            tasks: all_tasks,
        },
        task_sources,
    })
}
