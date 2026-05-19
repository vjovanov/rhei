fn completion_plan_path() -> Option<PathBuf> {
    let words = completion_words();
    let command = completion_command_name(&words)?;
    first_command_positional(&words, &command).map(PathBuf::from)
}

fn completion_command_name(words: &[String]) -> Option<String> {
    let mut expect_value = false;
    for word in words.iter().skip(1) {
        if word.is_empty() {
            break;
        }
        if expect_value {
            expect_value = false;
            continue;
        }
        if let Some(option) = word.strip_prefix("--") {
            if option.split_once('=').is_none() && option == "state-machine" {
                expect_value = true;
            }
            continue;
        }
        return Some(word.clone());
    }
    None
}

fn first_command_positional(words: &[String], command: &str) -> Option<String> {
    let command_index = words.iter().position(|word| word == command)?;
    let mut expect_value_for: Option<&str> = None;
    for word in words.iter().skip(command_index + 1) {
        if word.is_empty() {
            break;
        }
        if let Some(option) = expect_value_for.take() {
            if option != "set" && option != "set-file" && option != "values" && option != "output" {
                continue;
            }
            continue;
        }
        if let Some(option) = word.strip_prefix("--") {
            if let Some((_, _)) = option.split_once('=') {
                continue;
            }
            if matches!(
                option,
                "task"
                    | "from"
                    | "to"
                    | "result"
                    | "set"
                    | "set-file"
                    | "values"
                    | "output"
                    | "agent"
                    | "agent-mode"
                    | "model"
                    | "program-timeout"
                    | "parallel"
                    | "state-machine"
                    | "state"
                    | "assignee"
                    | "kind"
                    | "has-prior"
                    | "parent"
                    | "contains"
                    | "limit"
            ) {
                expect_value_for = Some(option);
            }
            continue;
        }
        return Some(word.clone());
    }
    None
}

fn completion_option_value(name: &str) -> Option<String> {
    let words = completion_words();
    let flag = format!("--{name}");
    let prefix = format!("--{name}=");
    let mut iter = words.iter().peekable();
    while let Some(word) = iter.next() {
        if let Some(value) = word.strip_prefix(&prefix) {
            return Some(value.to_string());
        }
        if word == &flag {
            return iter.peek().filter(|value| !value.is_empty()).map(|value| (*value).clone());
        }
    }
    None
}

fn completion_words() -> Vec<String> {
    let args = std::env::args_os().collect::<Vec<_>>();
    let start = args.iter().position(|arg| arg == "--").map(|idx| idx + 1).unwrap_or(1);
    args.into_iter().skip(start).map(|arg| arg.to_string_lossy().to_string()).collect()
}

fn flatten_tasks(rhei: &rhei_core::ast::Rhei) -> Vec<&rhei_core::ast::Task> {
    fn collect<'a>(task: &'a rhei_core::ast::Task, tasks: &mut Vec<&'a rhei_core::ast::Task>) {
        tasks.push(task);
        for child in &task.children {
            collect(child, tasks);
        }
    }

    let mut tasks = Vec::new();
    for task in &rhei.tasks {
        collect(task, &mut tasks);
    }
    tasks
}

fn current_task_state(plan: &Path, task_id: &str) -> MietteResult<String> {
    let loaded = load_plan(plan)?;
    flatten_tasks(&loaded.rhei)
        .into_iter()
        .find(|task| task.id.to_string() == task_id)
        .map(|task| task.state.clone())
        .ok_or_else(|| miette!("task '{}' not found in {}", task_id, plan.display()))
}

fn xdg_data_home() -> MietteResult<PathBuf> {
    match std::env::var_os("XDG_DATA_HOME") {
        Some(path) if !path.is_empty() => Ok(PathBuf::from(path)),
        _ => Ok(home_dir()?.join(".local/share")),
    }
}

fn xdg_config_home() -> MietteResult<PathBuf> {
    match std::env::var_os("XDG_CONFIG_HOME") {
        Some(path) if !path.is_empty() => Ok(PathBuf::from(path)),
        _ => Ok(home_dir()?.join(".config")),
    }
}

/// Load a [`rhei_validator::StateMachine`] from the user-provided path, or fall back to the
/// built-in default when no path was given.
fn load_state_machine(path: Option<&Path>) -> MietteResult<rhei_validator::StateMachine> {
    match path {
        Some(p) => rhei_validator::StateMachine::from_yaml_file(p)
            .map_err(|err| file_io_report(p, "failed to load states", err)),
        None => Ok(rhei_validator::StateMachine::builtin_default()),
    }
}

