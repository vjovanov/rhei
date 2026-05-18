fn profile_state_can_reach_final(machine: &StateMachine, profile: &Profile, start: &str) -> bool {
    let allowed: HashSet<&str> = profile.allowed.iter().map(String::as_str).collect();
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([start.to_string()]);

    while let Some(state) = queue.pop_front() {
        if !seen.insert(state.clone()) {
            continue;
        }
        if machine.states.get(&state).is_some_and(|def| def.terminal) {
            return true;
        }
        for transition in machine.transitions.iter().filter(|transition| {
            (transition.from.0 == state || transition.from.0 == "*")
                && allowed.contains(transition.to.0.as_str())
        }) {
            queue.push_back(transition.to.0.clone());
        }
    }

    false
}

/// Extract every condition string from `{if <condition>}` tags in `text`.
fn extract_if_conditions(text: &str) -> Vec<&str> {
    let mut conditions = Vec::new();
    let mut remaining = text;
    while let Some(start) = remaining.find("{if ") {
        let after_open = start + "{if ".len();
        if let Some(close) = remaining[after_open..].find('}') {
            conditions.push(&remaining[after_open..after_open + close]);
            remaining = &remaining[after_open + close + 1..];
        } else {
            break;
        }
    }
    conditions
}

fn validate_state_mcp_entries(
    state_name: &str,
    state: &StateDef,
) -> Result<(), StateMachineLoadError> {
    let Some(entries) = state.mcp_servers.as_deref() else {
        return Ok(());
    };

    if !entries.is_empty() {
        if state.gating {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' is gating and cannot declare 'mcp_servers' (gating states are human-only)"
            )));
        }
        if state.program.is_some() {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' declares 'program' and cannot declare 'mcp_servers' (programs execute deterministically)"
            )));
        }
        if state.terminal {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' is final and cannot declare 'mcp_servers' (terminal states have no work)"
            )));
        }
    }

    let mut seen = HashSet::new();
    for entry in entries {
        let id = entry.id();
        if id.trim().is_empty() {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' has an mcp_servers entry with an empty id"
            )));
        }
        if !seen.insert(id.to_string()) {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' has a duplicate mcp_servers id '{id}'"
            )));
        }
        if let StateMcpEntry::Object(obj) = entry {
            if obj.command.is_some() && obj.url.is_some() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' mcp_servers entry '{id}' declares both 'command' and 'url' (mutually exclusive)"
                )));
            }
            if let Some(command) = &obj.command {
                if command.is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' mcp_servers entry '{id}' has an empty 'command'"
                    )));
                }
            }
            if let Some(url) = &obj.url {
                if url.trim().is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' mcp_servers entry '{id}' has an empty 'url'"
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_state_skill_entries(
    state_name: &str,
    state: &StateDef,
) -> Result<(), StateMachineLoadError> {
    let Some(entries) = state.skills.as_deref() else {
        return Ok(());
    };

    if !entries.is_empty() {
        if state.gating {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' is gating and cannot declare 'skills' (gating states are human-only)"
            )));
        }
        if state.program.is_some() {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' declares 'program' and cannot declare 'skills' (programs execute deterministically)"
            )));
        }
        if state.terminal {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' is final and cannot declare 'skills' (terminal states have no work)"
            )));
        }
    }

    let mut seen = HashSet::new();
    for entry in entries {
        let id = entry.id();
        if id.trim().is_empty() {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' has a skills entry with an empty id"
            )));
        }
        if !seen.insert(id.to_string()) {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' has a duplicate skills id '{id}'"
            )));
        }
        if let StateSkillEntry::Object(obj) = entry {
            if let Some(path) = &obj.path {
                if path.trim().is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' skills entry '{id}' has an empty 'path'"
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Validate the shape of a tooling-unavailable trigger: either `true` or a
/// non-empty list of non-empty string ids. `false` and other shapes are rejected.
fn validate_transition_tooling_trigger(
    transition: &TransitionRule,
    value: Option<&serde_yaml::Value>,
    field_name: &str,
) -> Result<(), StateMachineLoadError> {
    let Some(value) = value else { return Ok(()) };
    match value {
        serde_yaml::Value::Bool(true) => Ok(()),
        serde_yaml::Value::Bool(false) => Err(StateMachineLoadError::Invalid(format!(
            "transition from '{}' to '{}' declares '{field_name}: false' — omit the field instead",
            transition.from.0, transition.to.0
        ))),
        serde_yaml::Value::Sequence(items) => {
            if items.is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "transition from '{}' to '{}' declares an empty '{field_name}' list",
                    transition.from.0, transition.to.0
                )));
            }
            let mut seen = HashSet::new();
            for item in items {
                let Some(id) = item.as_str() else {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "transition from '{}' to '{}' '{field_name}' entries must be strings",
                        transition.from.0, transition.to.0
                    )));
                };
                if id.trim().is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "transition from '{}' to '{}' '{field_name}' contains an empty id",
                        transition.from.0, transition.to.0
                    )));
                }
                if !seen.insert(id.to_string()) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "transition from '{}' to '{}' '{field_name}' contains duplicate id '{id}'",
                        transition.from.0, transition.to.0
                    )));
                }
            }
            Ok(())
        }
        _ => Err(StateMachineLoadError::Invalid(format!(
            "transition from '{}' to '{}' '{field_name}' must be `true` or a list of ids",
            transition.from.0, transition.to.0
        ))),
    }
}

