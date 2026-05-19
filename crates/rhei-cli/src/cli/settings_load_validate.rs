
/// Load settings from a JSON file, returning defaults if the file doesn't exist.
#[cfg(test)]
fn load_settings(path: &Path) -> MietteResult<RheiSettings> {
    Ok(load_settings_document(path)?.typed)
}

fn json_field_present(raw: &serde_json::Value, key: &str) -> bool {
    raw.as_object().map(|obj| obj.contains_key(key)).unwrap_or(false)
}

fn json_child<'a>(raw: &'a serde_json::Value, key: &str) -> &'a serde_json::Value {
    raw.as_object().and_then(|obj| obj.get(key)).unwrap_or(&serde_json::Value::Null)
}

fn json_nested_field_present(raw: &serde_json::Value, section: &str, key: &str) -> bool {
    json_child(raw, section).as_object().map(|obj| obj.contains_key(key)).unwrap_or(false)
}

fn merge_model_agent_binding(
    existing: &mut ModelAgentBinding,
    project: ModelAgentBinding,
    project_raw: &serde_json::Value,
) {
    if json_field_present(project_raw, "args") {
        existing.args = project.args;
    }
    if json_field_present(project_raw, "autonomous_args") {
        existing.autonomous_args = project.autonomous_args;
    }
    if json_field_present(project_raw, "timeout") {
        existing.timeout = project.timeout;
    }
}

fn load_merged_settings_for_completion(plan_root: &Path) -> RheiSettings {
    // Shell completion must not fail because a project settings file is half-written.
    load_merged_settings(plan_root)
        .unwrap_or_else(|_| RheiSettings { agents: built_in_agents(), ..Default::default() })
}

/// Load merged settings: built-ins, then global, then project-level overrides.
fn load_merged_settings(plan_root: &Path) -> MietteResult<RheiSettings> {
    let global = match home_dir() {
        Ok(home) => load_settings_document(&home.join(".config/rhei/settings.json"))?,
        Err(_) => empty_settings_document(),
    };

    let project = load_settings_document(&plan_root.join(".rhei/settings.json"))?;
    let project_raw = &project.raw;
    let global = global.typed;
    let project = project.typed;

    // Agent registry: built-ins seed the map; global then project entries
    // replace an id wholesale when present.
    let mut agents = built_in_agents();
    for (id, profile) in global.agents {
        agents.insert(id, profile);
    }
    for (id, profile) in project.agents {
        agents.insert(id, profile);
    }

    // Registries merge by id: start with global, override by project.
    let mut mcp_servers = global.mcp_servers.clone();
    for (id, profile) in project.mcp_servers {
        mcp_servers.insert(id, profile);
    }
    let mut skills = global.skills.clone();
    for (id, profile) in project.skills {
        skills.insert(id, profile);
    }
    // `models` merge by model id; within a matching id, `models.<id>.agents`
    // is deep-merged by agent id.
    // §FS-rhei-agents.1.3: Merge models by id and model-agent bindings by agent id.
    let mut models = global.models.clone();
    for (id, project_profile) in project.models {
        let project_model_raw = json_child(json_child(project_raw, "models"), &id);
        match models.get_mut(&id) {
            Some(existing) => {
                if json_field_present(project_model_raw, "provider") {
                    existing.provider = project_profile.provider;
                }
                if json_field_present(project_model_raw, "model") {
                    existing.model = project_profile.model;
                }
                if json_field_present(project_model_raw, "default_agent") {
                    existing.default_agent = project_profile.default_agent;
                }
                for (agent_id, binding) in project_profile.agents {
                    let project_binding_raw =
                        json_child(json_child(project_model_raw, "agents"), &agent_id);
                    match existing.agents.get_mut(&agent_id) {
                        Some(existing_binding) => merge_model_agent_binding(
                            existing_binding,
                            binding,
                            project_binding_raw,
                        ),
                        None => {
                            existing.agents.insert(agent_id, binding);
                        }
                    }
                }
            }
            None => {
                models.insert(id, project_profile);
            }
        }
    }

    // `defaults.mcp_servers` / `defaults.skills`: project replaces global
    // wholesale when present (including an explicit empty list).
    let defaults = SettingsDefaults {
        model: if json_nested_field_present(project_raw, "defaults", "model") {
            project.defaults.model
        } else {
            global.defaults.model
        },
        agent: if json_nested_field_present(project_raw, "defaults", "agent") {
            project.defaults.agent
        } else {
            global.defaults.agent
        },
        agent_mode: if json_nested_field_present(project_raw, "defaults", "agent_mode") {
            project.defaults.agent_mode
        } else {
            global.defaults.agent_mode
        },
        agent_timeout: if json_nested_field_present(project_raw, "defaults", "agent_timeout") {
            project.defaults.agent_timeout
        } else {
            global.defaults.agent_timeout
        },
        program_timeout: if json_nested_field_present(project_raw, "defaults", "program_timeout") {
            project.defaults.program_timeout
        } else {
            global.defaults.program_timeout
        },
        mcp_servers: if json_nested_field_present(project_raw, "defaults", "mcp_servers") {
            project.defaults.mcp_servers
        } else {
            global.defaults.mcp_servers
        },
        skills: if json_nested_field_present(project_raw, "defaults", "skills") {
            project.defaults.skills
        } else {
            global.defaults.skills
        },
    };

    Ok(RheiSettings {
        agent: if json_field_present(project_raw, "agent") { project.agent } else { global.agent },
        agent_mode: if json_field_present(project_raw, "agent_mode") {
            project.agent_mode
        } else {
            global.agent_mode
        },
        model: if json_field_present(project_raw, "model") { project.model } else { global.model },
        agent_timeout: if json_field_present(project_raw, "agent_timeout") {
            project.agent_timeout
        } else {
            global.agent_timeout
        },
        program_timeout: if json_field_present(project_raw, "program_timeout") {
            project.program_timeout
        } else {
            global.program_timeout
        },
        defaults,
        agents,
        models,
        mcp_servers,
        skills,
        snapshots: if json_field_present(project_raw, "snapshots") && project.snapshots.is_none() {
            None
        } else {
            merge_snapshot_settings(global.snapshots, project.snapshots)
        },
    })
}