struct ResolvedStateMachine {
    machine: rhei_validator::StateMachine,
    path: Option<PathBuf>,
}

fn auto_state_machine_path(input: &Path) -> PathBuf {
    if workspace::is_workspace(input) {
        input.join("states.yaml")
    } else {
        input.parent().unwrap_or_else(|| Path::new(".")).join("states.yaml")
    }
}

/// If `input` references a Directory Workspace via its inner `index.rhei.md`
/// file, return the workspace root directory; otherwise return `input`
/// unchanged. This lets command handlers continue to use the existing
/// `workspace::is_workspace(input)` + `input.join(...)` pattern regardless of
/// which form the user supplied on the command line.
fn normalize_workspace_input(input: &Path) -> PathBuf {
    workspace::workspace_dir(input).unwrap_or_else(|| input.to_path_buf())
}

fn resolve_state_machine_for_loaded_plan(
    input: &Path,
    loaded: &LoadedPlan,
    state_machine_path: Option<&Path>,
) -> MietteResult<ResolvedStateMachine> {
    if let Some(path) = state_machine_path {
        return Ok(ResolvedStateMachine {
            machine: load_state_machine(Some(path))?,
            path: Some(path.to_path_buf()),
        });
    }

    let builtin = rhei_validator::StateMachine::builtin_default();
    let declared_name = loaded.rhei.states.trim();
    let candidate = auto_state_machine_path(input);

    if candidate.is_file() {
        let machine = load_state_machine(Some(&candidate))?;
        if machine.name == declared_name {
            return Ok(ResolvedStateMachine { machine, path: Some(candidate) });
        }

        if declared_name != builtin.name {
            return Err(miette!(
                "plan declares state machine '{}', but auto-discovered states file '{}' declares '{}'",
                declared_name,
                candidate.display(),
                machine.name
            ));
        }
    }

    if declared_name != builtin.name {
        return Err(miette!(
            "plan declares state machine '{}', but no auto-discovered states file was found at '{}'.\nUse --state-machine <path> to override the default location.",
            declared_name,
            candidate.display()
        ));
    }

    Ok(ResolvedStateMachine { machine: builtin, path: None })
}

/// Human-readable label for the state machine source, used in diagnostics.
fn state_machine_label(path: Option<&Path>) -> String {
    match path {
        Some(p) => format!("'{}'", p.display()),
        None => "the built-in default state machine".to_string(),
    }
}

/// Execute the `states` subcommand: load the configured state machine and
/// print its states and declared transitions.
fn states_command(state_machine: Option<&Path>, as_json: bool) -> MietteResult<()> {
    let machine = load_state_machine(state_machine)?;

    if as_json {
        let rendered = render_state_machine_json(&machine)
            .map_err(|err| miette!("failed to serialize state machine: {err}"))?;
        println!("{rendered}");
    } else {
        println!("{}", render_state_machine_text(&machine));
    }

    Ok(())
}

/// Filter set for the `list` subcommand. See `Commands::List` for flag docs.
struct ListFilters {
    states: Vec<String>,
    assignee: Option<String>,
    no_assignee: bool,
    kind: Option<String>,
    has_prior: Option<String>,
    parent: Option<String>,
    root: bool,
    contains: Option<String>,
    terminal: bool,
    non_terminal: bool,
    ready: bool,
    blocked: bool,
    limit: usize,
}

