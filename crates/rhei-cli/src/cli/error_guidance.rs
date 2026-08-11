// Shared helpers that give every user-facing CLI error a next action.
// §FS-rhei-errors

// They exist so the "what do I run next" half of a diagnostic is built the same
// way everywhere: quoted for paste, echoing the arguments the user already
// typed, and pointing at a command that reveals whatever they were missing.

// The renderer that keeps a printed command intact lives in `cli_dispatch.rs`
// as `install_diagnostic_handler`: breaking only at spaces leaves a long token
// whole for the terminal to soft-wrap. §FS-rhei-errors.2

/// Quote a shell word so a printed command survives paste into an interactive
/// shell. §FS-rhei-errors.2
fn shell_quote(value: &str) -> String {
    // zsh expands `[`/`]` before the command runs, so an unquoted
    // `agent=codex[yolo]:openai:gpt-5.5` dies with `no matches found`.
    if value.is_empty() {
        return "''".to_string();
    }
    // A word that *begins* with `=` is subject to zsh's EQUALS expansion
    // (`=less` becomes the path to `less`), so it has to be quoted even though
    // `=` is safe everywhere else in a word.
    if value.starts_with('=') {
        return format!("'{}'", value.replace('\'', "'\"'\"'"));
    }
    if value.bytes().all(|byte| {
        matches!(
            byte,
            b'a'..=b'z'
                | b'A'..=b'Z'
                | b'0'..=b'9'
                | b'_'
                | b'-'
                | b'.'
                | b'/'
                | b':'
                | b'@'
                | b'%'
                | b'+'
                | b'='
                | b','
        )
    }) {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// Render a `KEY=VALUE` CLI argument with the value quoted when it needs it.
///
/// Quoting only the right-hand side keeps the key readable, which matters when
/// the point of the suggestion is to teach the input's name.
fn shell_assignment(key: &str, value: &str) -> String {
    format!("{key}={}", shell_quote(value))
}

/// Quote one CLI argument, keeping a leading `KEY=` readable.
///
/// `KEY=VALUE` arguments are the ones users retype by hand, so the key stays
/// bare and only the value is quoted; everything else is quoted whole.
fn shell_arg(value: &str) -> String {
    if let Some((key, rhs)) = value.split_once('=') {
        let is_identifier = !key.is_empty()
            && key.starts_with(|ch: char| ch.is_ascii_alphabetic())
            && key.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-');
        if is_identifier {
            return shell_assignment(key, rhs);
        }
    }
    shell_quote(value)
}

/// Join already-separated arguments into a runnable, paste-safe command line.
fn shell_command<I, S>(parts: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    parts.into_iter().map(|part| shell_arg(part.as_ref())).collect::<Vec<_>>().join(" ")
}

/// The closest candidate to `input`, or `None` when nothing is close enough to
/// be worth suggesting. §FS-rhei-errors.1.3
fn nearest_match<I, S>(input: &str, candidates: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    let lowered = input.to_ascii_lowercase();
    let closest = candidates
        .into_iter()
        .map(|candidate| {
            let candidate = candidate.as_ref().to_string();
            let distance = levenshtein_distance(&lowered, &candidate.to_ascii_lowercase());
            (candidate, distance)
        })
        .min_by(|(left_name, left), (right_name, right)| {
            left.cmp(right).then_with(|| left_name.cmp(right_name))
        })?;

    let (name, distance) = closest;
    let threshold = std::cmp::max(2, input.chars().count() / 3);
    (distance <= threshold).then_some(name)
}

/// How many candidates a fallback list prints in full. Past this the list stops
/// being something a user reads and starts being something they scroll past, so
/// the caller's "list them all with: …" command takes over. §FS-rhei-errors.1.3
const MAX_LISTED_CANDIDATES: usize = 8;

/// A `did you mean` clause for an unknown name, falling back to listing the
/// valid names when none is a near miss. §FS-rhei-errors.1.3
fn did_you_mean(input: &str, known: &[String]) -> Option<String> {
    // `known` is listed in the caller's order, so callers pass the order a user
    // would expect to read (declaration order, not hash order). Duplicates are
    // dropped here rather than at every call site: registries are assembled by
    // merging built-ins with user settings, and a name that survives in both
    // must still be offered once.
    let mut seen = HashSet::new();
    let known = known.iter().filter(|name| seen.insert(name.as_str())).collect::<Vec<_>>();
    if known.is_empty() {
        return None;
    }
    if let Some(name) = nearest_match(input, known.iter().map(|name| name.as_str())) {
        return Some(format!("Did you mean '{name}'?"));
    }
    if known.len() > MAX_LISTED_CANDIDATES {
        let head = known[..MAX_LISTED_CANDIDATES]
            .iter()
            .map(|name| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Some(format!(
            "Valid values include: {head} (and {} more).",
            known.len() - MAX_LISTED_CANDIDATES
        ));
    }
    Some(format!(
        "Valid values: {}.",
        known.iter().map(|name| name.as_str()).collect::<Vec<_>>().join(", ")
    ))
}

fn levenshtein_distance(left: &str, right: &str) -> usize {
    if left == right {
        return 0;
    }
    if left.is_empty() {
        return right.chars().count();
    }
    if right.is_empty() {
        return left.chars().count();
    }

    let right_chars = right.chars().collect::<Vec<_>>();
    let mut previous = (0..=right_chars.len()).collect::<Vec<_>>();
    let mut current = vec![0; right_chars.len() + 1];

    for (left_index, left_char) in left.chars().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_char) in right_chars.iter().enumerate() {
            let insertion = current[right_index] + 1;
            let deletion = previous[right_index + 1] + 1;
            let substitution = previous[right_index] + usize::from(left_char != *right_char);
            current[right_index + 1] = insertion.min(deletion).min(substitution);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[right_chars.len()]
}

/// Help text for a filesystem failure, derived from the error kind so the user
/// is told which of the three usual causes they hit. §FS-rhei-errors.6
fn io_error_help(path: &Path, kind: std::io::ErrorKind) -> String {
    let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty());
    match kind {
        // The two NotFound cases need different remedies: a typo in a name whose
        // directory exists is a `ls` away, while a missing directory has to be
        // created before anything can be written into it.
        std::io::ErrorKind::NotFound => match parent {
            Some(parent) if parent.is_dir() => format!(
                "nothing exists at that path. Check the spelling: ls {}",
                shell_quote(&parent.display().to_string())
            ),
            Some(parent) => format!(
                "nothing exists at that path, and neither does its directory. Create it with: \
                 mkdir -p {}",
                shell_quote(&parent.display().to_string())
            ),
            None => "nothing exists at that path. Check the spelling.".to_string(),
        },
        std::io::ErrorKind::PermissionDenied => format!(
            "the current user cannot access that path. Inspect it with: ls -ld {}",
            shell_quote(&path.display().to_string())
        ),
        std::io::ErrorKind::AlreadyExists => format!(
            "that path already exists. Remove it or pick another: ls -ld {}",
            shell_quote(&path.display().to_string())
        ),
        _ => match parent {
            Some(parent) => format!(
                "check that '{}' exists, is writable, and has free space.",
                parent.display()
            ),
            None => "check that the path exists and is writable.".to_string(),
        },
    }
}

/// Turn a state machine load failure into a diagnostic that says what to fix
/// and where. §FS-rhei-errors.1.2
fn state_machine_load_report(path: &Path, err: rhei_validator::StateMachineLoadError) -> Report {
    let quoted = shell_quote(&path.display().to_string());
    match err {
        rhei_validator::StateMachineLoadError::Io(err) => {
            file_io_report(path, "failed to read state machine", err)
        }
        rhei_validator::StateMachineLoadError::Yaml(err) => miette!(
            help = format!(
                "'{}' is not valid YAML. Fix the syntax at the position above, then re-check it \
                 with: rhei states --state-machine {quoted}",
                path.display()
            ),
            "failed to parse state machine '{}': {err}",
            path.display()
        ),
        rhei_validator::StateMachineLoadError::Invalid(message) => miette!(
            help = format!(
                "edit '{}' so the state definition above is valid, then re-check it with: \
                 rhei states --state-machine {quoted}",
                path.display()
            ),
            "invalid state machine '{}': {message}",
            path.display()
        ),
    }
}

/// Help for a state machine that declares something Rhei cannot execute.
fn state_machine_help() -> &'static str {
    "fix the state definition in the active states.yaml. Inspect the machine \
     rhei resolved with: rhei states"
}

/// Help for a settings file that is missing or malformed.
fn settings_help() -> &'static str {
    "settings merge from ~/.config/rhei/settings.json then .agents/rhei/settings.json. \
     Check both with: rhei diag"
}