fn validate_machine_settings_references(
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
) -> Vec<String> {
    let mut errors = Vec::new();

    // Agent registry self-validation: `command` is required, and
    // `mcp_flag` and `mcp_config_flag` are mutually exclusive per
    // §FS-rhei-agents.1.1.2: Validate agent transport profile settings.
    for (id, profile) in &settings.agents {
        if profile.command.is_empty() {
            errors.push(format!(
                "agent '{}' has an empty 'command'; the `command` field is required",
                id
            ));
        }
        if profile.mcp_flag.is_some() && profile.mcp_config_flag.is_some() {
            errors.push(format!(
                "agent '{}' declares both 'mcp_flag' and 'mcp_config_flag'; \
                 they are mutually exclusive",
                id
            ));
        }
    }

    // MCP server registry self-validation: exactly one of `command`/`url`;
    // §FS-rhei-agents.1.1.4: Validate MCP server registry entries.
    for (id, profile) in &settings.mcp_servers {
        match (profile.command.is_some(), profile.url.is_some()) {
            (false, false) => errors.push(format!(
                "mcp_servers.'{}' must declare exactly one of 'command' or 'url'",
                id
            )),
            (true, true) => errors.push(format!(
                "mcp_servers.'{}' declares both 'command' and 'url'; they are \
                 mutually exclusive",
                id
            )),
            (false, true) => {
                if profile.transport.as_deref().map_or(true, str::is_empty) {
                    errors.push(format!(
                        "mcp_servers.'{}' uses 'url' but does not declare 'transport'; \
                         set transport to 'sse' or 'websocket'",
                        id
                    ));
                }
            }
            (true, false) => {}
        }
    }

    // Model registry self-validation: `provider` and `model` are required
    // §FS-rhei-agents.1.1.3: Validate model profile registry entries.
    for (id, profile) in &settings.models {
        if profile.provider.as_deref().map_or(true, str::is_empty) {
            errors.push(format!("models.'{}' is missing required field 'provider'", id));
        }
        if profile.model.as_deref().map_or(true, str::is_empty) {
            errors.push(format!("models.'{}' is missing required field 'model'", id));
        }
    }

    validate_mcp_entries_known(
        "defaults.mcp_servers",
        settings.defaults.mcp_servers.as_deref(),
        &settings.mcp_servers,
        &mut errors,
    );
    validate_skill_entries_known(
        "defaults.skills",
        settings.defaults.skills.as_deref(),
        &settings.skills,
        &mut errors,
    );

    for (state_name, state) in &machine.states {
        validate_mcp_entries_known(
            &format!("state '{state_name}' mcp_servers"),
            state.mcp_servers.as_deref(),
            &settings.mcp_servers,
            &mut errors,
        );
        validate_skill_entries_known(
            &format!("state '{state_name}' skills"),
            state.skills.as_deref(),
            &settings.skills,
            &mut errors,
        );

        if let Some(agent) = state.agent.as_ref() {
            let Some(profile) = settings.agents.get(agent.id()) else {
                errors.push(format!(
                    "state '{}' references unknown agent '{}'",
                    state_name,
                    agent.id()
                ));
                continue;
            };
            if let Some(mode) = state.agent_mode.as_deref() {
                if !profile.modes.is_empty() && !profile.modes.contains_key(mode) {
                    errors.push(format!(
                        "state '{}' references unknown mode '{}' for agent '{}'",
                        state_name,
                        mode,
                        agent.id()
                    ));
                }
            }
        }

        let selectors = state
            .target
            .iter()
            .cloned()
            .chain(state.all_targets.iter().cloned())
            .collect::<Vec<_>>();
        for selector in selectors {
            match parse_execution_target(&selector) {
                Ok(target) => {
                    let Some(profile) = settings.agents.get(target.agent.as_str()) else {
                        errors.push(format!(
                            "state '{}' references unknown target agent '{}' in '{}'",
                            state_name, target.agent, selector
                        ));
                        continue;
                    };
                    if let Some(mode) = target.mode.as_deref() {
                        if !profile.modes.contains_key(mode) {
                            errors.push(format!(
                                "state '{}' references unknown target mode '{}' for agent '{}' in '{}'",
                                state_name, mode, target.agent, selector
                            ));
                        }
                    }
                }
                Err(err) => errors.push(format!(
                    "state '{}' has invalid target selector '{}': {}",
                    state_name, selector, err
                )),
            }
        }

        if state.snapshot.as_ref().and_then(|snapshot| snapshot.emit.as_ref()).is_some()
            || state.snapshot.as_ref().and_then(|snapshot| snapshot.inherit.as_ref()).is_some()
        {
            // Settings-aware snapshot checks need the merged agent/model
            // registry, so they live in the CLI validation layer rather than
            // §FS-rhei-snapshots.9.2 §FS-rhei-snapshots.11: Registry-aware checks.
            match resolve_agent_invocations(machine, state_name, settings, &default_run_options()) {
                Ok(invocations) if invocations.is_empty() => {
                    errors.push(format!(
                        "state '{}' declares snapshot operations but no effective target tuple resolves (snapshot-requires-target)",
                        state_name
                    ));
                }
                Ok(invocations) => {
                    let mut seen_slugs: HashMap<String, String> = HashMap::new();
                    for invocation in &invocations {
                        let Some(slug) = resolved_agent_target_slug(invocation) else {
                            errors.push(format!(
                                "state '{}' declares snapshot operations but agent '{}' does not resolve provider and model (snapshot-requires-target)",
                                state_name,
                                invocation.agent.id()
                            ));
                            continue;
                        };
                        if let Some(previous) =
                            seen_slugs.insert(slug.clone(), invocation.agent.id().to_string())
                        {
                            errors.push(format!(
                                "state '{}' has multiple resolved invocations for agents '{}' and '{}' that normalize to snapshot target slug '{}'",
                                state_name,
                                previous,
                                invocation.agent.id(),
                                slug
                            ));
                        }
                        if state
                            .snapshot
                            .as_ref()
                            .and_then(|snapshot| snapshot.emit.as_ref())
                            .is_some()
                            && !profile_has_snapshot_layout(&invocation.profile.session)
                        {
                            errors.push(format!(
                                "state '{}' declares snapshot.emit but agent '{}' has no supported snapshot session layout (unsupported-snapshot-session)",
                                state_name,
                                invocation.agent.id()
                            ));
                        }
                        if state
                            .snapshot
                            .as_ref()
                            .and_then(|snapshot| snapshot.inherit.as_ref())
                            .is_some_and(|inherit| inherit.required == Some(true))
                            && !profile_has_snapshot_preload(&invocation.profile.session)
                        {
                            errors.push(format!(
                                "state '{}' declares required snapshot.inherit but agent '{}' has no supported snapshot preload strategy (unsupported-snapshot-session)",
                                state_name,
                                invocation.agent.id()
                            ));
                        }
                    }
                }
                Err(err) => errors.push(format!(
                    "state '{}' declares snapshot operations but no effective target tuple resolves: {} (snapshot-requires-target)",
                    state_name, err
                )),
            }
        }
    }

    errors
}