/// Execute the `list` subcommand: load a plan and print tasks matching the
/// provided filters. Modeled after `bd list` from beads, with a filter set
/// adapted to Rhei's data model (no priority/labels/timestamps).
fn list_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    filters: ListFilters,
    as_json: bool,
) -> MietteResult<()> {
    let loaded = load_plan(input)?;
    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machine = resolved.machine;

    // Flatten the task tree into (task, parent_id) pairs, preserving source order.
    let mut flat: Vec<(&rhei_core::ast::Task, Option<TaskId>)> = Vec::new();
    fn walk<'a>(
        task: &'a rhei_core::ast::Task,
        parent: Option<TaskId>,
        out: &mut Vec<(&'a rhei_core::ast::Task, Option<TaskId>)>,
    ) {
        out.push((task, parent));
        let parent_id = Some(task.id.clone());
        for child in &task.children {
            walk(child, parent_id.clone(), out);
        }
    }
    for task in &loaded.rhei.tasks {
        walk(task, None, &mut flat);
    }

    // Pre-compute state map for ready/blocked checks (only top-level tasks
    // declare priors, but checking the full flat set is harmless).
    let state_map: HashMap<&TaskId, String> = flat
        .iter()
        .map(|(t, _)| (&t.id, normalized_state_name(t.state.as_str(), &machine)))
        .collect();

    let priors_satisfied = |task: &rhei_core::ast::Task| -> bool {
        task.prior.iter().all(|dep| {
            state_map.get(dep).map(|s| dependency_is_satisfied(s, &machine)).unwrap_or(false)
        })
    };

    // Normalize state filter values once so users can pass either canonical
    // names or aliases declared in the state machine.
    let state_filter: Vec<String> =
        filters.states.iter().map(|s| normalized_state_name(s.as_str(), &machine)).collect();
    let parent_filter = filters.parent.as_deref().map(parse_task_id);
    let has_prior_filter = filters.has_prior.as_deref().map(parse_task_id);
    let contains_lower = filters.contains.as_deref().map(|s| s.to_lowercase());

    let mut matches: Vec<&(&rhei_core::ast::Task, Option<TaskId>)> = Vec::new();
    for entry in &flat {
        let (task, parent_id) = entry;

        if !state_filter.is_empty() {
            let task_state = normalized_state_name(task.state.as_str(), &machine);
            if !state_filter.iter().any(|s| s == &task_state) {
                continue;
            }
        }

        if let Some(want) = filters.assignee.as_deref() {
            if task.assignee.as_deref() != Some(want) {
                continue;
            }
        }
        if filters.no_assignee && task.assignee.is_some() {
            continue;
        }

        if let Some(want) = filters.kind.as_deref() {
            if !task.kind.eq_ignore_ascii_case(want) {
                continue;
            }
        }

        if let Some(prior_id) = &has_prior_filter {
            if !task.prior.iter().any(|p| p == prior_id) {
                continue;
            }
        }

        if let Some(parent_id_filter) = &parent_filter {
            if parent_id.as_ref() != Some(parent_id_filter) {
                continue;
            }
        }
        if filters.root && parent_id.is_some() {
            continue;
        }

        if let Some(needle) = &contains_lower {
            let title_hit = task.title.to_lowercase().contains(needle);
            let body_hit = task.content.to_lowercase().contains(needle);
            if !title_hit && !body_hit {
                continue;
            }
        }

        let is_terminal = is_terminal_state(task.state.as_str(), &machine);
        if filters.terminal && !is_terminal {
            continue;
        }
        if filters.non_terminal && is_terminal {
            continue;
        }

        if filters.ready || filters.blocked {
            let normalized = normalized_state_name(task.state.as_str(), &machine);
            let is_gating = machine.states.get(&normalized).map(|def| def.gating).unwrap_or(false);
            let satisfied = priors_satisfied(task);
            let task_ready = !is_terminal && !is_gating && satisfied;
            if filters.ready && !task_ready {
                continue;
            }
            if filters.blocked && (is_terminal || satisfied) {
                continue;
            }
        }

        matches.push(entry);
    }

    if filters.limit > 0 && matches.len() > filters.limit {
        matches.truncate(filters.limit);
    }

    if as_json {
        let payload: Vec<serde_json::Value> = matches
            .iter()
            .map(|(task, parent_id)| {
                serde_json::json!({
                    "id": task.id.to_string(),
                    "kind": task.kind,
                    "title": task.title,
                    "state": task.state,
                    "assignee": task.assignee,
                    "prior": task.prior.iter().map(TaskId::to_string).collect::<Vec<_>>(),
                    "parent": parent_id.as_ref().map(TaskId::to_string),
                    "depth": task.id.depth(),
                })
            })
            .collect();
        let rendered = serde_json::to_string_pretty(&payload)
            .map_err(|err| miette!("failed to serialize task list: {err}"))?;
        println!("{rendered}");
        return Ok(());
    }

    if matches.is_empty() {
        println!("(no tasks match the given filters)");
        return Ok(());
    }

    for (task, _) in &matches {
        let indent = "  ".repeat(task.id.depth().saturating_sub(1));
        let mut line = format!(
            "{}{} {}: {} [{}]",
            indent,
            title_case_kind(&task.kind),
            task.id,
            task.title,
            task.state
        );
        if !task.prior.is_empty() {
            let priors: Vec<String> = task.prior.iter().map(TaskId::to_string).collect();
            line.push_str(&format!(" (prior: {})", priors.join(", ")));
        }
        if let Some(assignee) = &task.assignee {
            line.push_str(&format!(" @{}", assignee));
        }
        println!("{line}");
    }

    Ok(())
}