fn validate_program_value(
    state_name: &str,
    value: &serde_yaml::Value,
) -> Result<(), StateMachineLoadError> {
    match value {
        serde_yaml::Value::String(command) => {
            if command.trim().is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' declares an empty 'program' value"
                )));
            }
        }
        serde_yaml::Value::Mapping(mapping) => {
            let Some(command) = mapping.get(serde_yaml::Value::String("command".to_string()))
            else {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' program object must include a 'command' field"
                )));
            };
            validate_program_command(state_name, command)?;

            if let Some(env) = mapping.get(serde_yaml::Value::String("env".to_string())) {
                match env {
                    serde_yaml::Value::Mapping(env_map) => {
                        for (key, value) in env_map {
                            let Some(key) = key.as_str() else {
                                return Err(StateMachineLoadError::Invalid(format!(
                                    "state '{state_name}' program.env keys must be strings"
                                )));
                            };
                            if key.trim().is_empty() {
                                return Err(StateMachineLoadError::Invalid(format!(
                                    "state '{state_name}' program.env contains an empty key"
                                )));
                            }
                            if !matches!(
                                value,
                                serde_yaml::Value::Null
                                    | serde_yaml::Value::Bool(_)
                                    | serde_yaml::Value::Number(_)
                                    | serde_yaml::Value::String(_)
                            ) {
                                return Err(StateMachineLoadError::Invalid(format!(
                                    "state '{state_name}' program.env['{key}'] must be a scalar value"
                                )));
                            }
                        }
                    }
                    _ => {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' program.env must be a mapping"
                        )))
                    }
                }
            }

            if let Some(working_directory) =
                mapping.get(serde_yaml::Value::String("working_directory".to_string()))
            {
                match working_directory {
                    serde_yaml::Value::String(path) if !path.trim().is_empty() => {}
                    serde_yaml::Value::String(_) => {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' program.working_directory must be a non-empty string"
                        )))
                    }
                    _ => {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' program.working_directory must be a string"
                        )))
                    }
                }
            }

            if let Some(shell) = mapping.get(serde_yaml::Value::String("shell".to_string())) {
                if !matches!(shell, serde_yaml::Value::Bool(_)) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' program.shell must be a boolean"
                    )));
                }
            }
        }
        _ => {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' program must be a non-empty string or an object"
            )))
        }
    }

    Ok(())
}

fn validate_program_command(
    state_name: &str,
    command: &serde_yaml::Value,
) -> Result<(), StateMachineLoadError> {
    match command {
        serde_yaml::Value::String(value) => {
            if value.trim().is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' program.command must be a non-empty string"
                )));
            }
        }
        serde_yaml::Value::Sequence(values) => {
            if values.is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' program.command array must not be empty"
                )));
            }
            for value in values {
                match value {
                    serde_yaml::Value::String(item) if !item.trim().is_empty() => {}
                    _ => {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' program.command entries must be non-empty strings"
                        )))
                    }
                }
            }
        }
        _ => {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' program.command must be a string or string array"
            )))
        }
    }

    Ok(())
}

fn validate_artifact_definitions(
    state_name: &str,
    field_name: &str,
    artifacts: &[StateArtifactDef],
) -> Result<(), StateMachineLoadError> {
    let mut seen_names = HashSet::new();

    for artifact in artifacts {
        let name = artifact.name.trim();
        if name.is_empty() {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' contains an artifact in '{field_name}' with an empty 'name'"
            )));
        }
        if !seen_names.insert(name) {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' contains duplicate artifact name '{name}' in '{field_name}'"
            )));
        }

        let path = artifact.path.trim();
        if path.is_empty() {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' artifact '{name}' in '{field_name}' has an empty 'path'"
            )));
        }
        if artifact.optional && field_name == "outputs" {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' artifact '{name}' in 'outputs' may not be marked 'optional'; only inputs may be optional"
            )));
        }
        if Path::new(path).is_absolute() {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' artifact '{name}' in '{field_name}' must use a relative path, got '{path}'"
            )));
        }
        if path_escapes_workspace_root(path) {
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{state_name}' artifact '{name}' in '{field_name}' escapes the workspace root via path '{path}'"
            )));
        }
    }

    Ok(())
}

fn path_escapes_workspace_root(path: &str) -> bool {
    let expanded = path.replace("{task_id}", "task").replace("{state}", "state");
    let mut depth = 0usize;

    for component in Path::new(&expanded).components() {
        match component {
            Component::Prefix(_) | Component::RootDir => return true,
            Component::ParentDir => {
                if depth == 0 {
                    return true;
                }
                depth -= 1;
            }
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
        }
    }

    false
}

/// Parse a human-readable duration string into seconds.
///
/// Supported formats: `30s`, `5m`, `1h`, `2h30m`, `1h15m30s`.
/// Returns `None` if the string is not a valid duration.
pub fn parse_duration_secs(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let mut total: u64 = 0;
    let mut current_num = String::new();
    let mut found_any = false;

    for ch in s.chars() {
        if ch.is_ascii_digit() {
            current_num.push(ch);
        } else {
            let n: u64 = current_num.parse().ok()?;
            current_num.clear();
            match ch {
                'h' => total = total.checked_add(n.checked_mul(3600)?)?,
                'm' => total = total.checked_add(n.checked_mul(60)?)?,
                's' => total = total.checked_add(n)?,
                _ => return None,
            }
            found_any = true;
        }
    }

    // Reject trailing digits without a unit suffix or empty input.
    if !current_num.is_empty() || !found_any {
        return None;
    }

    Some(total)
}