/// Help for a plan whose markdown does not carry what a command needs.
fn plan_authoring_help() -> &'static str {
    "check the plan's task metadata (**State:**, **Prior:**, **Assignee:**), then re-run: \
     rhei validate <plan>"
}

/// Help for the snapshot store.
fn snapshot_help() -> &'static str {
    "inspect the snapshot store with: rhei snapshot list"
}

/// Help for a template bundle whose `template.yaml` is wrong.
///
/// The reader here is the template author, not the person instantiating, so the
/// remedy is the manifest plus the validation command that re-checks it.
fn template_manifest_help() -> &'static str {
    "fix template.yaml in the template directory, then re-check the bundle with: \
     rhei instantiate <template> --dry-run"
}

/// Help for a stale or malformed per-task git worktree reference.
fn worktree_ref_help() -> &'static str {
    "a task worktree reference is written by the state that created the worktree. Delete the \
     stale file under runtime/worktree-refs/ and re-run that state."
}

/// Help for a broken internal invariant: an error the user cannot cause must
/// not invent a remedy, so it asks for a report. §FS-rhei-errors.1.2
fn internal_error_help() -> &'static str {
    "this is a bug in rhei, not a problem with your input. Please report it with \
     the command you ran and this message."
}

// The helps below are the shared vocabulary for the recurring failure
// categories: functions, not inline literals, so improving one remedy improves
// every site that reaches it. §FS-rhei-errors.1.2

