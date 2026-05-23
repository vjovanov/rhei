fn render_state_machine_text(machine: &rhei_validator::StateMachine) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "State machine: {} (version: {})\n",
        machine.name,
        format_version(&machine.version)
    ));
    if !machine.models.is_empty() {
        out.push_str(&format!("Models: {}\n", machine.models.join(", ")));
    }
    if let Some(profiles) = machine.profiles.as_ref() {
        out.push_str("Profiles:\n");
        for (name, profile) in profiles {
            out.push_str(&format!(
                "  {name}: initial={}, allowed=[{}]\n",
                profile.initial,
                profile.allowed.join(", ")
            ));
        }
    }
    if let Some(policy) = machine.node_policy.as_ref() {
        out.push_str(&format!(
            "Node policy: root={}, default={}\n",
            policy.root, policy.default
        ));
    }

    out.push_str("\nStates:\n");
    if machine.states.is_empty() {
        out.push_str("  (none defined)\n");
    } else {
        for (idx, (name, def)) in machine.states.iter().enumerate() {
            if idx > 0 {
                out.push('\n');
            }
            let mut flags = Vec::new();
            if def.terminal {
                flags.push("final");
            }
            if def.gating {
                flags.push("gating");
            }
            if def.concurrent {
                flags.push("concurrent");
            }
            let flag_suffix =
                if flags.is_empty() { String::new() } else { format!(" [{}]", flags.join(", ")) };
            let description = def.description.as_deref().unwrap_or("");
            out.push_str(&format!("  {name}{flag_suffix}"));
            if !description.is_empty() {
                out.push_str(&format!(" — {description}"));
            }
            out.push('\n');
            if let Some(visits) = def.visits {
                out.push_str(&format!("      Visits: {visits}\n"));
            }
            if let Some(poll) = def.poll.as_ref() {
                out.push_str(&format!(
                    "      Poll: interval={}, max_attempts={}\n",
                    poll.interval, poll.max_attempts
                ));
            }
            if let Some(target) = def.target.as_deref() {
                out.push_str(&format!("      Target: {target}\n"));
            }
            if !def.all_targets.is_empty() {
                out.push_str(&format!("      Targets: {}\n", def.all_targets.join(", ")));
            }
            if !def.all_models.is_empty() {
                out.push_str(&format!("      Models: {}\n", def.all_models.join(", ")));
            } else if let Some(model) = def.model.as_deref() {
                out.push_str(&format!("      Model: {model}\n"));
            }
            if let Some(agent) = def.agent.as_ref() {
                out.push_str(&format!("      Agent: {}\n", agent.id()));
            }
            if let Some(mode) = def.agent_mode.as_deref() {
                out.push_str(&format!("      Agent mode: {mode}\n"));
            }
            if let Some(timeout) = def.agent_timeout.as_deref() {
                out.push_str(&format!("      Agent timeout: {timeout}\n"));
            }
            if def.program.is_some() {
                out.push_str("      Program: configured\n");
            }
            if let Some(timeout) = def.program_timeout.as_deref() {
                out.push_str(&format!("      Program timeout: {timeout}\n"));
            }
            if let Some(mcp_servers) = def.mcp_servers.as_ref() {
                let ids = mcp_servers.iter().map(|entry| entry.id()).collect::<Vec<_>>();
                out.push_str(&format!("      MCP servers: {}\n", ids.join(", ")));
            }
            if let Some(skills) = def.skills.as_ref() {
                let ids = skills.iter().map(|entry| entry.id()).collect::<Vec<_>>();
                out.push_str(&format!("      Skills: {}\n", ids.join(", ")));
            }
            if def.snapshot.is_some() {
                out.push_str("      Snapshot: configured\n");
            }
            if !def.inputs.is_empty() {
                out.push_str("      Inputs:\n");
                for artifact in &def.inputs {
                    out.push_str(&format!("        - {}: {}\n", artifact.name, artifact.path));
                }
            }
            if !def.outputs.is_empty() {
                out.push_str("      Outputs:\n");
                for artifact in &def.outputs {
                    out.push_str(&format!("        - {}: {}\n", artifact.name, artifact.path));
                }
            }
            if let Some(personality) =
                def.personality.as_deref().map(str::trim).filter(|s| !s.is_empty())
            {
                out.push_str(&format!("      Personality: {personality}\n"));
            }
            if let Some(instructions) = def.instructions.as_deref() {
                for line in instructions.lines() {
                    out.push_str(&format!("      {line}\n"));
                }
            }
        }
    }

    out.push_str("\nTransitions:\n");
    if machine.transitions.is_empty() {
        out.push_str("  (none declared)\n");
    } else {
        for rule in &machine.transitions {
            out.push_str(&format!("  {} -> {}", rule.from.0, rule.to.0));
            let mut annotations = Vec::new();
            if let Some(cb) = rule.on_leave.as_ref() {
                annotations.push(format!("on_leave={}", cb.0));
            }
            if let Some(cb) = rule.on_enter.as_ref() {
                annotations.push(format!("on_enter={}", cb.0));
            }
            if let Some(cond) = rule.condition.as_ref() {
                annotations.push(format!("when={cond}"));
            }
            if let Some(t) = rule.timeout.as_ref() {
                annotations.push(format!("timeout={t}"));
            }
            if !annotations.is_empty() {
                out.push_str(&format!(" ({})", annotations.join(", ")));
            }
            out.push('\n');
        }
    }

    out
}

