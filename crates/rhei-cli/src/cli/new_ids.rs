// Deriving, validating, and allocating the ids `rhei new` writes.
//
// Its own part because id rules are the one place where a create can be wrong
// in a way no later command can repair: an id is what every `**Prior:**`, every
// runtime artifact path, and every `rhei list` line is keyed by.

// §FS-rhei-new.4

/// Derive a rhei id from a title: lowercase, every run of characters outside
/// `[a-z0-9_-]` collapsed to a single `-`, trimmed, and any leading non-letter
/// dropped. `None` when nothing usable survives. §FS-rhei-new.4
fn derive_rhei_id(title: &str) -> Option<String> {
    let mut out = String::new();
    for ch in title.chars() {
        let ch = ch.to_ascii_lowercase();
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out: String = out
        .trim_matches('-')
        .chars()
        .skip_while(|ch| !ch.is_ascii_alphabetic())
        .collect();
    (!out.is_empty()).then_some(out)
}

/// A legal single-segment rhei id: a letter, then letters, digits, `_`, or `-`.
// §AR-rhei-panta.3: the id prefixes every ticket id in the project.
fn is_legal_rhei_id(id: &str) -> bool {
    id.bytes().next().is_some_and(|b| b.is_ascii_alphabetic())
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// A legal ticket id segment: `NUMBER | IDENTIFIER`, where an identifier starts
/// with a letter. §FS-rhei-plan-language.2
fn is_legal_ticket_segment(id: &str) -> bool {
    if id.is_empty() {
        return false;
    }
    if id.bytes().all(|b| b.is_ascii_digit()) {
        // `NUMBER = "0" | NONZERO_DIGIT, { DIGIT }` — no leading zeros.
        return id == "0" || !id.starts_with('0');
    }
    is_legal_rhei_id(id)
}

/// The rhei id `rhei new` will create, from `--id` or the title, refusing
/// anything that would not load. §FS-rhei-new.4
fn resolve_new_rhei_id(title: &str, explicit: Option<&str>) -> MietteResult<String> {
    let id = match explicit {
        Some(id) => id.trim().to_string(),
        None => derive_rhei_id(title).ok_or_else(|| miette!(
help = "pass an explicit id, for example: rhei new \"<title>\" --id my-rhei",

            "no rhei id can be derived from the title '{title}': an id must start with a \
             letter and contain only letters, digits, `_`, or `-`"
        ))?,
    };
    if !is_legal_rhei_id(&id) {
        let suggestion = derive_rhei_id(&id)
            .map(|fixed| format!(" Try `--id {fixed}`."))
            .unwrap_or_default();
        return Err(miette!(
help = "a rhei id prefixes every ticket id in the project, so it has to be a single legal segment.",

            "'{id}' is not a valid rhei id: it must start with a letter and contain only \
             letters, digits, `_`, or `-`.{suggestion}"
        ));
    }
    // §FS-rhei-panta.2: `basin` is reserved whether or not basin content
    // exists. Refusing here beats refusing at the next load, where it arrives
    // as a broken project rather than as a rejected argument.
    if id == workspace::BASIN_RHEI_ID {
        return Err(miette!(
help = "the basin is where unfiled tickets go; capture one with `rhei new \"<title>\" --under basin`.",

            "'{}' is a reserved rhei id: it names the project basin, the synthetic rhei \
             that holds tickets with no owning rhei",
            workspace::BASIN_RHEI_ID
        ));
    }
    Ok(id)
}

/// The next free ticket id among `siblings`: one more than the highest numeric
/// sibling, starting at 1. §FS-rhei-new.4
fn next_sibling_number(siblings: &[String]) -> u32 {
    siblings
        .iter()
        .filter_map(|id| id.parse::<u32>().ok())
        .max()
        .map(|highest| highest.saturating_add(1))
        .unwrap_or(1)
}

/// The rhei-local ticket id to write, from `--id` or the sibling numbering,
/// refusing an illegal segment or a collision. §FS-rhei-new.4
fn resolve_new_ticket_segment(
    explicit: Option<&str>,
    siblings: &[String],
    parent_label: &str,
) -> MietteResult<String> {
    let Some(explicit) = explicit else {
        return Ok(next_sibling_number(siblings).to_string());
    };
    let id = explicit.trim();
    if !is_legal_ticket_segment(id) {
        return Err(miette!(
help = "a ticket id segment is a number, or a name starting with a letter (`fix-cache`).",

            "'{id}' is not a valid ticket id: it must be a number, or start with a letter \
             and contain only letters, digits, `_`, or `-`"
        ));
    }
    if siblings.iter().any(|sibling| sibling == id) {
        return Err(miette!(
help = "pick a free id with --id, or omit --id and let the next number be chosen.",

            "{parent_label} already holds a ticket with id '{id}'"
        ));
    }
    Ok(id.to_string())
}

/// Complete `--under`: every rhei id, then every ticket id. A rhei id creates a
/// top-level ticket and a ticket id creates a subtask, so both belong.
// §FS-rhei-new.3
fn complete_new_parent(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(plan) = completion_plan_path() else {
        return Vec::new();
    };
    let prefix = current.to_string_lossy();
    let Ok(loaded) = load_plan(&plan) else {
        return Vec::new();
    };
    let mut candidates: Vec<CompletionCandidate> = loaded
        .rhei_ids
        .iter()
        .filter(|id| id.starts_with(prefix.as_ref()))
        .map(|id| {
            CompletionCandidate::new(id.clone()).help(Some("rhei — adds a top-level ticket".into()))
        })
        .collect();
    candidates.extend(flatten_tasks(&loaded.rhei).into_iter().filter_map(|task| {
        let id = task.id.to_string();
        id.starts_with(prefix.as_ref()).then(|| {
            CompletionCandidate::new(id).help(Some(format!("subtask of {}", task.title).into()))
        })
    }));
    candidates
}

/// Complete `--kind` from the node kinds the target plan declares.
// §FS-rhei-plan-language.3.7
fn complete_new_node_kind(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(plan) = completion_plan_path() else {
        return Vec::new();
    };
    let prefix = current.to_string_lossy();
    let Ok(loaded) = load_plan(&plan) else {
        return Vec::new();
    };
    loaded
        .rhei
        .structure
        .node_kinds
        .iter()
        .filter(|kind| kind.starts_with(prefix.as_ref()))
        .map(|kind| CompletionCandidate::new(kind.clone()))
        .collect()
}
