#[derive(Clone)]
struct CallbackPaths {
    plan_path: PathBuf,
    state_machine_path: Option<PathBuf>,
    working_dir: PathBuf,
}

#[derive(Clone, Copy)]
struct TransitionFiles<'a> {
    task_file: &'a Path,
    metadata_file: &'a Path,
}

fn resolve_callback_paths(
    state_machine_path: Option<&Path>,
    plan_path: &Path,
) -> MietteResult<CallbackPaths> {
    let plan_path = plan_path.canonicalize().map_err(|err| {
        file_io_report(plan_path, "failed to resolve plan path for callbacks", err)
    })?;
    let state_machine_path = state_machine_path
        .map(|path| {
            path.canonicalize().map_err(|err| {
                file_io_report(path, "failed to resolve state machine path for callbacks", err)
            })
        })
        .transpose()?;
    let base_dir = if let Some(path) = state_machine_path.as_deref() {
        path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."))
    } else if plan_path.is_dir() {
        plan_path.as_path()
    } else {
        plan_path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."))
    };

    let working_dir = base_dir.canonicalize().map_err(|err| {
        file_io_report(base_dir, "failed to resolve callback working directory", err)
    })?;

    Ok(CallbackPaths { plan_path, state_machine_path, working_dir })
}

fn execution_workspace_root(plan_path: &Path) -> PathBuf {
    if plan_path.is_dir() {
        plan_path.to_path_buf()
    } else {
        plan_path.parent().unwrap_or(Path::new(".")).to_path_buf()
    }
}

/// Build the cross-language `TransitionContext` JSON payload that is
/// delivered to callbacks on stdin.
///
/// Shape matches `docs/functional-spec/rhei-transitions.spec.md` #transitioncontext-data-structure.
/// The `triggered_by` field must be one of `"user" | "callback" | "system" | "engine"`.
/// `transition_data` seeds the `transitionData` slot; pass `serde_json::Value::Object(Map::new())`
/// for the initial `on_leave` call, and the accumulated data from `on_leave` for `on_enter`.
#[allow(clippy::too_many_arguments)]
fn build_transition_context_json(
    plan: Option<&rhei_core::ast::Rhei>,
    plan_path: &Path,
    task_id_str: &str,
    from_state: &str,
    to_state: &str,
    triggered_by: &str,
    transition_data: &serde_json::Value,
    working_dir: &Path,
) -> serde_json::Value {
    use serde_json::{json, Map, Value};

    let task_node = plan.and_then(|rhei| find_task_by_id_str(&rhei.tasks, task_id_str));

    let task_json = match task_node {
        Some(task) => json!({
            "id": task_id_to_json(&task.id),
            "kind": task.kind,
            "title": task.title,
            "content": task.content,
            "metadata": task_metadata_json(plan, task_id_str, from_state),
            "children": task.children.iter().map(task_summary_json).collect::<Vec<_>>(),
        }),
        None => json!({
            "id": task_id_str,
            "metadata": Value::Object(Map::new()),
            "children": Value::Array(Vec::new()),
        }),
    };

    let rhei_json = match plan {
        Some(rhei) => json!({
            "title": rhei.title,
            "path": plan_path.display().to_string(),
            "tasks": rhei.tasks.iter().map(task_summary_json).collect::<Vec<_>>(),
        }),
        None => json!({
            "title": Value::Null,
            "path": plan_path.display().to_string(),
            "tasks": Value::Array(Vec::new()),
        }),
    };

    json!({
        "rhei": rhei_json,
        "task": task_json,
        "transition": {
            "from": from_state,
            "to": to_state,
            "triggeredBy": triggered_by,
            "timestamp": current_iso8601(),
        },
        "transitionData": transition_data,
        "environment": {
            "platform": "cli",
            "version": env!("CARGO_PKG_VERSION"),
            "workingDirectory": working_dir.display().to_string(),
        },
    })
}

fn task_id_to_json(id: &TaskId) -> serde_json::Value {
    serde_json::Value::String(id.to_string())
}

fn task_summary_json(task: &rhei_core::ast::Task) -> serde_json::Value {
    serde_json::json!({
        "id": task_id_to_json(&task.id),
        "kind": task.kind,
        "title": task.title,
        "state": task.state,
    })
}