fn render_state_machine_json(machine: &rhei_validator::StateMachine) -> Result<String> {
    let states: Vec<serde_json::Value> = machine
        .states
        .iter()
        .map(|(name, def)| {
            serde_json::json!({
                "name": name,
                "description": &def.description,
                "instructions": &def.instructions,
                "personality": &def.personality,
                "final": def.terminal,
                "gating": def.gating,
                "concurrent": def.concurrent,
                "poll": &def.poll,
                "visits": def.visits,
                "target": &def.target,
                "all_targets": &def.all_targets,
                "all_models": &def.all_models,
                "model": &def.model,
                "agent": def.agent.as_ref().map(|agent| agent.id()),
                "agent_mode": &def.agent_mode,
                "agent_timeout": &def.agent_timeout,
                "program": &def.program,
                "program_timeout": &def.program_timeout,
                "mcp_servers": &def.mcp_servers,
                "skills": &def.skills,
                "snapshot": &def.snapshot,
                "inputs": &def.inputs,
                "outputs": &def.outputs,
            })
        })
        .collect();

    let transitions =
        serde_json::to_value(&machine.transitions).context("serialize transitions")?;
    let version =
        serde_json::to_value(&machine.version).context("serialize state machine version")?;

    let payload = serde_json::json!({
        "name": machine.name,
        "models": &machine.models,
        "profiles": &machine.profiles,
        "node_policy": &machine.node_policy,
        "version": version,
        "states": states,
        "transitions": transitions,
    });

    serde_json::to_string_pretty(&payload).context("render state machine as JSON")
}

fn format_version(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::String(s) => s.clone(),
        other => serde_yaml::to_string(other)
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "unknown".to_string()),
    }
}

/// Read the markdown plan source file from disk.
fn read_input_file(path: &Path) -> MietteResult<String> {
    fs::read_to_string(path).map_err(|err| file_io_report(path, "failed to read input file", err))
}

/// A loaded plan with optional workspace task-to-file mapping.
struct LoadedPlan {
    rhei: rhei_core::ast::Rhei,
    /// For directory workspaces: maps task ID string → source file path.
    /// Empty for single-file plans.
    task_sources: HashMap<String, PathBuf>,
}

impl LoadedPlan {
    /// Return the file path that contains the given task.
    /// For single-file plans, returns `fallback` (the plan file itself).
    fn task_file(&self, task_id: &str, fallback: &Path) -> PathBuf {
        self.task_sources.get(task_id).cloned().unwrap_or_else(|| fallback.to_path_buf())
    }
}

/// Load a plan from a file or directory workspace.
fn load_plan(path: &Path) -> MietteResult<LoadedPlan> {
    if let Some(ws_dir) = workspace::workspace_dir(path) {
        let ws = workspace::load_workspace(&ws_dir).map_err(|err| miette!("{}", err.message))?;
        Ok(LoadedPlan { rhei: ws.rhei, task_sources: ws.task_sources })
    } else {
        let input = read_input_file(path)?;
        let rhei = rhei_core::parse(&input).map_err(|err| parse_report(path, &input, &err))?;
        Ok(LoadedPlan { rhei, task_sources: HashMap::new() })
    }
}

