fn completion_plan_path() -> Option<PathBuf> {
    let words = completion_words();
    let command = completion_command_name(&words)?;
    // §FS-rhei-panta.6: with the plan positional omitted, complete task and
    // rhei ids against the same target the command itself would resolve.
    first_command_positional(&words, &command)
        .map(PathBuf::from)
        .or_else(|| resolve_plan_target(None).ok().map(|target| target.path))
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
        .ok_or_else(|| miette!(
            help = task_id_help(),
            "task '{}' not found in {}", task_id, plan.display()
        ))
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
        // §FS-rhei-errors.1.2: a bad state machine says which file to edit and
        // how to re-check it, not just that loading failed.
        Some(p) => rhei_validator::StateMachine::from_yaml_file(p)
            .map_err(|err| state_machine_load_report(p, err)),
        None => Ok(rhei_validator::StateMachine::builtin_default()),
    }
}

struct ResolvedStateMachine {
    machine: rhei_validator::StateMachine,
    path: Option<PathBuf>,
}

/// Every machine governing a loaded plan, with the file each resolved from:
/// the project default plus one entry per self-declaring rhei. A ticket's
/// machine — and the callback base its transitions run under — resolves
/// through its owning rhei.
// §DA-per-rhei-state-machines §AR-rhei-panta.4
struct ResolvedMachineSet {
    default: ResolvedStateMachine,
    per_rhei: BTreeMap<String, ResolvedStateMachine>,
}

impl ResolvedMachineSet {
    fn single(default: ResolvedStateMachine) -> Self {
        Self { default, per_rhei: BTreeMap::new() }
    }

    /// The resolved machine governing the ticket named by a project-qualified
    /// id string (`auth.1`): its owning rhei's machine, else the default.
    fn for_task_str(&self, task_id: &str) -> &ResolvedStateMachine {
        let rhei_id = task_id.split('.').next().unwrap_or(task_id);
        self.per_rhei.get(rhei_id).unwrap_or(&self.default)
    }

    fn machine_for_task_str(&self, task_id: &str) -> &rhei_validator::StateMachine {
        &self.for_task_str(task_id).machine
    }

    /// The validator-facing set (owned clone).
    fn validator_set(&self) -> rhei_validator::MachineSet {
        rhei_validator::MachineSet {
            default: self.default.machine.clone(),
            per_rhei: self
                .per_rhei
                .iter()
                .map(|(id, resolved)| (id.clone(), resolved.machine.clone()))
                .collect(),
        }
    }

    /// One group per distinct machine, default first, each carrying the rhei
    /// ids that run it. This is what a reader is shown: a `Source:` line names
    /// one file, so a group that spans two different files would name the
    /// wrong one for at least one of the rheis it claims.
    // §FS-rhei-states-cmd.3: one rendered block per genuinely distinct machine.
    fn machine_groups(&self) -> Vec<MachineGroup<'_>> {
        let mut out = vec![(
            self.default.machine.fingerprint(),
            MachineGroup { resolved: &self.default, rheis: Vec::new() },
        )];
        for (rhei_id, resolved) in &self.per_rhei {
            let fingerprint = resolved.machine.fingerprint();
            match out.iter_mut().find(|(seen, _)| *seen == fingerprint) {
                Some((_, group)) => group.rheis.push(rhei_id.as_str()),
                None => out.push((
                    fingerprint,
                    MachineGroup { resolved, rheis: vec![rhei_id.as_str()] },
                )),
            }
        }
        out.into_iter().map(|(_, group)| group).collect()
    }

    /// [`machine_groups`](Self::machine_groups) narrowed to named rheis. Every
    /// group then names its rheis, including one that runs the project default
    /// — under narrowing "the default" is no longer a statement about the rest
    /// of the project. An empty scope is the whole project.
    // §FS-rhei-states-cmd.3: `--rhei` narrows which machines are reported.
    fn machine_groups_for_scope<'a>(&'a self, scope: &'a [String]) -> Vec<MachineGroup<'a>> {
        if scope.is_empty() {
            return self.machine_groups();
        }
        let mut out: Vec<(String, MachineGroup<'a>)> = Vec::new();
        for rhei_id in scope {
            let resolved = self.per_rhei.get(rhei_id).unwrap_or(&self.default);
            let fingerprint = resolved.machine.fingerprint();
            match out.iter_mut().find(|(seen, _)| *seen == fingerprint) {
                Some((_, group)) => group.rheis.push(rhei_id.as_str()),
                None => out.push((
                    fingerprint,
                    MachineGroup { resolved, rheis: vec![rhei_id.as_str()] },
                )),
            }
        }
        out.into_iter().map(|(_, group)| group).collect()
    }
}