fn task_metadata_json(
    plan: Option<&rhei_core::ast::Rhei>,
    task_id_str: &str,
    state: &str,
) -> serde_json::Value {
    use serde_json::{Map, Value};

    let mut out = Map::new();
    out.insert("state".to_string(), Value::String(state.to_string()));

    let target = parse_task_id(task_id_str);
    let task = plan.and_then(|rhei| find_task_by_id(&rhei.tasks, &target));
    if let Some(task) = task {
        out.insert(
            "dependsOn".to_string(),
            Value::Array(task.prior.iter().map(|id| Value::String(id.to_string())).collect()),
        );
    } else {
        out.insert("dependsOn".to_string(), Value::Array(Vec::new()));
    }

    // Merge frontmatter task metadata (`metadata.tasks.<id>`) if present.
    if let Some(rhei) = plan {
        if let Some(task_meta) = task_metadata_map(rhei.metadata.as_ref(), &target) {
            for (key, value) in task_meta {
                let Some(key_str) = key.as_str() else { continue };
                // Don't clobber canonical fields.
                if key_str == "state" || key_str == "dependsOn" {
                    continue;
                }
                if let Ok(json_value) = yaml_value_to_json(value) {
                    out.insert(key_str.to_string(), json_value);
                }
            }
        }
    }

    Value::Object(out)
}

fn yaml_value_to_json(value: &YamlValue) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::to_value(value)
}

fn find_task_by_id_str<'a>(
    tasks: &'a [rhei_core::ast::Task],
    task_id_str: &str,
) -> Option<&'a rhei_core::ast::Task> {
    let target = parse_task_id(task_id_str);
    find_task_by_id(tasks, &target)
}

fn find_task_by_id<'a>(
    tasks: &'a [rhei_core::ast::Task],
    target: &TaskId,
) -> Option<&'a rhei_core::ast::Task> {
    for task in tasks {
        if &task.id == target {
            return Some(task);
        }
        if let Some(found) = find_task_by_id(&task.children, target) {
            return Some(found);
        }
    }
    None
}

fn current_iso8601() -> String {
    let now =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = now.as_secs();
    let (year, month, day, hour, minute, second) = unix_to_utc_components(secs);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", year, month, day, hour, minute, second)
}

/// Minimal civil-calendar conversion from seconds-since-Unix-epoch to UTC
/// Y-M-D h:m:s components. Uses the Howard Hinnant algorithm. Good through
/// the full 64-bit range; we don't need leap seconds or time zones for a
/// transition timestamp.
fn unix_to_utc_components(secs: u64) -> (i32, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let time_of_day = (secs % 86_400) as u32;
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;

    // days since 1970-01-01 → civil date.
    let z = days + 719_468;
    let era = if z >= 0 { z / 146_097 } else { (z - 146_096) / 146_097 };
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = y + if m <= 2 { 1 } else { 0 };
    (y as i32, m as u32, d as u32, hour, minute, second)
}

/// Write `content` atomically to `path` via a same-directory temp file.
fn write_file_atomic(path: &Path, content: &str) -> MietteResult<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|err| miette!("failed to create temp file: {err}"))?;
    tmp.write_all(content.as_bytes()).map_err(|err| miette!("failed to write temp file: {err}"))?;
    tmp.persist(path).map_err(|err| miette!("failed to persist temp file: {err}"))?;
    Ok(())
}

fn write_plan_metadata(input: &Path, metadata: &Metadata) -> MietteResult<()> {
    let metadata_file = if workspace::is_workspace(input) {
        input.join("index.rhei.md")
    } else {
        input.to_path_buf()
    };
    let raw = fs::read_to_string(&metadata_file)
        .map_err(|err| file_io_report(&metadata_file, "failed to read plan metadata file", err))?;
    let updated = rewrite_frontmatter(&raw, metadata)?;
    write_file_atomic(&metadata_file, &updated)
}

fn record_poll_self_loop_if_needed(
    input: &Path,
    metadata: Option<&Metadata>,
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
    current_state: &str,
    to_state: &str,
) -> MietteResult<bool> {
    if current_state != to_state {
        return Ok(false);
    }
    let Some(poll) = machine.states.get(current_state).and_then(|def| def.poll.as_ref()) else {
        return Ok(false);
    };
    let interval = rhei_validator::parse_duration_secs(&poll.interval).unwrap_or(0);
    let next_attempt_count =
        current_state_visit_count(metadata, &task.id, current_state, task.state.as_str(), machine)
            .saturating_add(1);
    let metadata = set_poll_next_attempt_metadata(
        metadata,
        &task.id,
        current_state,
        current_unix_secs().saturating_add(interval),
        next_attempt_count,
    );
    write_plan_metadata(input, &metadata)?;
    Ok(true)
}

/// Merge `src` object keys into `dst` (last write wins). Non-object `src`
/// values are ignored. Used to accumulate `data` payloads across multiple
/// `on_leave` callbacks into a single `transitionData`.
fn merge_transition_data(dst: &mut serde_json::Value, src: &serde_json::Value) {
    use serde_json::Value;
    let (Value::Object(dst_map), Value::Object(src_map)) = (dst, src) else {
        return;
    };
    for (key, value) in src_map {
        dst_map.insert(key.clone(), value.clone());
    }
}