fn validate_mcp_entries_known(
    label: &str,
    entries: Option<&[StateMcpEntry]>,
    registry: &BTreeMap<String, McpServerProfile>,
    errors: &mut Vec<String>,
) {
    for entry in entries.unwrap_or(&[]) {
        if !entry.is_inline() && !registry.contains_key(entry.id()) {
            errors.push(format!("{label} references unknown mcp server '{}'", entry.id()));
        }
    }
}

fn validate_skill_entries_known(
    label: &str,
    entries: Option<&[StateSkillEntry]>,
    registry: &BTreeMap<String, SkillProfile>,
    errors: &mut Vec<String>,
) {
    for entry in entries.unwrap_or(&[]) {
        if !entry.is_inline() && !registry.contains_key(entry.id()) {
            errors.push(format!("{label} references unknown skill '{}'", entry.id()));
        }
    }
}

fn validate_snapshot_plan_context(
    loaded: &LoadedPlan,
    machine: &rhei_validator::StateMachine,
) -> Vec<String> {
    let mut errors = Vec::new();
    for task in &loaded.rhei.tasks {
        let state_name = normalized_state_name(task.state.as_str(), machine);
        if machine
            .states
            .get(&state_name)
            .and_then(|state| state.snapshot.as_ref())
            .and_then(|snapshot| snapshot.inherit.as_ref())
            .and_then(|inherit| inherit.from_axis.as_deref())
            == Some("ancestor")
        {
            errors.push(format!(
                "Task {} is a root task in state '{}' but that state declares snapshot.inherit.from: ancestor (snapshot root tasks have no ancestor)",
                task.id, state_name
            ));
        }
    }
    errors
}

