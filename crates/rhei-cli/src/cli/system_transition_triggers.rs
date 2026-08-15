
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
    /// last-write-wins.
    // §FS-rhei-agents.7.5: Timeout transition data merge.
    seed_data: Option<serde_json::Value>,
    /// System failure routes leave the source state because work failed, not
    /// because the source state's success artifacts were produced.
    skip_source_outputs: bool,
    /// Result message the caller carries through the move. It satisfies the
    /// terminal-result obligation on an edge into a `final: true` state, and is
    /// appended to `runtime/results/<task-id>.md` once the move succeeds.
    ///
    /// Only a caller that *knows* the outcome sets this: the human verbs pass
    /// `--result`, and the engine passes one on the routes the engine itself
    /// ended. A canned "advanced by the engine" line would be provenance, which
    /// the ledger already carries, and not a result.
    // §FS-rhei-states.3.3 §FS-rhei-run.3
    result_message: Option<String>,
    /// A message the engine records *only* when the effective target turns out
    /// to be `final: true` and nothing else answered for the ticket.
    ///
    /// Callback-only advancement sets it: taking the edge is an outcome the
    /// engine produced, with no subprocess in the source state that could know
    /// better, so the engine narrates exactly that. It is not a carried
    /// message, because the same sentence would be a lie on a non-terminal hop
    /// and noise beside a result a callback did write.
    // §FS-rhei-run.3
    terminal_result_fallback: Option<String>,
}

/// The message `rhei run` records when it takes an edge itself, with no agent
/// or program having run in the source state.
///
/// It says plainly that no worker result was recorded — which is the fact a
/// reader of the file needs, and the fact the old empty result file withheld.
/// It is a fallback: a result a callback wrote wins, and on a non-terminal hop
/// nothing is written at all.
// §FS-rhei-run.3 §FS-rhei-states.3.3
fn callback_only_terminal_result(from: &str) -> String {
    format!(
        "`rhei run`: this task was finished by callback-only orchestration from state '{from}'. \
         No agent or program ran in that state, so no worker result was recorded."
    )
}

/// Variant of [`execute_transition`] for `rhei run`'s callback-only
/// advancement, which carries the engine's own account of the move for the case
/// where the edge lands on a `final: true` state and nothing else answered.
#[allow(clippy::too_many_arguments)]
fn execute_callback_only_transition(
    files: TransitionFiles<'_>,
    callback_paths: &CallbackPaths,
    machine: &rhei_validator::StateMachine,
    task_id_str: &str,
    from: &str,
    to: &str,
    no_callbacks: bool,
) -> MietteResult<String> {
    execute_transition_with_origin(
        files,
        callback_paths,
        machine,
        task_id_str,
        from,
        to,
        no_callbacks,
        TransitionOrigin {
            terminal_result_fallback: Some(callback_only_terminal_result(from)),
            ..TransitionOrigin::default()
        },
    )
}

/// Variant of [`execute_transition`] that fires the rule with a system-set
/// origin — currently used by the timeout watchdog to label the transition
/// as `triggeredBy: 'system'` and to seed `transitionData.timeout`.
// §FS-rhei-agents.7.5: System timeout transition origin.
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
) -> MietteResult<String> {
    let mut data = serde_json::Map::new();
    data.insert("timeout".to_string(), serde_json::Value::String(timeout_label.to_string()));
    // §FS-rhei-run.3: the engine ended this work, so the engine says why.
    let message = if timeout_label.is_empty() {
        format!("`rhei run`: the subprocess timed out in state '{from}'.")
    } else {
        format!("`rhei run`: the subprocess timed out after {timeout_label} in state '{from}'.")
    };
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
            skip_source_outputs: true,
            result_message: Some(message),
            terminal_result_fallback: None,
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
) -> MietteResult<String> {
    let mut data = serde_json::Map::new();
    data.insert(
        "unavailable".to_string(),
        serde_json::Value::Array(
            unavailable.iter().cloned().map(serde_json::Value::String).collect(),
        ),
    );
    data.insert("kind".to_string(), serde_json::Value::String(kind.as_str().to_string()));
    // §FS-rhei-run.3: the engine ended this work, so the engine says why.
    let message = format!(
        "`rhei run`: required {} unavailable in state '{}': {}.",
        kind.as_str(),
        from,
        unavailable.join(", ")
    );
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
            skip_source_outputs: true,
            result_message: Some(message),
            terminal_result_fallback: None,
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_system_program_exit_transition(
    files: TransitionFiles<'_>,
    callback_paths: &CallbackPaths,
    machine: &rhei_validator::StateMachine,
    task_id_str: &str,
    from: &str,
    to: &str,
    exit_code: i32,
    no_callbacks: bool,
) -> MietteResult<String> {
    let mut data = serde_json::Map::new();
    data.insert("exitCode".to_string(), serde_json::Value::from(exit_code));
    // A zero exit is the worker reporting success: the worker owns the result,
    // exactly as it does on the ordinary auto-advance path. A non-zero exit is
    // the engine ending the work, so the engine says why. §FS-rhei-run.3
    let message = (exit_code != 0)
        .then(|| format!("`rhei run`: the subprocess exited {exit_code} in state '{from}'."));
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
            skip_source_outputs: exit_code != 0,
            result_message: message,
            terminal_result_fallback: None,
        },
    )
}
