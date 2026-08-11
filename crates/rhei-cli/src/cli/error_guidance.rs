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

/// A `did you mean` clause for an unknown name, falling back to listing the
/// valid names when none is a near miss. §FS-rhei-errors.1.3
fn did_you_mean(input: &str, known: &[String]) -> Option<String> {
    // `known` is listed in the caller's order, so callers pass the order a user
    // would expect to read (declaration order, not hash order).
    if known.is_empty() {
        return None;
    }
    if let Some(name) = nearest_match(input, known) {
        return Some(format!("Did you mean '{name}'?"));
    }
    Some(format!("Valid values: {}.", known.join(", ")))
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