/// Help for the atomic temp-file dance every plan edit goes through.
fn temp_write_help() -> &'static str {
    "rhei writes plan edits through a temp file in the same directory. Check that \
     directory is writable and has free space."
}

/// Help for the process working directory disappearing under the command.
fn cwd_help() -> &'static str {
    "re-run from a directory that still exists."
}

/// Help for a task id that is not in the plan.
fn task_id_help() -> &'static str {
    "list the task ids in this plan with: rhei list <plan>"
}

/// Help for a task another actor advanced while this command was deciding.
fn task_moved_help() -> &'static str {
    "someone moved the task since you looked. Re-read its current state with: \
     rhei list <plan>"
}

/// Help for an unnamed state.
fn unknown_state_help() -> &'static str {
    "pick a state the machine declares. List them with: rhei states"
}

/// Help for an artifact path that leaves the workspace.
fn artifact_path_help() -> &'static str {
    "artifact paths are workspace-relative. Remove the leading '/' or the '..' \
     segments from this artifact's `path` in the state machine."
}

/// Help for `runtime/` as a whole not being writable.
fn runtime_dir_help() -> &'static str {
    "rhei records results and transitions under runtime/. Check that the workspace \
     directory is writable."
}

/// Help for the per-task result files.
fn runtime_results_help() -> &'static str {
    "rhei records results under runtime/results/. Check that directory is writable."
}

/// Help for the transition log.
fn transition_log_help() -> &'static str {
    "rhei appends to runtime/state-transitions.log. Check that directory is writable."
}

/// Help for the log file a program state writes to.
fn program_log_help() -> &'static str {
    "program output is logged under runtime/logs/. Check that directory is writable."
}

/// Help for a program state whose command failed.
fn program_state_failed_help() -> &'static str {
    "the program state failed. Its log is under runtime/logs/; fix the cause, then re-run."
}

/// Help for the log file an agent invocation writes to.
fn agent_log_help() -> &'static str {
    "agent output is logged under runtime/logs/. Check that directory is writable."
}

/// Help for an agent command that would not start or behaved unexpectedly.
fn agent_command_help() -> &'static str {
    "check the agent's command and flags in settings.json: rhei diag"
}

/// Help for a run that ended on a failing agent pass.
fn run_report_help() -> &'static str {
    "inspect the run with the report it printed, fix the cause, and re-run: rhei run <plan>"
}

/// Help for a run with nothing left it is allowed to pick up.
fn nothing_claimable_help() -> &'static str {
    "every remaining task is blocked, gated, or assigned. See what is left with: \
     rhei list <plan>"
}