/// Load a plan for `rhei validate`, collecting recoverable parse errors where
/// validation promises batch diagnostics.
fn load_plan_for_validation(path: &Path) -> MietteResult<LoadedPlan> {
    if let Some(ws_dir) = workspace::workspace_dir(path) {
        return load_workspace_for_validation(&ws_dir);
    }

    let raw = read_input_file(path)?;
    let (maybe_rhei, errs) = rhei_core::parser::parse_collect(&raw);
    match (maybe_rhei, errs.is_empty()) {
        (Some(rhei), true) => Ok(LoadedPlan { rhei, task_sources: HashMap::new() }),
        (_, false) | (None, _) => Err(parse_errors_report(path, &raw, &errs)),
    }
}

fn load_workspace_for_validation(ws_dir: &Path) -> MietteResult<LoadedPlan> {
    let index_path = ws_dir.join("index.rhei.md");
    let index_raw = read_input_file(&index_path)?;
    let index = rhei_core::parser::parse_workspace_index(&index_raw)
        .map_err(|err| parse_report(&index_path, &index_raw, &err))?;

    let tasks_dir = ws_dir.join("tasks");
    let mut all_tasks = Vec::new();
    let mut task_sources = HashMap::new();
    let mut parse_error_groups = Vec::new();
    let mut duplicate_task_error: Option<String> = None;

    if tasks_dir.is_dir() {
        let task_files = workspace::discover_task_files(&tasks_dir)
            .map_err(|err| miette!("{}", err.message))?;

        for path in task_files {
            let raw = read_input_file(&path)?;
            let (maybe_tasks, errors) =
                rhei_core::parser::parse_workspace_tasks_collect_with_structure(
                    &raw,
                    &index.structure,
                );
            if !errors.is_empty() {
                parse_error_groups.push(ParseErrorGroup { path, input: raw, errors });
                continue;
            }
            let Some(tasks) = maybe_tasks else {
                continue;
            };
            for task in &tasks {
                if duplicate_task_error.is_none() {
                    if let Err(err) =
                        collect_workspace_task_sources(task, &path, &mut task_sources)
                    {
                        duplicate_task_error = Some(err.message);
                    }
                }
            }
            all_tasks.extend(tasks);
        }
    }

    if !parse_error_groups.is_empty() {
        return Err(workspace_parse_errors_report(&parse_error_groups));
    }
    if let Some(error) = duplicate_task_error {
        return Err(miette!("{error}"));
    }

    if all_tasks.is_empty() {
        return Err(miette!("workspace contains no tasks (tasks/ directory is empty or missing)"));
    }

    Ok(LoadedPlan {
        rhei: rhei_core::ast::Rhei {
            title: index.title,
            states: index.states,
            states_declared: index.states_declared,
            structure: index.structure,
            metadata: index.metadata,
            content_sections: index.content_sections,
            tasks: all_tasks,
        },
        task_sources,
    })
}

fn collect_workspace_task_sources(
    task: &rhei_core::ast::Task,
    path: &Path,
    task_sources: &mut HashMap<String, PathBuf>,
) -> rhei_core::parser::Result<()> {
    let id = task.id.to_string();
    if let Some(existing) = task_sources.get(&id) {
        return Err(rhei_core::parser::ParseError::new(
            format!(
                "duplicate task ID '{}': defined in both {} and {}",
                id,
                existing.display(),
                path.display()
            ),
            None,
        ));
    }
    task_sources.insert(id, path.to_path_buf());

    for child in &task.children {
        collect_workspace_task_sources(child, path, task_sources)?;
    }

    Ok(())
}

/// Read and parse a markdown plan file into a [`rhei_core::ast::Rhei`].
fn parse_input_file(path: &Path) -> MietteResult<rhei_core::ast::Rhei> {
    Ok(load_plan(path)?.rhei)
}

/// Execute the `validate` subcommand once or in watch mode.
fn validate_command(input: &Path, state_machine: Option<&Path>, watch: bool) -> MietteResult<()> {
    if watch {
        watch_validation_command(input, state_machine)
    } else {
        run_validation_once(input, state_machine)
    }
}