/// One distinct machine and the rheis running it. §FS-rhei-states-cmd.3
struct MachineGroup<'a> {
    resolved: &'a ResolvedStateMachine,
    /// Rhei ids that resolved to this exact machine. Empty on the project
    /// default group, which governs every rhei that declares nothing.
    rheis: Vec<&'a str>,
}

/// Everything the execution paths need per rhei: the machine set for state
/// classification plus the callback base each rhei's transitions run under —
/// a self-declared machine's callbacks resolve relative to *its* states file,
/// exactly as when that workspace runs standalone.
// §DA-per-rhei-state-machines
#[derive(Clone)]
struct ExecutionMachines {
    set: rhei_validator::MachineSet,
    default_callbacks: CallbackPaths,
    per_rhei_callbacks: BTreeMap<String, CallbackPaths>,
    /// The `--state-machine` this invocation resolved under, when one was
    /// given. A run records it so an attached surface renders the machine the
    /// run is executing instead of whatever the default resolves to.
    // §FS-rhei-run-headless.5
    state_machine_override: Option<PathBuf>,
}

impl ExecutionMachines {
    fn build(resolved: &ResolvedMachineSet, input: &Path) -> MietteResult<Self> {
        let default_callbacks = resolve_callback_paths(resolved.default.path.as_deref(), input)?;
        let mut per_rhei_callbacks = BTreeMap::new();
        for (rhei_id, machine) in &resolved.per_rhei {
            per_rhei_callbacks
                .insert(rhei_id.clone(), resolve_callback_paths(machine.path.as_deref(), input)?);
        }
        Ok(Self {
            set: resolved.validator_set(),
            default_callbacks,
            per_rhei_callbacks,
            state_machine_override: None,
        })
    }

    /// Remember the explicit machine override, for the run descriptor.
    // §FS-rhei-run-headless.5
    fn with_state_machine_override(mut self, path: Option<&Path>) -> Self {
        self.state_machine_override = path.map(Path::to_path_buf);
        self
    }

    fn for_task(&self, id: &TaskId) -> &rhei_validator::StateMachine {
        self.set.for_task(id)
    }

    fn for_task_str(&self, task_id: &str) -> &rhei_validator::StateMachine {
        let rhei_id = task_id.split('.').next().unwrap_or(task_id);
        self.set.per_rhei.get(rhei_id).unwrap_or(&self.set.default)
    }

    fn callbacks_for_str(&self, task_id: &str) -> &CallbackPaths {
        let rhei_id = task_id.split('.').next().unwrap_or(task_id);
        self.per_rhei_callbacks.get(rhei_id).unwrap_or(&self.default_callbacks)
    }
}

/// Resolve every machine a loaded plan runs under: the default via the
/// existing project rules, plus each self-declaring rhei's machine — its own
/// execution root's `states.yaml` first, then the shared name-match rules.
/// An explicit `--state-machine` stays a whole-scope override and errors when
/// a rhei in scope declares a different machine name.
// §AR-rhei-panta.4
fn resolve_state_machines_for_loaded_plan(
    input: &Path,
    loaded: &LoadedPlan,
    state_machine_path: Option<&Path>,
) -> MietteResult<ResolvedMachineSet> {
    let default = resolve_state_machine_for_loaded_plan(input, loaded, state_machine_path)?;

    let mut per_rhei = BTreeMap::new();
    let mut declared: Vec<(&String, &String)> = loaded.rhei_machines.iter().collect();
    declared.sort();
    for (rhei_id, machine_name) in declared {
        // Restating the default means the same thing as omitting the line.
        if *machine_name == default.machine.name {
            continue;
        }
        if let Some(override_path) = state_machine_path {
            return Err(miette!(
help = "--state-machine replaces resolution for the whole scope. Narrow the scope with --rhei, or drop the override and let each rhei resolve its own machine.",

                "--state-machine '{}' declares '{}', but rhei '{rhei_id}' declares state \
                 machine '{machine_name}'. The override replaces resolution for the whole \
                 scope; it cannot reinterpret that rhei's states under another machine. \
                 Narrow the invocation or drop the override.",
                override_path.display(),
                default.machine.name,
            ));
        }
        let resolved =
            resolve_declared_rhei_machine(input, loaded, rhei_id, machine_name)?;
        per_rhei.insert(rhei_id.clone(), resolved);
    }

    Ok(ResolvedMachineSet { default, per_rhei })
}