/// Help for a transition callback declared by the state machine.
fn callback_command_help() -> &'static str {
    "the callback command is declared in the state machine. Fix the command or the \
     state it redirects to, then retry the transition."
}

/// Help for `pollAttempts`-style operands used outside a poll state.
fn poll_operand_help() -> &'static str {
    "pollAttempts and pollMaxAttempts exist only inside a state that declares \
     `poll:`. Use a different operand, or make the state a poll state."
}

/// Help for a plan whose `**States:**` name disagrees with the states file.
fn states_declaration_help() -> &'static str {
    "the plan's `**States:**` declaration must match the name inside the states \
     file. Rename one of them, or point --state-machine at the matching file."
}

/// Help for a duration that is not `<number><unit>`.
fn duration_format_help() -> &'static str {
    "durations are a number plus a unit: 7d, 4h, 30m, 10s."
}

/// Help for the git worktree rhei needs to read.
fn git_worktree_help() -> &'static str {
    "rhei needs a readable git worktree here. Check `git status` runs in this directory."
}

/// Help for `--watch` failing to acquire an OS watch handle.
fn watch_help() -> &'static str {
    "--watch needs an OS file-watch handle. Re-run without --watch, or raise the \
     inotify limits."
}

/// Help for `rhei viz` pointed somewhere it found nothing to render.
fn viz_path_help() -> &'static str {
    "check the path and re-run: rhei viz <plan-or-directory>"
}

/// Help for an intervention with no dashboard listening.
fn dashboard_required_help() -> &'static str {
    "the dashboard must be running to receive an intervention: rhei run <plan> --dashboard"
}

/// Help for a snapshot reference that does not parse.
fn snapshot_reference_help() -> &'static str {
    "a reference is <task>:<name>[:<state>][@<visit>][:<target>][/g<N>]. Copy one \
     from: rhei snapshot list"
}

/// Help for a snapshot generation whose stored bytes do not read back.
fn snapshot_corrupt_help() -> &'static str {
    "this cached snapshot is corrupt. Delete its generation directory and re-record \
     it: rhei snapshot gc --orphaned"
}

/// Help for the redactor hook that runs over recorded snapshots.
fn snapshot_redactor_help() -> &'static str {
    "the redactor is the command in `snapshot.redact` in settings.json. Check it \
     exists, reads stdin, and writes stdout."
}

/// Help for an agent that can neither record nor resume a native session.
fn session_capture_resume_help() -> &'static str {
    "this agent profile cannot capture or resume a native session. Configure \
     `agents.<id>.session` in settings.json, or continue with an agent that supports it."
}

/// Help for an agent that cannot record a native session.
fn session_capture_help() -> &'static str {
    "this agent profile cannot capture a native session. Configure \
     `agents.<id>.session` in settings.json, or drop snapshot emission for this state."
}

/// Help for a snapshot that cannot satisfy the state's `snapshot.inherit`.
fn snapshot_inherit_help() -> &'static str {
    "the override does not satisfy the state's snapshot.inherit contract. Pick a \
     snapshot that does — list them with: rhei snapshot list — or relax \
     snapshot.inherit in the state machine."
}

/// Help for a snapshot this agent cannot resume.
fn snapshot_resume_help() -> &'static str {
    "that snapshot cannot be resumed by this agent. Pick another with: rhei snapshot \
     list, or run the state without --from-snapshot."
}

/// Help for a `--from-snapshot` value the run does not offer.
fn snapshot_candidates_help() -> &'static str {
    "the candidates above are the snapshot.inherit invocations this run offers. Pass \
     one of them, or drop --from-snapshot."
}

/// Help for a snapshot lookup that needs a fully-qualified target.
fn snapshot_key_help() -> &'static str {
    "snapshots are keyed by agent, provider, and model. Use a full \
     <agent>:<provider>:<model> selector for this state."
}

/// Help for a snapshot with no recorded target.
fn snapshot_target_help() -> &'static str {
    "a snapshot records the target it ran under. Re-create the snapshot, or pass an \
     explicit target."
}

/// Help for more than one cached generation matching an inherit rule.
fn snapshot_ambiguous_help() -> &'static str {
    "more than one cached generation matches. Narrow it with snapshot.inherit.select \
     in the state machine, or prune with: rhei snapshot gc"
}