/// Parse a plan, load the selected states, and print validation results.
fn run_validation_once(input: &Path, state_machine: Option<&Path>) -> MietteResult<()> {
    let loaded = load_plan_for_validation(input)?;

    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine)?;
    let base_path = input.parent().unwrap_or(Path::new("."));
    let mut report =
        rhei_validator::validate_with_machine_and_base(&loaded.rhei, &resolved.machine, base_path);
    let workspace_root = execution_workspace_root(input);
    let settings = load_merged_settings(&workspace_root)?;
    report.errors.extend(validate_machine_settings_references(&resolved.machine, &settings));
    report.errors.extend(validate_snapshot_plan_context(&loaded, &resolved.machine));
    report.warnings.extend(snapshot_orphan_validation_warnings(
        &workspace_root,
        &loaded,
        &resolved.machine,
        &settings,
    )?);

    if report.has_errors() {
        return Err(validation_report(input, resolved.path.as_deref(), &report.errors));
    }

    print_validation_report(&report.warnings);

    Ok(())
}

/// Print success output and any non-fatal validation warnings.
fn print_validation_report(warnings: &[String]) {
    println!("Validation succeeded");
    for warning in warnings {
        println!("warning: {warning}");
    }
}

/// Watch the plan and states files and re-run validation on relevant changes.
fn watch_validation_command(input: &Path, state_machine: Option<&Path>) -> MietteResult<()> {
    let watch_plan = validation_watch_plan(input, state_machine);

    println!(
        "Watch mode started for '{}' (states: {})",
        input.display(),
        watch_plan.state_machine_label,
    );

    let (tx, rx) = mpsc::channel();
    let mut watcher: RecommendedWatcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        Config::default(),
    )
    .map_err(|err| miette!("failed to initialize file watcher: {err}"))?;

    for root in &watch_plan.roots {
        watcher
            .watch(&root.path, root.mode)
            .map_err(|err| miette!("failed to watch '{}': {err}", root.path.display()))?;
    }

    run_validation_pass(input, state_machine);

    loop {
        let event = match rx.recv() {
            Ok(Ok(event)) => event,
            Ok(Err(err)) => {
                eprintln!("watch error: {err}");
                continue;
            }
            Err(err) => return Err(miette!("watch channel disconnected: {err}")),
        };

        if !should_revalidate(&event, &watch_plan.targets) {
            continue;
        }

        while debounce_has_relevant_event(&rx, &watch_plan.targets) {}

        println!("--- change detected, revalidating ---");
        run_validation_pass(input, state_machine);
    }
}

/// Run one validation pass in watch mode, writing any failure to stderr.
fn run_validation_pass(input: &Path, state_machine: Option<&Path>) {
    if let Err(err) = run_validation_once(input, state_machine) {
        eprintln!("{err:?}");
    }
}

fn debounce_has_relevant_event(
    rx: &mpsc::Receiver<notify::Result<Event>>,
    targets: &[WatchTarget],
) -> bool {
    match rx.recv_timeout(Duration::from_millis(250)) {
        Ok(Ok(event)) => should_revalidate(&event, targets),
        Ok(Err(err)) => {
            eprintln!("watch error: {err}");
            false
        }
        Err(RecvTimeoutError::Timeout) => false,
        Err(RecvTimeoutError::Disconnected) => false,
    }
}

fn should_revalidate(event: &Event, targets: &[WatchTarget]) -> bool {
    if !is_relevant_event_kind(&event.kind) {
        return false;
    }

    event.paths.iter().any(|path| path_matches(path, targets))
}

fn is_relevant_event_kind(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) | EventKind::Any
    )
}

fn path_matches(path: &Path, targets: &[WatchTarget]) -> bool {
    targets.iter().any(|target| match target {
        WatchTarget::Exact(watched) => paths_equivalent(path, watched),
        WatchTarget::Descendant(root) => path_is_under(path, root),
    })
}

fn paths_equivalent(candidate: &Path, watched: &Path) -> bool {
    match (normalize_path(candidate), normalize_path(watched)) {
        (Some(candidate), Some(watched)) => return candidate == watched,
        (Some(candidate), None) if candidate == watched => return true,
        (None, Some(watched)) if candidate == watched => return true,
        (None, None) => {}
        _ => {}
    }

    let candidate_file_name = candidate.file_name();
    let watched_file_name = watched.file_name();

    candidate_file_name.is_some()
        && candidate_file_name == watched_file_name
        && candidate.components().last() == watched.components().last()
}

