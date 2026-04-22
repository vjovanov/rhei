//! Token definitions for the Markdown Plan Compiler.
//!
//! These tokens cover the lexical elements defined in the plan language
//! specification. Fielded variants mirror the specification exactly.

use crate::ast::TaskId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// Top-level rhei header marker (e.g., "# Rhei: ...").
    RheiHeader,

    /// States declaration: "**States:** <name>"
    MetadataStates { name: String },

    /// Marker for the "## Tasks" section start.
    TasksSection,

    /// Section header: "## <title>" (non-Tasks H2 headers).
    SectionHeader { title: String },

    /// Node heading at H3..=H6 (`### <kind> <id>: <title>`,
    /// `#### <kind> <id>: <title>`, etc.).
    ///
    /// `level` is the heading depth (3..=6). `kind` is the heading keyword in
    /// its original casing. `id` is the full hierarchical id parsed from the
    /// heading.
    NodeHeader { level: u8, kind: String, id: TaskId },

    /// Metadata "Prior": "**Prior:** <kind> <id>, <kind> <id>, ..."
    MetadataPrior { task_ids: Vec<TaskId> },

    /// Metadata "State": "**State:** <state>"
    MetadataState { state: String },

    /// Metadata "Assignee": "**Assignee:** <name>"
    MetadataAssignee { name: String },

    /// Any non-heading, non-metadata text content.
    TextContent,
}