/// Help for unpacking something embedded in the binary into scratch space.
fn embedded_extraction_help() -> &'static str {
    "built-in skills and templates are unpacked into a temp directory. Check that \
     $TMPDIR exists, is writable, and has free space."
}

/// Help for a `rhei init` conflict, which every message already names a flag for.
fn init_conflict_help() -> &'static str {
    "inspect what is already here with: rhei list, then re-run init with the flag \
     named above."
}

/// Help for a command that needs a ticket id it was not given.
fn ticket_id_required_help() -> &'static str {
    "ticket ids are the bold `Task <id>` values in the plan. List them with: rhei list <plan>"
}

/// Help for a `--rhei` scope that excludes what the command was asked to touch.
fn rhei_scope_help() -> &'static str {
    "drop --rhei to search the whole project, or name the rhei that owns the ticket. \
     List the rheis with: rhei list"
}

/// Help for `--local` used where no project root could be found.
fn local_install_help() -> &'static str {
    "--local writes into the current project. Run it inside a git repository or a \
     Panta project, or install for your user with --user."
}

/// The project root `--local` writes into, or the diagnostic for its absence.
///
/// One call, rather than ten copies of the message. §FS-rhei-errors.1.2
fn require_project_root(project_root: Option<&Path>) -> MietteResult<&Path> {
    project_root.ok_or_else(|| {
        miette!(help = local_install_help(), "--local requires a project root")
    })
}

/// Help for an agent id not in the merged registry. `known` is already seeded
/// with the built-ins, so this never names them twice. §FS-rhei-errors.1.3
fn unknown_agent_help(id: &str, known: &[String]) -> String {
    let hint = did_you_mean(id, known).map(|hint| format!("{hint} ")).unwrap_or_default();
    format!(
        "{hint}Define it under `agents.<id>` in .agents/rhei/settings.json or \
         ~/.config/rhei/settings.json."
    )
}

/// Help for a `--agent` value that is really a selector. Without it the user is
/// told to define `agents.my-agent[nope]:some-model`. §FS-rhei-errors.1.2
fn agent_flag_selector_help(value: &str, known: &[String]) -> Option<String> {
    if !value.contains(':') && !value.contains('[') {
        return None;
    }
    let target = parse_execution_target(value).ok()?;
    let mut parts = vec!["--agent".to_string(), target.agent.clone()];
    if let Some(mode) = target.mode {
        parts.push("--agent-mode".to_string());
        parts.push(mode);
    }
    parts.push("--model".to_string());
    parts.push(target.model);
    let mut help = format!(
        "--agent takes a bare agent id; the mode and model have their own flags. \
         Write it as: {}",
        shell_command(&parts)
    );
    // Surface both problems at once when the id inside the selector is itself a
    // typo, rather than making the user discover it on the next attempt.
    if !known.iter().any(|name| name == &target.agent) {
        if let Some(name) = nearest_match(&target.agent, known) {
            // Its own line: a question mark running straight off the end of a
            // command reads as part of the command.
            help = format!("{help}\nDid you mean '{name}'?");
        }
    }
    Some(help)
}

/// Help for a required handoff that has no recorded producer to inherit from.
// §FS-rhei-errors.6: every failure names the next action.
fn handoff_missing_source_help() -> &'static str {
    "a required handoff inherits from the transition that entered this state. Either \
     reach this state through a transition that produces it, or set `required: false` \
     on the `inherit` entry in the state machine: rhei states"
}

/// Help for a required handoff whose producing state declares no matching output.
fn handoff_no_output_help() -> &'static str {
    "the producing state must declare the handoff it hands over: add an `outputs` entry \
     with `kind: handoff` to that state, or relax the `inherit` entry: rhei states"
}

/// Help for a handoff selection that matches more than one declared output.
fn handoff_ambiguous_help() -> &'static str {
    "more than one handoff output matched. Name the one you want with `name:` on the \
     `inherit` entry, or set `merge: all` to take every match: rhei states"
}

/// Help for a declared handoff whose artifact was never written with content.
fn handoff_empty_artifact_help() -> &'static str {
    "the producing state declared this handoff but wrote nothing to it. An empty file \
     does not satisfy a handoff — check that state's agent log under runtime/logs/, \
     then re-run the producing task."
}
