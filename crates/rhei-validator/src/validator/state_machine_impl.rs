impl StateMachine {
    /// Return the built-in default state machine shipped with rhei.
    pub fn builtin_default() -> Self {
        Self::from_yaml_str(DEFAULT_STATES_YAML).expect("built-in states YAML is always valid")
    }

    /// Load a StateMachine from YAML string contents.
    pub fn from_yaml_str(yaml: &str) -> Result<Self, StateMachineLoadError> {
        reject_explicit_empty_all_targets(yaml)?;
        reject_inline_prompt_templates(yaml)?;
        let sm: Self = serde_yaml::from_str(yaml)?;
        sm.validate()
    }

    fn validate(self) -> Result<Self, StateMachineLoadError> {
        self.validate_model_configuration()?;
        self.validate_prompt_templates()?;
        self.validate_program_configuration()?;
        self.validate_snapshot_configuration()?;
        self.validate_tooling_configuration()?;
        self.validate_template_conditions()?;
        self.validate_poll_configuration()?;
        self.validate_profiles_and_node_policy()?;
        self.validate_terminal_state_present()?;
        Ok(self)
    }

    /// Reject state machines that declare zero terminal states. Without one,
    /// `rhei complete`, terminal-state filters, and prerequisite resolution
    /// cannot work correctly, and a forgotten or mistyped `final: true` is
    /// otherwise silently accepted.
    fn validate_terminal_state_present(&self) -> Result<(), StateMachineLoadError> {
        if self.states.values().any(|state| state.terminal) {
            return Ok(());
        }
        Err(StateMachineLoadError::Invalid(format!(
            "state machine '{}' declares no terminal states. Mark at least one \
             state with `final: true` (note: the field is `final`, not `terminal`).",
            self.name
        )))
    }

    /// Validate reusable prompt template declarations and per-state references.
    // §FS-rhei-states.4.4: Prompt templates must resolve their concrete placeholders.
    fn validate_prompt_templates(&self) -> Result<(), StateMachineLoadError> {
        for (template_name, template) in &self.prompt_templates {
            if template_name.trim().is_empty() {
                return Err(StateMachineLoadError::Invalid(
                    "prompt_templates contains an empty template id".to_string(),
                ));
            }
            let has_instructions = template
                .instructions
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty());
            if !has_instructions {
                return Err(StateMachineLoadError::Invalid(format!(
                    "prompt template '{template_name}' must contain non-empty Markdown prompt text"
                )));
            }
        }

        for (state_name, state) in &self.states {
            let Some(reference) = state.prompt_template.as_ref() else {
                continue;
            };
            let template_name = reference.name().trim();
            if template_name.is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' declares an empty 'prompt_template' name"
                )));
            }
            let template = self.prompt_templates.get(template_name).ok_or_else(|| {
                StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' references unknown prompt template '{template_name}'"
                ))
            })?;

            let values = reference.values();
            if let Some(values) = values {
                for (key, value) in values {
                    if !is_prompt_template_placeholder_name(key) {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' prompt_template.values contains invalid key '{key}' (expected an identifier)"
                        )));
                    }
                    if !is_prompt_template_scalar_value(value) {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' prompt_template.values.{key} must be a scalar value"
                        )));
                    }
                }
            }

            for (field_name, text) in [
                ("personality", template.personality.as_deref()),
                ("instructions", template.instructions.as_deref()),
            ] {
                let Some(text) = text else {
                    continue;
                };
                for token in extract_runtime_template_tokens(text) {
                    if is_prompt_template_control_token(token) {
                        continue;
                    }
                    if is_prompt_template_placeholder_name(token) {
                        if values.is_some_and(|values| values.contains_key(token)) {
                            continue;
                        }
                        return Err(StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' prompt template '{template_name}' {field_name} uses placeholder '{{{token}}}' but prompt_template.values does not supply '{token}'"
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    // §FS-rhei-states.9.2: Resolve non-root node profiles by policy order.

    /// Resolve the profile for a non-root node, following node-policy order:
    /// `overrides`, `by_type[<kind>]`, then `default`.
    /// Returns `None` when `profiles` / `node_policy` is absent.
    pub fn profile_for_node(&self, kind: &str, level: u8) -> Option<&Profile> {
        let (profiles, policy) = self.profiles.as_ref().zip(self.node_policy.as_ref())?;
        let resolved_name = policy
            .overrides
            .iter()
            .find(|ov| ov.match_.matches(kind, level))
            .map(|ov| ov.profile.as_str())
            .or_else(|| {
                policy
                    .by_type
                    .iter()
                    .find(|(candidate, _)| candidate.eq_ignore_ascii_case(kind))
                    .map(|(_, profile)| profile.as_str())
            })
            .unwrap_or(policy.default.as_str());
        profiles.get(resolved_name)
    }

    /// §FS-rhei-states.9.2: Resolve the profile bound to the plan-root node.
    pub fn root_profile(&self) -> Option<&Profile> {
        let (profiles, policy) = self.profiles.as_ref().zip(self.node_policy.as_ref())?;
        profiles.get(policy.root.as_str())
    }

    /// Load a StateMachine from a file path.
    pub fn from_yaml_file<P: AsRef<Path>>(path: P) -> Result<Self, StateMachineLoadError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)?;
        reject_explicit_empty_all_targets(&text)?;
        reject_inline_prompt_templates(&text)?;
        let mut sm: Self = serde_yaml::from_str(&text)?;
        reject_legacy_prompt_templates_file(path)?;
        let prompt_templates_dir = prompt_templates_dir_for_state_machine(path);
        sm.prompt_templates = load_prompt_templates_dir(&prompt_templates_dir)?;
        sm.validate()
    }

    /// Returns true if `state` is among the allowed states.
    pub fn is_valid_state<S: AsRef<str>>(&self, state: S) -> bool {
        self.states.contains_key(state.as_ref())
    }

    /// Return the set of allowed state names.
    pub fn allowed_states(&self) -> impl Iterator<Item = &str> {
        self.states.keys().map(|s| s.as_str())
    }

    /// Return the declared transitions between states.
    pub fn transitions(&self) -> &[TransitionRule] {
        &self.transitions
    }

    fn validate_model_configuration(&self) -> Result<(), StateMachineLoadError> {
        let mut seen = HashSet::new();
        for model in &self.models {
            let trimmed = model.trim();
            if trimmed.is_empty() {
                return Err(StateMachineLoadError::Invalid(
                    "top-level 'models' entries must be non-empty strings".to_string(),
                ));
            }
            if !seen.insert(trimmed) {
                return Err(StateMachineLoadError::Invalid(format!(
                    "top-level 'models' contains duplicate entry '{trimmed}'"
                )));
            }
        }

        for (state_name, state) in &self.states {
            if state.target.is_some() && !state.all_targets.is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' cannot set both 'target' and 'all_targets'"
                )));
            }
            if (state.target.is_some() || !state.all_targets.is_empty())
                && (state.model.is_some()
                    || !state.all_models.is_empty()
                    || state.agent.is_some()
                    || state.agent_mode.is_some())
            {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' cannot combine 'target' or 'all_targets' with \
                     'model', 'all_models', 'agent', or 'agent_mode'"
                )));
            }
            if let Some(selector) = state.target.as_deref() {
                parse_execution_target(selector).map_err(|message| {
                    StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' has invalid 'target': {message}"
                    ))
                })?;
            }
            if !state.all_targets.is_empty() {
                let mut seen_targets = HashSet::new();
                let mut seen_target_slugs: HashMap<String, String> = HashMap::new();
                for selector in &state.all_targets {
                    let parsed = parse_execution_target(selector).map_err(|message| {
                        StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' has invalid 'all_targets' entry: {message}"
                        ))
                    })?;
                    let normalized = parsed.selector();
                    if !seen_targets.insert(normalized.clone()) {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' contains duplicate 'all_targets' entry '{normalized}'"
                        )));
                    }
                    let slug = parsed.slug();
                    if let Some(previous) = seen_target_slugs.insert(slug.clone(), selector.clone())
                    {
                        return Err(StateMachineLoadError::Invalid(format!(
                            "state '{state_name}' has all_targets entries '{previous}' and '{selector}' that normalize to the same snapshot target slug '{slug}'"
                        )));
                    }
                }
            }
            if !state.all_models.is_empty() && state.model.is_some() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' cannot set both 'all_models' and 'model'"
                )));
            }

            if state.visits == Some(0) {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' declares 'visits: 0' but visits must be at least 1"
                )));
            }

            validate_artifact_definitions(state_name, "inputs", &state.inputs)?;
            validate_artifact_definitions(state_name, "outputs", &state.outputs)?;

            // Agent validation.
            if let Some(agent) = &state.agent {
                if state.terminal {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' is final and cannot declare an 'agent' (terminal states have no work to execute)"
                    )));
                }
                if agent.id().trim().is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' declares an empty 'agent' value"
                    )));
                }
            }
            if let Some(mode) = &state.agent_mode {
                if state.agent.is_none() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' declares 'agent_mode' without declaring an 'agent'"
                    )));
                }
                if mode.trim().is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' declares an empty 'agent_mode' value"
                    )));
                }
            }
            if let Some(timeout) = &state.agent_timeout {
                if parse_duration_secs(timeout).is_none() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' has invalid 'agent_timeout' value '{timeout}' \
                         (expected format like '30s', '5m', '1h', '2h30m')"
                    )));
                }
            }
            if let Some(timeout) = &state.program_timeout {
                if parse_duration_secs(timeout).is_none() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' has invalid 'program_timeout' value '{timeout}' \
                         (expected format like '30s', '5m', '1h', '2h30m')"
                    )));
                }
            }
            if state.agent.is_some() && state.program.is_some() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' cannot declare both 'agent' and 'program' (they are mutually exclusive)"
                )));
            }

            if !state.all_models.is_empty() && self.models.is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "state '{state_name}' sets 'all_models' but the machine does not declare any top-level 'models'"
                )));
            }

            let mut state_seen = HashSet::new();
            for model in &state.all_models {
                let trimmed = model.trim();
                if trimmed.is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' contains an empty 'all_models' entry"
                    )));
                }
                if !state_seen.insert(trimmed) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' contains duplicate 'all_models' entry '{trimmed}'"
                    )));
                }
                if !seen.contains(trimmed) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' references unknown model '{trimmed}' in 'all_models'"
                    )));
                }
            }

            if let Some(model) = state.model.as_deref() {
                let trimmed = model.trim();
                if trimmed.is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' declares an empty 'model' value"
                    )));
                }
                if self.models.is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' sets 'model: {trimmed}' but the machine does not declare any top-level 'models'"
                    )));
                }
                if !seen.contains(trimmed) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' references unknown model '{trimmed}'"
                    )));
                }
            }
        }

        Ok(())
    }


}

