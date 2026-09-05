// Shared helpers that give every user-facing CLI error a next action.
// §FS-rhei-errors

// They exist so the "what do I run next" half of a diagnostic is built the same
// way everywhere: quoted for paste, echoing the arguments the user already
// typed, and pointing at a command that reveals whatever they were missing.

// The renderer that keeps a printed command intact lives in `cli_dispatch.rs`
// as `install_diagnostic_handler`: breaking only at spaces leaves a long token
// whole for the terminal to soft-wrap. §FS-rhei-errors.2

/// Quote a shell word so a printed command survives paste into an interactive
/// shell — whichever shell this platform gives the user. §FS-rhei-errors.2
fn shell_quote(value: &str) -> String {
    rhei_core::platform::shell_quote(value)
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
        "{hint}Define it under `agents.<id>` in .agent-grounds/rhei/settings.json or \
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
