//! Abstract Syntax Tree (AST) definitions for the Markdown Plan Compiler.
//!
//! These types model the hierarchical structure parsed from plan markdown,
//! aligning with the specification in docs/plan-language-spec.md.
//!
//! High-level model:
//! - Rhei: holds the plan title, free-form content blocks prior to the Tasks section,
//!         and the list of tasks.
//! - Task: a task with an identifier (numeric or named), a title, metadata, and subtasks.
//! - Subtask: a numbered subtask (scoped to its parent task) with a title and content.
//! - TaskMetadata: normalized representation of metadata fields (State, Prior).
//! - ContentBlock: generic carrier for non-structural content at the rhei level.
//!
//! Notes:
//! - Additional content block types (lists, code blocks, etc.) can be introduced later.
//! - Validation of metadata semantics is handled by the validator crate (not here).

use serde::{Deserialize, Serialize};
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
    /// A named H2 section with its title and accumulated content.
    Section { title: String, content: String },
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
        Self { depends_on: Vec::new(), state: None, state_first: true }
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
    /// Free-form content accumulated from lines between metadata and the first subtask.
    pub content: String,
    /// List of subtasks parsed under this task.
    pub subtasks: Vec<Subtask>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rhei {
    /// Title captured from the "# Rhei: <title>" header.
    pub title: String,
    /// Name of the state machine to use. Defaults to "rhei".
    pub states: String,
    /// Content appearing before the "## Tasks" section.
    pub content: Vec<ContentBlock>,
    /// Collection of tasks defined under the "## Tasks" section.
    pub tasks: Vec<Task>,
}

/// State name used in state machine transition definitions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StateName(pub String);

/// Unique transition identifier built from source and target state names.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransitionKey {
    /// Source state for the transition (`from` in YAML/JSON).
    pub from: StateName,
    /// Target state for the transition (`to` in YAML/JSON).
    pub to: StateName,
}

/// Reference to a platform-prefixed callback identifier.
///
/// Examples: `cli:validate_preconditions`, `js:prepareProcessing`,
/// `py:validate_dataset`, `java:com.example.Handler::method`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CallbackRef(pub String);

/// Declarative transition rule from the state machine configuration.
///
/// Field names and serde mappings follow the transition YAML/JSON shape from
/// the formal transition docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionRule {
    /// Source state name, or `"*"` for wildcard semantics.
    pub from: StateName,
    /// Target state name.
    pub to: StateName,
    /// Optional callback invoked before leaving the source state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_leave: Option<CallbackRef>,
    /// Optional callback invoked after entering the target state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_enter: Option<CallbackRef>,
    /// Optional condition expression for system-triggered transitions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Optional timeout duration (for example `24h`, `30m`, `45s`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn state_name_equality_and_roundtrip_json() {
        let a = StateName("pending".to_string());
        let b = StateName("pending".to_string());
        let c = StateName("running".to_string());

        assert_eq!(a, b);
        assert_ne!(a, c);

        let encoded = serde_json::to_string(&a).expect("serialize StateName");
        assert_eq!(encoded, "\"pending\"");

        let decoded: StateName = serde_json::from_str(&encoded).expect("deserialize StateName");
        assert_eq!(decoded, a);
    }

    #[test]
    fn callback_ref_roundtrip_json() {
        let callback = CallbackRef("cli:validate_preconditions".to_string());

        let encoded = serde_json::to_string(&callback).expect("serialize CallbackRef");
        assert_eq!(encoded, "\"cli:validate_preconditions\"");

        let decoded: CallbackRef = serde_json::from_str(&encoded).expect("deserialize CallbackRef");
        assert_eq!(decoded, callback);
    }

    #[test]
    fn transition_key_construction_and_json() {
        let key = TransitionKey {
            from: StateName("draft".to_string()),
            to: StateName("pending".to_string()),
        };

        let value = serde_json::to_value(&key).expect("serialize TransitionKey");
        assert_eq!(
            value,
            json!({
                "from": "draft",
                "to": "pending"
            })
        );

        let decoded: TransitionKey =
            serde_json::from_value(value).expect("deserialize TransitionKey");
        assert_eq!(decoded, key);
    }

    #[test]
    fn transition_rule_serializes_with_optional_fields_omitted() {
        let rule = TransitionRule {
            from: StateName("pending".to_string()),
            to: StateName("running".to_string()),
            on_leave: None,
            on_enter: None,
            condition: None,
            timeout: None,
        };

        let value = serde_json::to_value(&rule).expect("serialize TransitionRule");
        assert_eq!(
            value,
            json!({
                "from": "pending",
                "to": "running"
            })
        );
    }

    #[test]
    fn transition_rule_full_roundtrip_with_callbacks_and_edge_values() {
        let original = TransitionRule {
            from: StateName("*".to_string()),
            to: StateName("cancelled".to_string()),
            on_leave: Some(CallbackRef("js:package_for_review".to_string())),
            on_enter: Some(CallbackRef(
                "java:com.example.workflow.WorkflowHandlers::recordCompletion".to_string(),
            )),
            condition: Some("retryCount >= 3".to_string()),
            timeout: Some("24h".to_string()),
        };

        let value = serde_json::to_value(&original).expect("serialize TransitionRule");
        assert_eq!(
            value,
            json!({
                "from": "*",
                "to": "cancelled",
                "on_leave": "js:package_for_review",
                "on_enter": "java:com.example.workflow.WorkflowHandlers::recordCompletion",
                "condition": "retryCount >= 3",
                "timeout": "24h"
            })
        );

        let decoded: TransitionRule =
            serde_json::from_value(value).expect("deserialize TransitionRule");
        assert_eq!(decoded, original);
    }

    #[test]
    fn transition_rule_deserializes_without_optional_fields() {
        let input = json!({
            "from": "queued",
            "to": "processing"
        });

        let rule: TransitionRule =
            serde_json::from_value(input).expect("deserialize TransitionRule");
        assert_eq!(rule.from, StateName("queued".to_string()));
        assert_eq!(rule.to, StateName("processing".to_string()));
        assert_eq!(rule.on_leave, None);
        assert_eq!(rule.on_enter, None);
        assert_eq!(rule.condition, None);
        assert_eq!(rule.timeout, None);
    }

    #[test]
    fn transition_rule_rejects_non_string_callback_ref() {
        let input = json!({
            "from": "pending",
            "to": "running",
            "on_leave": { "invalid": true }
        });

        let err =
            serde_json::from_value::<TransitionRule>(input).expect_err("expected invalid callback");
        let message = err.to_string();
        assert!(message.contains("string"), "error should mention string type, got: {message}");
    }

    #[test]
    fn transition_rule_json_field_names_match_spec() {
        let rule = TransitionRule {
            from: StateName("in-progress".to_string()),
            to: StateName("human-review".to_string()),
            on_leave: Some(CallbackRef("cli:package_for_review".to_string())),
            on_enter: Some(CallbackRef("cli:notify_reviewers".to_string())),
            condition: Some("priority == \"high\"".to_string()),
            timeout: Some("30m".to_string()),
        };

        let obj = serde_json::to_value(&rule).expect("serialize TransitionRule");
        let keys = match obj {
            Value::Object(map) => map.keys().cloned().collect::<Vec<_>>(),
            _ => panic!("expected object"),
        };

        assert!(keys.contains(&"from".to_string()));
        assert!(keys.contains(&"to".to_string()));
        assert!(keys.contains(&"on_leave".to_string()));
        assert!(keys.contains(&"on_enter".to_string()));
        assert!(keys.contains(&"condition".to_string()));
        assert!(keys.contains(&"timeout".to_string()));
    }
}
