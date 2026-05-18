use serde_json::{json, Map, Value};

use rhei_core::ast::{Rhei, Task, TaskId, TaskIdSegment};

use crate::PlanOutputGenerator;

pub struct JsonOutput {
    pub pretty: bool,
}

impl PlanOutputGenerator for JsonOutput {
    fn generate_rhei(&self, rhei: &rhei_core::ast::Rhei) -> serde_json::Value {
        rhei_json(rhei)
    }
}

/// Convert a parsed Rhei into a serde_json::Value.
pub fn to_json_value(rhei: &rhei_core::ast::Rhei) -> serde_json::Value {
    JsonOutput { pretty: false }.generate_rhei(rhei)
}

/// Convert a parsed Rhei into a pretty-printed JSON string.
pub fn to_json_string_pretty(rhei: &rhei_core::ast::Rhei) -> String {
    let v = to_json_value(rhei);
    serde_json::to_string_pretty(&v).expect("pretty JSON serialization")
}

// -----------------------------------------------------------------------------
// Internal helpers
// -----------------------------------------------------------------------------

fn id_segment_json(seg: &TaskIdSegment) -> Value {
    match seg {
        TaskIdSegment::Number(n) => json!({ "number": n }),
        TaskIdSegment::Named(s) => json!({ "named": s }),
    }
}

fn task_id_json(id: &TaskId) -> Value {
    let segments: Vec<Value> = id.segments.iter().map(id_segment_json).collect();
    json!({
        "path": id.to_string(),
        "segments": segments,
    })
}

fn task_json(t: &Task) -> Value {
    let prior = t.prior.iter().map(task_id_json).collect::<Vec<Value>>();
    let children = t.children.iter().map(task_json).collect::<Vec<Value>>();

    let mut obj = Map::new();
    obj.insert("id".to_string(), task_id_json(&t.id));
    obj.insert("kind".to_string(), Value::String(t.kind.clone()));
    obj.insert("title".to_string(), Value::String(t.title.clone()));
    obj.insert("state".to_string(), Value::String(t.state.clone()));
    obj.insert("prior".to_string(), Value::Array(prior));
    if let Some(ref assignee) = t.assignee {
        obj.insert("assignee".to_string(), Value::String(assignee.clone()));
    }
    if !t.content.is_empty() {
        obj.insert("content".to_string(), Value::String(t.content.clone()));
    }
    obj.insert("children".to_string(), Value::Array(children));
    Value::Object(obj)
}

fn rhei_json(rhei: &Rhei) -> Value {
    let content_sections = rhei
        .content_sections
        .iter()
        .map(|s| {
            json!({
                "title": s.title,
                "content": s.content,
            })
        })
        .collect::<Vec<Value>>();

    let tasks = rhei.tasks.iter().map(task_json).collect::<Vec<Value>>();

    json!({
        "title": rhei.title,
        "states": rhei.states,
        "structure": {
            "max_levels": rhei.structure.max_levels,
            "node_kinds": rhei.structure.node_kinds,
        },
        "frontmatter": rhei.metadata.as_ref().and_then(|metadata| serde_json::to_value(metadata).ok()),
        "content_sections": content_sections,
        "tasks": tasks
    })
}
