//! Token definitions for the Markdown Plan Compiler.
//!
//! These tokens cover the lexical elements defined in the plan language
//! specification. Fielded variants mirror the specification exactly.

use crate::ast::TaskId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// Top-level saga header marker (e.g., "# Saga: ...").
    SagaHeader,

    /// Marker for the "## Tasks" section start.
    TasksSection,

    /// Task header: "### Task <id>: <title>"
    /// where <id> can be numeric (NUMBER) or named (IDENTIFIER).
    TaskHeader {
        id: TaskId,
    },

    /// Subtask header:
    /// "#### Subtask <task_number>.<subtask_number>: <title>"
    /// Subtask numbers are always numeric as per the specification.
    SubtaskHeader {
        task_number: u32,
        subtask_number: u32,
    },

    /// Metadata "Prior": "**Prior:** Task <id1>, Task <id2>, ..."
    /// where each task id may be numeric or named.
    MetadataPrior {
        task_ids: Vec<TaskId>,
    },

    /// Metadata "State": "**State:** <state>"
    MetadataState {
        state: String,
    },

    /// Any non-heading, non-metadata text content.
    ///
    /// Note: The specification lists TextContent without fields. For now,
    /// we model it as a unit variant per the spec. Content attachment
    /// decisions can be deferred to the parser/AST stage.
    TextContent,
}
