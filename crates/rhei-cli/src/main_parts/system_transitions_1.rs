
/// Origin metadata for a state transition. Lets callers override the
/// `triggeredBy` slot on the `TransitionContext` passed to callbacks and
/// seed `transitionData` with engine-side values (e.g. the timeout
/// duration that triggered the rule).
#[derive(Debug, Default, Clone)]
struct TransitionOrigin {
    /// Override the default `triggered_by` slot. `None` falls back to
    /// `"user"` (or `"callback"` when an on_leave redirect rerouted).
    triggered_by: Option<&'static str>,
    /// Initial `transitionData` payload. On_leave callbacks merge into this
    /// last-write-wins per spec §Timeout Callbacks.
    seed_data: Option<serde_json::Value>,
}

/// Variant of [`execute_transition`] that fires the rule with a system-set
/// origin — currently used by the timeout watchdog to label the transition
/// as `triggeredBy: 'system'` and to seed `transitionData.timeout` per
/// spec §Timeout Callbacks.
#[allow(clippy::too_many_arguments)]
fn execute_system_timeout_transition(
    files: TransitionFiles<'_>,
    callback_paths: &CallbackPaths,
    machine: &rhei_validator::StateMachine,
    task_id_str: &str,
    from: &str,
    to: &str,
    timeout_label: &str,
    no_callbacks: bool,
) -> MietteResult<()> {
    let mut data = serde_json::Map::new();
    data.insert("timeout".to_string(), serde_json::Value::String(timeout_label.to_string()));
    execute_transition_with_origin(
        files,
        callback_paths,
        machine,
        task_id_str,
        from,
        to,
        no_callbacks,
        TransitionOrigin {
            triggered_by: Some("system"),
            seed_data: Some(serde_json::Value::Object(data)),
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_system_tooling_transition(
    files: TransitionFiles<'_>,
    callback_paths: &CallbackPaths,
    machine: &rhei_validator::StateMachine,
    task_id_str: &str,
    from: &str,
    to: &str,
    kind: ToolingKind,
    unavailable: &[String],
    no_callbacks: bool,
) -> MietteResult<()> {
    let mut data = serde_json::Map::new();
    data.insert(
        "unavailable".to_string(),
        serde_json::Value::Array(
            unavailable.iter().cloned().map(serde_json::Value::String).collect(),
        ),
    );
    data.insert("kind".to_string(), serde_json::Value::String(kind.as_str().to_string()));
    execute_transition_with_origin(
        files,
        callback_paths,
        machine,
        task_id_str,
        from,
        to,
        no_callbacks,
        TransitionOrigin {
            triggered_by: Some("system"),
            seed_data: Some(serde_json::Value::Object(data)),
        },
    )
}