fn reject_explicit_empty_all_targets(yaml: &str) -> Result<(), StateMachineLoadError> {
    // `all_targets` carries `#[serde(default)]`, so serde collapses missing
    // and explicit-empty into the same empty Vec. Re-parse the raw YAML to
    // distinguish them and reject `all_targets: []` as authoring sugar that
    // most likely means "I intended to list targets here and forgot."
    let raw: serde_yaml::Value = serde_yaml::from_str(yaml)?;
    let Some(states) = raw.get("states").and_then(serde_yaml::Value::as_mapping) else {
        return Ok(());
    };

    for (state_name, state_value) in states {
        let Some(state) = state_value.as_mapping() else { continue };
        let Some(all_targets) = state.get("all_targets") else { continue };
        if all_targets.as_sequence().is_some_and(Vec::is_empty) {
            let label = state_name.as_str().unwrap_or("<unknown>");
            return Err(StateMachineLoadError::Invalid(format!(
                "state '{label}' declares 'all_targets: []' but all_targets must be a non-empty list when present"
            )));
        }
    }

    Ok(())
}

fn reject_inline_prompt_templates(yaml: &str) -> Result<(), StateMachineLoadError> {
    let raw: serde_yaml::Value = serde_yaml::from_str(yaml)?;
    if raw.get("prompt_templates").is_some() {
        return Err(StateMachineLoadError::Invalid(
            "'prompt_templates' must be defined as a sibling directory of Markdown files, not as a top-level field in 'states.yaml'"
                .to_string(),
        ));
    }
    Ok(())
}