fn snapshot_orphan_validation_warnings(
    workspace_root: &Path,
    loaded: &LoadedPlan,
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
) -> MietteResult<Vec<String>> {
    let cache_root = snapshot_cache_dir(settings, workspace_root);
    if !cache_root.exists() {
        return Ok(Vec::new());
    }
    let records = read_snapshot_records(&cache_root)?;
    let mut warnings = Vec::new();
    for record in records {
        if snapshot_record_is_orphaned_for_loaded(&record, loaded, machine, settings) {
            warnings.push(format!(
                "snapshot {} is orphaned relative to the current plan/state machine",
                record.display_ref()
            ));
        }
    }
    Ok(warnings)
}

fn snapshot_record_is_orphaned_for_loaded(
    record: &SnapshotRecord,
    loaded: &LoadedPlan,
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
) -> bool {
    let task_exists =
        flatten_tasks(&loaded.rhei).into_iter().any(|task| task.id.to_string() == record.task_id);
    if !task_exists {
        return true;
    }
    if !machine.states.contains_key(&record.emitting_state) {
        return true;
    }
    let Ok(slugs) = effective_target_slugs_for_state(machine, &record.emitting_state, settings)
    else {
        return true;
    };
    slugs.is_empty() || !slugs.contains(&record.target_slug)
}

fn profile_has_snapshot_layout(session: &Option<serde_json::Value>) -> bool {
    session.as_ref().is_some_and(snapshot_emit_session_supported)
}

fn profile_has_snapshot_preload(session: &Option<serde_json::Value>) -> bool {
    session.as_ref().is_some_and(snapshot_preload_session_supported)
}