/// Resolve one self-declaring rhei's machine file: the rhei's own execution
/// root first — the shape every instantiated template ships — then the
/// project-level name-match rules. §AR-rhei-panta.4 §FS-rhei-plan-language.1.3
fn resolve_declared_rhei_machine(
    input: &Path,
    loaded: &LoadedPlan,
    rhei_id: &str,
    machine_name: &str,
) -> MietteResult<ResolvedStateMachine> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(root) = loaded.rhei_roots.get(rhei_id) {
        candidates.push(root.join("states.yaml"));
    }
    candidates.push(auto_state_machine_path(input));
    let mut roots: Vec<&PathBuf> = loaded.rhei_roots.values().collect();
    roots.sort();
    roots.dedup();
    for root in roots {
        candidates.push(root.join("states.yaml"));
    }

    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut matches: Vec<(PathBuf, rhei_validator::StateMachine)> = Vec::new();
    for candidate in candidates {
        if !seen.insert(candidate.clone()) || !candidate.is_file() {
            continue;
        }
        // An unloadable candidate is a real project problem; swallowing it
        // here would surface as a misleading "not found" instead.
        let machine = load_state_machine(Some(&candidate))?;
        if machine.name == machine_name {
            // The rhei's own root wins outright; other locations must be a
            // unique match. §AR-rhei-panta.4
            let own_root = loaded
                .rhei_roots
                .get(rhei_id)
                .is_some_and(|root| candidate == root.join("states.yaml"));
            if own_root {
                return Ok(ResolvedStateMachine { machine, path: Some(candidate) });
            }
            matches.push((candidate, machine));
        }
    }

    match matches.len() {
        0 => Err(miette!(
help = states_declaration_help(),

            "rhei '{rhei_id}' declares state machine '{machine_name}', but no states file \
             declaring it was found in the rhei's root, the project root, or any other \
             rhei root. Add a `states.yaml` declaring '{machine_name}' next to the rhei, \
             or pass --state-machine <path>.",
        )),
        1 => {
            let (path, machine) = matches.into_iter().next().expect("single match");
            Ok(ResolvedStateMachine { machine, path: Some(path) })
        }
        _ => {
            let paths: Vec<String> =
                matches.iter().map(|(path, _)| format!("'{}'", path.display())).collect();
            Err(miette!(
help = states_declaration_help(),

                "rhei '{rhei_id}' declares state machine '{machine_name}', and more than one \
                 root holds a states file declaring it: {}.\nMove the definitive file to the \
                 rhei's own root or pass --state-machine <path>.",
                paths.join(", ")
            ))
        }
    }
}