fn reject_legacy_prompt_templates_file(path: &Path) -> Result<(), StateMachineLoadError> {
    let legacy_path = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("prompt-templates.yaml");
    if legacy_path.exists() {
        return Err(StateMachineLoadError::Invalid(
            "'prompt-templates.yaml' is no longer supported; place prompt Markdown files in sibling 'prompt_templates/'"
                .to_string(),
        ));
    }
    Ok(())
}

fn prompt_templates_dir_for_state_machine(path: &Path) -> PathBuf {
    path.parent()
        .unwrap_or_else(|| Path::new("."))
        .join("prompt_templates")
}

fn load_prompt_templates_dir(
    path: &Path,
) -> Result<IndexMap<String, PromptTemplateDef>, StateMachineLoadError> {
    let mut templates = IndexMap::new();
    if !path.exists() {
        return Ok(templates);
    }
    if !path.is_dir() {
        return Err(StateMachineLoadError::Invalid(format!(
            "prompt_templates path '{}' must be a directory",
            path.display()
        )));
    }

    let mut entries = std::fs::read_dir(path)?.collect::<Result<Vec<_>, std::io::Error>>()?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let file_type = entry.file_type()?;
        if !file_type.is_file() {
            continue;
        }
        let prompt_path = entry.path();
        if prompt_path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let template_name = prompt_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::trim)
            .filter(|stem| !stem.is_empty())
            .ok_or_else(|| {
                StateMachineLoadError::Invalid(format!(
                    "prompt template file '{}' must have a non-empty UTF-8 file stem",
                    prompt_path.display()
                ))
            })?
            .to_string();
        if templates.contains_key(&template_name) {
            return Err(StateMachineLoadError::Invalid(format!(
                "prompt_templates contains duplicate prompt template id '{template_name}'"
            )));
        }
        let text = std::fs::read_to_string(&prompt_path)?;
        templates.insert(
            template_name,
            PromptTemplateDef { personality: None, instructions: Some(text) },
        );
    }

    Ok(templates)
}

fn is_prompt_template_placeholder_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn is_prompt_template_scalar_value(value: &serde_yaml::Value) -> bool {
    matches!(
        value,
        serde_yaml::Value::Null
            | serde_yaml::Value::Bool(_)
            | serde_yaml::Value::Number(_)
            | serde_yaml::Value::String(_)
    )
}

fn extract_runtime_template_tokens(text: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut idx = 0usize;
    while idx < text.len() {
        if text[idx..].starts_with("\\{") {
            idx += 2;
            continue;
        }
        if !text[idx..].starts_with('{') {
            let ch = text[idx..].chars().next().expect("substring should have a char");
            idx += ch.len_utf8();
            continue;
        }
        let start = idx;
        let token_start = start + 1;
        let Some(end_offset) = text[token_start..].find('}') else {
            break;
        };
        let end = token_start + end_offset;
        tokens.push(&text[token_start..end]);
        idx = end + 1;
    }
    tokens
}

fn is_prompt_template_control_token(token: &str) -> bool {
    matches!(token, "else" | "endif") || token.starts_with("if ")
}