fn path_is_under(candidate: &Path, root: &Path) -> bool {
    match (normalize_path(candidate), normalize_path(root)) {
        (Some(candidate), Some(root)) => candidate.starts_with(root),
        _ => candidate.starts_with(root),
    }
}

#[derive(Debug, Clone)]
struct ValidationWatchPlan {
    targets: Vec<WatchTarget>,
    roots: Vec<WatchRoot>,
    state_machine_label: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum WatchTarget {
    Exact(PathBuf),
    Descendant(PathBuf),
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct WatchRoot {
    path: PathBuf,
    mode: RecursiveMode,
}

fn validation_watch_plan(input: &Path, state_machine: Option<&Path>) -> ValidationWatchPlan {
    let state_machine_path = state_machine.map(Path::to_path_buf).or_else(|| {
        let candidate = watch_auto_state_machine_path(input);
        if candidate.is_file() { Some(candidate) } else { None }
    });
    let state_machine_label = state_machine_label(state_machine_path.as_deref());

    let mut targets = plan_watch_targets(input);
    if let Some(path) = state_machine {
        targets.push(WatchTarget::Exact(canonical_watch_path(path)));
    } else {
        targets.push(WatchTarget::Exact(canonical_watch_path(&watch_auto_state_machine_path(input))));
    }

    let mut roots = Vec::new();
    for target in &targets {
        add_watch_root_for_target(&mut roots, target);
    }

    ValidationWatchPlan { targets, roots, state_machine_label }
}

fn plan_watch_targets(input: &Path) -> Vec<WatchTarget> {
    if let Some(workspace_root) = workspace::workspace_dir(input) {
        return workspace_watch_targets(&workspace_root);
    }

    if input.is_dir() {
        workspace_watch_targets(input)
    } else {
        vec![WatchTarget::Exact(canonical_watch_path(input))]
    }
}

fn workspace_watch_targets(workspace_root: &Path) -> Vec<WatchTarget> {
    vec![
        WatchTarget::Exact(canonical_watch_path(&workspace_root.join("index.rhei.md"))),
        WatchTarget::Descendant(canonical_watch_path(&workspace_root.join("tasks"))),
    ]
}

fn watch_auto_state_machine_path(input: &Path) -> PathBuf {
    if let Some(workspace_root) = workspace::workspace_dir(input) {
        workspace_root.join("states.yaml")
    } else if input.is_dir() {
        input.join("states.yaml")
    } else {
        input.parent().unwrap_or_else(|| Path::new(".")).join("states.yaml")
    }
}

#[cfg(test)]
fn canonical_watched_paths(input: &Path, state_machine: &Path) -> Vec<WatchTarget> {
    let mut targets = plan_watch_targets(input);
    targets.push(WatchTarget::Exact(canonical_watch_path(state_machine)));
    targets
}

fn add_watch_root_for_target(roots: &mut Vec<WatchRoot>, target: &WatchTarget) {
    let (path, mode) = match target {
        WatchTarget::Exact(path) => {
            let root = path.parent().unwrap_or_else(|| Path::new("."));
            (canonical_watch_path(root), RecursiveMode::NonRecursive)
        }
        WatchTarget::Descendant(path) => {
            if path.is_dir() {
                (canonical_watch_path(path), RecursiveMode::Recursive)
            } else {
                let root = path.parent().unwrap_or_else(|| Path::new("."));
                (canonical_watch_path(root), RecursiveMode::Recursive)
            }
        }
    };

    let root = WatchRoot { path, mode };
    if !roots.iter().any(|existing| existing == &root) {
        roots.push(root);
    }
}

fn canonical_watch_path(path: &Path) -> PathBuf {
    if let Some(normalized) = normalize_path(path) {
        return normalized;
    }

    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(path)
    };

    let Some(parent) = absolute.parent() else {
        return absolute;
    };
    let Some(normalized_parent) = normalize_path(parent) else {
        return absolute;
    };

    absolute
        .file_name()
        .map(|name| normalized_parent.join(name))
        .unwrap_or(normalized_parent)
}

fn normalize_path(path: &Path) -> Option<PathBuf> {
    path.canonicalize().ok()
}