fn auto_state_machine_path(input: &Path) -> PathBuf {
    if let Some(project_dir) = workspace::panta_project_dir(input) {
        project_dir.join("states.yaml")
    } else if workspace::is_workspace(input) {
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
    workspace::panta_project_dir(input)
        .or_else(|| workspace::workspace_dir(input))
        .unwrap_or_else(|| input.to_path_buf())
}

fn resolve_state_machine_for_loaded_plan(
    input: &Path,
    loaded: &LoadedPlan,
    state_machine_path: Option<&Path>,
) -> MietteResult<ResolvedStateMachine> {
    if let Some(path) = state_machine_path {
        let machine = load_state_machine(Some(path))?;
        if loaded.rhei.states_declared && machine.name != loaded.rhei.states.trim() {
            return Err(miette!(
                help = states_declaration_help(),
                "plan declares state machine '{}', but --state-machine '{}' declares '{}'",
                loaded.rhei.states.trim(),
                path.display(),
                machine.name
            ));
        }
        return Ok(ResolvedStateMachine { machine, path: Some(path.to_path_buf()) });
    }

    let builtin = rhei_validator::StateMachine::builtin_default();
    let declared_name = loaded.rhei.states.trim();
    let candidate = auto_state_machine_path(input);

    if !loaded.rhei.states_declared {
        return Ok(ResolvedStateMachine { machine: builtin, path: None });
    }

    let mut mismatch: Option<String> = None;
    if candidate.is_file() {
        let machine = load_state_machine(Some(&candidate))?;
        if machine.name == declared_name {
            return Ok(ResolvedStateMachine { machine, path: Some(candidate) });
        }
        mismatch = Some(machine.name);
    }

    // §AR-rhei-panta.4: when the project-root file is absent or mismatched,
    // a *unique* `name:` match in a rhei root resolves the machine file;
    // several matches are ambiguous — a stale copy must not win silently.
    if declared_name != builtin.name {
        let mut roots: Vec<&PathBuf> = loaded.task_roots.values().collect();
        roots.sort();
        roots.dedup();
        let mut matches: Vec<(PathBuf, rhei_validator::StateMachine)> = Vec::new();
        for root in roots {
            let rhei_candidate = root.join("states.yaml");
            if rhei_candidate == candidate || !rhei_candidate.is_file() {
                continue;
            }
            // An unloadable candidate is a real project problem; swallowing
            // it here would surface as a misleading "not found" instead.
            let machine = load_state_machine(Some(&rhei_candidate))?;
            if machine.name == declared_name {
                matches.push((rhei_candidate, machine));
            }
        }
        if matches.len() > 1 {
            let paths: Vec<String> =
                matches.iter().map(|(path, _)| format!("'{}'", path.display())).collect();
            return Err(miette!(
                help = states_declaration_help(),
                "plan declares state machine '{}', and more than one rhei root holds a \
                 states file declaring it: {}.\nMove the definitive file to the project \
                 root or pass --state-machine <path>.",
                declared_name,
                paths.join(", ")
            ));
        }
        if let Some((path, machine)) = matches.into_iter().next() {
            return Ok(ResolvedStateMachine { machine, path: Some(path) });
        }
        return Err(match mismatch {
            Some(found) => miette!(
                help = states_declaration_help(),
                "plan declares state machine '{}', but auto-discovered states file '{}' declares '{}', and no rhei root holds a states file declaring it.\nUse --state-machine <path> to override the default location.",
                declared_name,
                candidate.display(),
                found
            ),
            None => miette!(
                help = states_declaration_help(),
                "plan declares state machine '{}', but no auto-discovered states file was found at '{}' or, by name, in any rhei root.\nUse --state-machine <path> to override the default location.",
                declared_name,
                candidate.display()
            ),
        });
    }

    Ok(ResolvedStateMachine { machine: builtin, path: None })
}

/// Resolve the state machine for `rhei states`.
///
/// An explicit `--state-machine` answers on its own — the command must stay
/// usable for inspecting a machine file anywhere. Otherwise the target plan
/// decides, exactly as it does for `validate`, `list`, and `run`.
///
/// Discovery is best-effort in both directions that matter: outside any
/// project there is nothing to resolve against, so the built-in default is the
/// honest answer; and an auto-discovered plan that fails to load must not make
/// `rhei states` unusable while the author is repairing that very plan. An
/// explicitly named plan is strict, because the user asked about *that* plan.
///
/// Returns the resolved machines and the rhei ids the invocation is narrowed
/// to — from `--rhei`, or from a target that named one member rhei. Empty is
/// the whole project.
// §FS-rhei-panta.6: project-wide by default, narrowed by `--rhei`.
fn resolve_state_machine_for_states_command(
    input: Option<PathBuf>,
    state_machine: Option<&Path>,
    selected_rheis: &[String],
) -> MietteResult<(ResolvedMachineSet, Vec<String>)> {
    if let Some(path) = state_machine {
        let machine = load_state_machine(Some(path))?;
        return Ok((
            ResolvedMachineSet::single(ResolvedStateMachine {
                machine,
                path: Some(path.to_path_buf()),
            }),
            Vec::new(),
        ));
    }

    let explicit = input.is_some();
    let builtin = || {
        (
            ResolvedMachineSet::single(ResolvedStateMachine {
                machine: rhei_validator::StateMachine::builtin_default(),
                path: None,
            }),
            Vec::new(),
        )
    };
    let target = match resolve_plan_target(input) {
        Ok(target) => target,
        // No plan and no project to speak of: the built-in default is what any
        // plan authored here would get.
        Err(_) if !explicit => return Ok(builtin()),
        Err(err) => return Err(err),
    };

    let selected = target.scope_with(selected_rheis);
    let target = target.path;
    match load_plan(&target).and_then(|loaded| {
        // Validate the selection against the project before reporting on it,
        // so an unknown id names the available rheis instead of quietly
        // narrowing to nothing. §FS-rhei-panta.6
        let scope = resolve_rhei_scope(&loaded, &selected)?;
        let resolved = resolve_state_machines_for_loaded_plan(&target, &loaded, None)?;
        Ok((resolved, scope.map(|ids| ids.into_iter().collect()).unwrap_or_default()))
    }) {
        Ok(resolved) => Ok(resolved),
        Err(err) if !explicit && selected_rheis.is_empty() => {
            eprintln!(
                "warning: could not resolve the state machine from {} ({err}); showing the \
                 built-in default",
                target.display()
            );
            Ok(builtin())
        }
        Err(err) => Err(err),
    }
}

/// Human-readable label for the state machine source, used in diagnostics.
fn state_machine_label(path: Option<&Path>) -> String {
    match path {
        Some(p) => format!("'{}'", p.display()),
        None => "the built-in default state machine".to_string(),
    }
}

/// Execute the `states` subcommand: resolve the state machine the plan or
/// project actually runs under, then print its states and transitions.
/// Resolution matches every other command. §FS-rhei-plan-language.1.3
fn states_command(
    input: Option<PathBuf>,
    state_machine: Option<&Path>,
    selected_rheis: &[String],
    as_json: bool,
) -> MietteResult<()> {
    let (resolved, scope) =
        resolve_state_machine_for_states_command(input, state_machine, selected_rheis)?;
    let groups = resolved.machine_groups_for_scope(&scope);

    if as_json {
        // JSON keeps its stable single-object shape when one machine governs;
        // a heterogeneous project renders as an array, default first.
        // §FS-rhei-states-cmd.5
        let rendered = if groups.len() == 1 {
            render_state_machine_json(&groups[0].resolved.machine).map_err(|err| {
                miette!(help = internal_error_help(), "failed to serialize state machine: {err}")
            })?
        } else {
            let values = groups
                .iter()
                .map(|group| render_state_machine_json_value(&group.resolved.machine))
                .collect::<Vec<_>>();
            serde_json::to_string_pretty(&values).map_err(|err| {
                miette!(help = internal_error_help(), "failed to serialize state machines: {err}")
            })?
        };
        println!("{rendered}");
        return Ok(());
    }

    // Text: one block per distinct machine, the project default first. A block
    // names the rheis running it, except the project-wide default block, whose
    // reach is "everything that declares nothing". §FS-rhei-states-cmd.3
    for (index, group) in groups.iter().enumerate() {
        if index > 0 {
            println!();
        }
        let label = state_machine_label(group.resolved.path.as_deref());
        if group.rheis.is_empty() {
            println!("Source: {label}");
        } else {
            println!("Source: {label} (rhei: {})", group.rheis.join(", "));
        }
        println!("{}", render_state_machine_text(&group.resolved.machine));
    }

    Ok(())
}

/// Filter set for the `list` subcommand. See `Commands::List` for flag docs.
struct ListFilters {
    /// Narrow to named rheis; empty is the whole project. §FS-rhei-panta.6.4
    rhei: Vec<String>,
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
    // Listing is the surface an author reaches for *while* a plan is broken, so
    // it reports what it could not load and shows the rest. §FS-rhei-panta.6
    let loaded = load_plan_leniently(input)?;
    for skipped in &loaded.unloadable {
        eprintln!("warning: {skipped}");
    }
    let rhei_scope = resolve_rhei_scope(&loaded, &filters.rhei)?;
    let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machines = resolved.validator_set();

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

    // §FS-rhei-panta.6: an empty project is a valid project, not an error —
    // say what it is and how to grow it.
    if flat.is_empty() {
        if as_json {
            println!("[]");
        } else if loaded.is_panta_project() {
            println!("(project has no tickets yet)");
            println!("{}", add_a_rhei_help());
        } else {
            println!("(this rhei has no tickets yet)");
        }
        return Ok(());
    }

    // Pre-compute state map for ready/blocked checks (only top-level tasks
    // declare priors, but checking the full flat set is harmless).
    let state_map: HashMap<&TaskId, String> = flat
        .iter()
        .map(|(t, _)| (&t.id, normalized_state_name(t.state.as_str(), machines.for_task(&t.id))))
        .collect();

    let priors_satisfied = |task: &rhei_core::ast::Task| -> bool {
        task.prior.iter().all(|dep| {
            state_map
                .get(dep)
                .map(|s| dependency_is_satisfied(s, machines.for_task(dep)))
                .unwrap_or(false)
        })
    };

    // Judged against every machine in the project rather than the `--rhei`
    // scope: a real state no in-scope rhei uses is an honest empty result.
    // §FS-rhei-list.2.1: a state no machine declares is an error, not silence.
    for requested in &filters.states {
        let requested = requested.trim();
        let known = machines
            .distinct()
            .into_iter()
            .any(|machine| machine.is_valid_state(normalized_state_name(requested, machine)));
        if !known {
            let mut available: BTreeSet<&str> = BTreeSet::new();
            for machine in machines.distinct() {
                available.extend(machine.allowed_states());
            }
            let known =
                available.iter().map(|state| state.to_string()).collect::<Vec<_>>();
            return Err(miette!(
                help = did_you_mean(requested, &known)
                    .unwrap_or_else(|| "this machine declares no states.".to_string()),
                "unknown state '{}'; states in this {}: {}",
                requested,
                if loaded.is_panta_project() { "project" } else { "plan" },
                available.into_iter().collect::<Vec<_>>().join(", ")
            ));
        }
    }

    // Normalize state filter values once per machine so users can pass either
    // canonical names or counted-visit forms; a filter value normalizes under
    // each distinct machine and matches per ticket. §DA-per-rhei-state-machines
    let state_filter: Vec<String> = filters
        .states
        .iter()
        .flat_map(|s| {
            machines
                .distinct()
                .into_iter()
                .map(|machine| normalized_state_name(s.as_str(), machine))
                .collect::<Vec<_>>()
        })
        .collect();
    // §FS-rhei-panta.6: ticket targets accept the qualified id or an
    // unambiguous rhei-local shorthand — including these filter values.
    let parent_filter = filters
        .parent
        .as_deref()
        .map(|id| resolve_cli_task_id(&loaded, id, &rhei_scope))
        .transpose()?
        .map(|id| parse_task_id(&id));
    let has_prior_filter = filters
        .has_prior
        .as_deref()
        .map(|id| resolve_cli_task_id(&loaded, id, &rhei_scope))
        .transpose()?
        .map(|id| parse_task_id(&id));
    let contains_lower = filters.contains.as_deref().map(|s| s.to_lowercase());

    let mut matches: Vec<&(&rhei_core::ast::Task, Option<TaskId>)> = Vec::new();
    for entry in &flat {
        let (task, parent_id) = entry;

        // §FS-rhei-panta.6.4: `--rhei` filters the listing to named rheis.
        if !task_in_rhei_scope(&rhei_scope, &task.id.to_string()) {
            continue;
        }

        if !state_filter.is_empty() {
            let task_state =
                normalized_state_name(task.state.as_str(), machines.for_task(&task.id));
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

        let machine = machines.for_task(&task.id);
        let is_terminal = is_terminal_state(task.state.as_str(), machine);
        if filters.terminal && !is_terminal {
            continue;
        }
        if filters.non_terminal && is_terminal {
            continue;
        }

        if filters.ready || filters.blocked {
            let normalized = normalized_state_name(task.state.as_str(), machine);
            let is_gating = machine.states.get(&normalized).map(|def| def.gating).unwrap_or(false);
            let satisfied = priors_satisfied(task);
            // A ticket whose subtree is still open is not work anyone can be
            // handed — its children are. §FS-rhei-list.3.1 §FS-rhei-next.3

            // Supervision refines both halves: a supervisor is work while its
            // subtree is open, and a held descendant is not.
            // §FS-rhei-supervision.3.2
            let supervising = task_is_supervising(task, machine);
            let subtree_done = supervising || descendants_are_terminal(task, &machines);
            let held = held_by_supervisor(task, &loaded.rhei, &machines).is_some();
            let task_ready = !is_terminal && !is_gating && satisfied && subtree_done && !held;
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
                    // Depth within the owning rhei: the Panta qualification
                    // segment is routing, not plan structure. §FS-rhei-list.4.2
                    "depth": task.profile_level(),
                })
            })
            .collect();
        let rendered = serde_json::to_string_pretty(&payload)
            .map_err(|err| miette!(
                help = internal_error_help(),
                "failed to serialize task list: {err}"
            ))?;
        println!("{rendered}");
        return Ok(());
    }

    if matches.is_empty() {
        println!("(no tasks match the given filters)");
        return Ok(());
    }

    for (task, _) in &matches {
        // Indent by depth within the owning rhei, so top-level tickets stay
        // flush-left after Panta qualification. §FS-rhei-list.4.1
        let indent = "  ".repeat(usize::from(task.profile_level()).saturating_sub(1));
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
