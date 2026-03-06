//! Abstract Syntax Tree (AST) definitions for the Markdown Plan Compiler.
//!
//! These types model the hierarchical structure parsed from plan markdown,
//! aligning with the specification in docs/plan-language-spec.md.
//!
//! High-level model:
//! - Saga: holds the plan title, free-form content blocks prior to the Tasks section,
//!         and the list of tasks.
//! - Task: a task with an identifier (numeric or named), a title, metadata, and subtasks.
//! - Subtask: a numbered subtask (scoped to its parent task) with a title and content.
//! - TaskMetadata: normalized representation of metadata fields (State, Prior).
//! - ContentBlock: generic carrier for non-structural content at the saga level.
//!
//! Notes:
//! - Additional content block types (lists, code blocks, etc.) can be introduced later.
//! - Validation of metadata semantics is handled by the validator crate (not here).

use std::fmt;

/// Task identifier supporting either a numeric id or a named identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TaskId {
    Number(u32),
    Named(String),
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskId::Number(n) => write!(f, "{}", n),
            TaskId::Named(s) => write!(f, "{}", s),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentBlock {
    /// A plain text paragraph or line collected outside of the Tasks section.
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskMetadata {
    /// Normalized "Prior" dependencies as task identifiers.
    pub depends_on: Vec<TaskId>,
    /// Optional "State" value (raw string, semantic validation deferred).
    pub state: Option<String>,
    /// True if the first seen metadata line for this task was **State:**.
    /// The parser sets this, and the validator enforces the ordering rule.
    pub state_first: bool,
}

impl Default for TaskMetadata {
    fn default() -> Self {
        Self {
            depends_on: Vec::new(),
            state: None,
            state_first: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subtask {
    /// Parent task number as declared in the header (redundant but explicit).
    pub task_number: u32,
    /// This subtask's ordinal within its parent task, as declared.
    pub subtask_number: u32,
    /// Title captured from the subtask header.
    pub title: String,
    /// Free-form content accumulated until the next header (or EOF).
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// Task identifier as declared (numeric or named).
    pub id: TaskId,
    /// Title captured from the task header.
    pub title: String,
    /// Metadata normalized into a structured form.
    pub metadata: TaskMetadata,
    /// List of subtasks parsed under this task.
    pub subtasks: Vec<Subtask>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saga {
    /// Title captured from the "# Saga: <title>" header.
    pub title: String,
    /// Content appearing before the "## Tasks" section.
    pub content: Vec<ContentBlock>,
    /// Collection of tasks defined under the "## Tasks" section.
    pub tasks: Vec<Task>,
}
