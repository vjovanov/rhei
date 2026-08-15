
struct TransitionTaskInfo {
    task: rhei_core::ast::Task,
    level: u8,
}

fn task_profile_allows_state(
    machine: &rhei_validator::StateMachine,
    kind: &str,
    level: u8,
    state: &str,
) -> bool {
    machine
        .profile_for_node(kind, level)
        .is_none_or(|profile| profile.allowed.iter().any(|allowed| allowed == state))
}

/// The plan path as a user would type it: relative to the working directory
/// when it sits beneath it, absolute otherwise.
///
/// A `help =` names commands the user is meant to run next, and every other one
/// echoes the argument they typed. The transition path only has the resolved,
/// canonicalized plan path, so it renders that back down to the same shape
/// rather than pasting an absolute path into the middle of a suggested command.
// §FS-rhei-errors.2
fn plan_arg_for_help(plan_path: &Path) -> String {
    let shown = std::env::current_dir()
        .ok()
        .and_then(|cwd| cwd.canonicalize().ok())
        .and_then(|cwd| plan_path.strip_prefix(cwd).ok())
        .map(|relative| {
            if relative.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                relative.to_path_buf()
            }
        })
        .unwrap_or_else(|| plan_path.to_path_buf());
    shell_quote(&shown.display().to_string())
}

/// Reject a move into a `final: true` state while the task still has a
/// non-terminal descendant.
///
/// Descendants-first is a property of the graph, not of a command: it belongs
/// on the shared transition path beside compare-and-swap, artifact
/// enforcement, and callbacks, so `rhei transition`, `rhei complete`, `rhei
/// run`'s auto-advance, and a callback redirect all enforce it identically.
/// It cannot be delegated to machine authors either — transition `condition:`
/// expressions see only visit and exit-code variables, so no machine can gate
/// a parent's terminal edge on its children.
///
/// This is deliberately not symmetric with `**Prior:**` readiness, which
/// `rhei transition` skips as the human escape hatch: an out-of-order prior is
/// a `rhei validate` warning, a terminal parent with an open descendant is an
/// error. `transition` may produce a warning; it must never produce an error.
// §FS-rhei-transition-cmd.3.1 §FS-rhei-states.2.3
fn ensure_descendants_terminal_for_terminal_entry(
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
    local_id: &str,
    qualified_id: &str,
    to: &str,
    plan_path: &Path,
) -> MietteResult<()> {
    if task.children.is_empty()
        || !machine.states.get(to).map(|def| def.terminal).unwrap_or(false)
    {
        return Ok(());
    }
    // The task tree parsed here carries rhei-local ids; the caller knows the
    // project-qualified form, so recover the prefix and report descendants the
    // way every other surface prints them. §FS-rhei-panta.6
    let prefix = qualified_id.strip_suffix(local_id).unwrap_or("");
    let open = non_terminal_descendants(task, machine, prefix);
    if open.is_empty() {
        return Ok(());
    }
    // Name the command that shows the open work and the one that claims it:
    // "finish the descendants" is the answer, but the user still has to find
    // them. §FS-rhei-errors.2
    let plan = plan_arg_for_help(plan_path);
    Err(miette!(
        help = format!(
            "a parent is finished after its subtree is, and nothing finishes it on its \
             children's behalf. See the open work with: rhei list {plan} --non-terminal, \
             then claim it with: rhei next {plan}"
        ),
        "Task {} cannot enter terminal state '{}' while descendant tasks remain non-terminal.\n\
         Offending descendants: {}",
        qualified_id,
        to,
        open.join(", ")
    ))
}

/// The ticket's result file under the owning rhei's execution root.
/// §FS-rhei-complete.3
fn result_file_path(artifact_root: &Path, task_id: &str) -> PathBuf {
    artifact_root.join("runtime").join("results").join(format!("{task_id}.md"))
}

/// Where one invocation writes its account, relative to the artifact root.
///
/// A single-invocation state writes the ticket's result file itself. A
/// fanned-out state gives every invocation its own fragment keyed by the same
/// identity the rest of its artifacts are keyed by, because one shared path
/// would let the last writer erase its siblings and the first writer satisfy
/// the obligation on everyone's behalf.
// §FS-rhei-states.3.3
fn result_relative_path(task_id: &str, identity: Option<&str>) -> String {
    match identity {
        Some(identity) => format!("runtime/results/{task_id}/{identity}.md"),
        None => format!("runtime/results/{task_id}.md"),
    }
}

/// [`result_relative_path`] resolved against the owning rhei's execution root.
// §FS-rhei-states.3.3
fn invocation_result_file_path(
    artifact_root: &Path,
    task_id: &str,
    identity: Option<&str>,
) -> PathBuf {
    artifact_root.join(result_relative_path(task_id, identity))
}

/// The per-invocation key a fanned-out state's result fragments are filed
/// under: the target slug for `all_targets`, the model id for `all_models`.
///
/// `None` for every state that runs one invocation — those keep writing the
/// ticket's result file directly, so nothing changes for the common case.
// §FS-rhei-states.3.3 §FS-rhei-transitions.4.2
fn fanout_result_identity(
    state_def: Option<&rhei_validator::StateDef>,
    target: Option<&ExecutionTarget>,
    model: Option<&str>,
) -> Option<String> {
    let state_def = state_def?;
    if !state_def.all_targets.is_empty() {
        return target.map(|target| target.slug());
    }
    if !state_def.all_models.is_empty() {
        return model.map(slugify_target_value);
    }
    None
}

/// Every result fragment a fanned-out state's invocations were told to write,
/// in declared invocation order, paired with the identity that keyed it.
// §FS-rhei-states.3.3
fn fanout_result_fragments(
    artifact_root: &Path,
    task_id: &str,
    state_def: &rhei_validator::StateDef,
    invocations: &[ResolvedAgent],
) -> Vec<(String, PathBuf)> {
    invocations
        .iter()
        .filter_map(|resolved| {
            let identity = fanout_result_identity(
                Some(state_def),
                resolved.target.as_ref(),
                resolved.model.as_deref(),
            )?;
            let path = invocation_result_file_path(artifact_root, task_id, Some(&identity));
            Some((identity, path))
        })
        .collect()
}

/// Fold a fanned-out state's per-invocation fragments into the ticket's one
/// result file, one attributed `## Result` entry each, in declared invocation
/// order.
///
/// Called by `rhei run` after the last invocation and before the transition is
/// applied, so the shared path sees a single non-empty result carrying every
/// worker's account instead of whichever invocation happened to write last.
/// The heading carries the identity and no arrow, so the result-file history
/// reader (which keys on `<from> → <to>` headings) still reads the file the way
/// it always did. Entries are **appended**: a ticket that already collected a
/// result on an earlier hop keeps it, exactly as any other carried message
/// accumulates.
///
/// Every declared invocation must have written its fragment. This is the same
/// rule declared `outputs:` follow — those are checked across every invocation
/// identity on the shared path (`ensure_state_outputs_exist_for_transition`) —
/// and it is what makes the per-invocation completion condition stick: the
/// invocations finish in whatever order they finish, so without it the last one
/// to satisfy *its own* condition would carry a silent sibling over the edge.
// §FS-rhei-states.3.3 §FS-rhei-agents.3.2 §FS-rhei-complete.3.2
fn merge_fanout_result_fragments(
    artifact_root: &Path,
    task_id: &str,
    state_name: &str,
    state_def: &rhei_validator::StateDef,
    invocations: &[ResolvedAgent],
) -> MietteResult<bool> {
    let fragments = fanout_result_fragments(artifact_root, task_id, state_def, invocations);
    if fragments.is_empty() {
        return Ok(false);
    }
    let mut merged = String::new();
    let mut missing: Vec<String> = Vec::new();
    for (identity, path) in fragments {
        let content = fs::read_to_string(&path).unwrap_or_default();
        if content.trim().is_empty() {
            missing.push(format!("{identity} ({})", path.display()));
            continue;
        }
        // A fragment that already opens with its own `## Result` heading is
        // re-titled rather than nested: the merged file must read as one list
        // of entries, not as a heading inside a heading.
        let trimmed = content.trim();
        let body = trimmed.strip_prefix("## Result").map(str::trim_start).unwrap_or(trimmed);
        merged.push_str(&format!("## Result \u{2014} {identity}\n\n{body}\n\n"));
    }
    if !missing.is_empty() {
        return Err(miette!(
            help = format!(
                "each invocation of a fanned-out state writes its own result, and the ticket's \
                 result is the merge of them. Rerun to let the missing invocation(s) write, or \
                 write the file(s) named above."
            ),
            "Task {} cannot finish from '{}': {} of its fan-out invocation(s) wrote no result.\n\
             Missing: {}",
            task_id,
            state_name,
            missing.len(),
            missing.join(", ")
        ));
    }
    let destination = result_file_path(artifact_root, task_id);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| file_io_report(parent, "failed to create runtime/results", err))?;
    }
    let mut existing = fs::read_to_string(&destination).unwrap_or_default();
    if !existing.trim().is_empty() && !existing.ends_with('\n') {
        existing.push('\n');
    }
    let combined =
        if existing.trim().is_empty() { merged } else { format!("{existing}{merged}") };
    fs::write(&destination, combined)
        .map_err(|err| file_io_report(&destination, "failed to merge fanout results", err))?;
    Ok(true)
}

/// The ticket's result file as a subprocess must see it: absolute.
///
/// A subprocess runs from the checkout root, which is routinely not the Rhei
/// artifact root, so a root that was itself given relative to `rhei run`'s own
/// working directory would resolve somewhere else entirely in the child.
// §FS-rhei-agents.4 §FS-rhei-programs.2
fn absolute_result_file_path(artifact_root: &Path, task_id: &str) -> PathBuf {
    let path = result_file_path(artifact_root, task_id);
    std::path::absolute(&path).unwrap_or(path)
}

/// [`absolute_result_file_path`] for one invocation of a possibly fanned-out
/// state: the ticket's result file, or that invocation's own fragment.
// §FS-rhei-states.3.3 §FS-rhei-agents.4
fn absolute_invocation_result_file_path(
    artifact_root: &Path,
    task_id: &str,
    identity: Option<&str>,
) -> PathBuf {
    let path = invocation_result_file_path(artifact_root, task_id, identity);
    std::path::absolute(&path).unwrap_or(path)
}

/// Whether a file exists and holds something other than whitespace.
///
/// Whitespace-only counts as absent, on the same reading state handoffs use: an
/// existence-only contract would otherwise let an empty file stand in for an
/// answer.
// §FS-rhei-states.3.3 §FS-rhei-states.3.2
fn file_has_content(path: &Path) -> bool {
    fs::read_to_string(path).map(|content| !content.trim().is_empty()).unwrap_or(false)
}

/// Whether the ticket already has a result worth the name.
// §FS-rhei-states.3.3
fn task_result_is_present(artifact_root: &Path, task_id: &str) -> bool {
    file_has_content(&result_file_path(artifact_root, task_id))
}

/// Reject an edge into a `final: true` state when nothing says why the ticket
/// ended there.
///
/// The terminal result is an artifact contract of the target state that no
/// machine declares and none can opt out of. `outputs:` cannot express it —
/// those are checked when a state is *left*, and a terminal state is never left
/// — so it is enforced here, on the edge in, at the same point the target
/// state's `inputs:` are enforced and against the same effective target, so a
/// callback `nextState` redirect cannot smuggle a terminal entry past it.
///
/// It is satisfied by an existing non-empty result file or by a message the
/// caller carried through the move; the message is appended once the move
/// succeeds.
// §FS-rhei-states.3.3 §FS-rhei-transition-cmd.3.2
fn ensure_terminal_result_available(
    machine: &rhei_validator::StateMachine,
    artifact_root: &Path,
    qualified_id: &str,
    from: &str,
    to: &str,
    carried_message: Option<&str>,
    plan_path: &Path,
) -> MietteResult<()> {
    if !machine.states.get(to).map(|def| def.terminal).unwrap_or(false) {
        return Ok(());
    }
    if carried_message.is_some_and(|message| !message.trim().is_empty()) {
        return Ok(());
    }
    if task_result_is_present(artifact_root, qualified_id) {
        return Ok(());
    }
    let relative = format!("runtime/results/{qualified_id}.md");
    // The suggested commands carry the plan, like every other `help =` here;
    // without it they only run from at or below the plan's own directory.
    // §FS-rhei-errors.2
    let plan = plan_arg_for_help(plan_path);
    // Name the file that was checked and the flag that carries the message:
    // "write a result" is the answer, but the user still has to know where.
    // §FS-rhei-errors.2
    Err(miette!(
        help = format!(
            "a final state records why the ticket ended there. Pass it on the move: \
             rhei transition {plan} --task {qualified_id} --from {from} --to {to} \
             --result \"<what happened>\" \
             (rhei complete {plan} --task {qualified_id} --result \"<what happened>\" for the \
             everyday finish), or write {relative} before the move."
        ),
        "Task {} cannot enter terminal state '{}' without a result.\n\
         Expected a non-empty result file at: {}",
        qualified_id,
        to,
        result_file_path(artifact_root, qualified_id).display()
    ))
}

fn ensure_task_profile_allows_state(
    machine: &rhei_validator::StateMachine,
    task_id_str: &str,
    kind: &str,
    level: u8,
    state: &str,
) -> MietteResult<()> {
    let Some(profile) = machine.profile_for_node(kind, level) else {
        return Ok(());
    };
    if profile.allowed.iter().any(|allowed| allowed == state) {
        return Ok(());
    }

    Err(miette!(
        help = "the task's node profile restricts which states it may enter. Change the task's profile, or widen the profile in the state machine.",
        "Task {} cannot enter state '{}': state is not allowed by its resolved profile. Profile allows: [{}]",
        task_id_str,
        state,
        profile.allowed.join(", ")
    ))
}

#[allow(clippy::too_many_arguments)]
fn execute_transition_with_origin(
    files: TransitionFiles<'_>,
    callback_paths: &CallbackPaths,
    machine: &rhei_validator::StateMachine,
    task_id_str: &str,
    from: &str,
    to: &str,
    no_callbacks: bool,
    origin: TransitionOrigin,
) -> MietteResult<String> {
    let task_file = files.task_file;
    let metadata_file = files.metadata_file;
    let workspace_root = execution_workspace_root(&callback_paths.plan_path);
    let settings = load_merged_settings(&workspace_root)?;

    // Validate that both `from` and `to` are valid states.
    if !machine.is_valid_state(from) {
        let allowed = machine.allowed_states().collect::<Vec<_>>().join(", ");
        return Err(miette!(
            help = unknown_state_help(),
            "'{}' is not a valid state. Allowed: [{}]", from, allowed
        ));
    }
    if !machine.is_valid_state(to) {
        let allowed = machine.allowed_states().collect::<Vec<_>>().join(", ");
        return Err(miette!(
            help = unknown_state_help(),
            "'{}' is not a valid state. Allowed: [{}]", to, allowed
        ));
    }

    // Open the file(s) with an exclusive lock for the duration of the operation.
    let metadata_handle = fs::File::open(metadata_file)
        .map_err(|err| file_io_report(metadata_file, "failed to open plan file", err))?;
    metadata_handle
        .lock_exclusive()
        .map_err(|err| file_io_report(metadata_file, "failed to acquire file lock", err))?;
    let task_handle = if task_file == metadata_file {
        None
    } else {
        let handle = fs::File::open(task_file)
            .map_err(|err| file_io_report(task_file, "failed to open plan file", err))?;
        handle
            .lock_exclusive()
            .map_err(|err| file_io_report(task_file, "failed to acquire file lock", err))?;
        Some(handle)
    };

    // Read the raw markdown while holding the locks.
    let metadata_raw = fs::read_to_string(metadata_file)
        .map_err(|err| file_io_report(metadata_file, "failed to read plan file", err))?;
    let task_raw = if task_file == metadata_file {
        metadata_raw.clone()
    } else {
        fs::read_to_string(task_file)
            .map_err(|err| file_io_report(task_file, "failed to read plan file", err))?
    };

    // Parse to validate structure and find the task.
    // Try full plan parse first; fall back to manifest + task-file parse.
    let target_id = parse_task_id(task_id_str);
    // The metadata key is the ticket's local id everywhere except the basin,
    // whose metadata shares the project manifest under qualified ids.
    let metadata_key = parse_task_id(files.metadata_id);
    let manifest = if task_file == metadata_file {
        None
    } else {
        Some(parse_metadata_manifest(metadata_file, &metadata_raw)?)
    };
    let task_info = find_task_transition_info(
        &task_raw,
        task_file,
        manifest.as_ref().map(|index| &index.structure),
        &target_id,
        task_id_str,
    )?;
    let current_state_raw = task_info.task.state.clone();
    let current_state = normalized_state_name(&current_state_raw, machine);
    let metadata = if task_file == metadata_file {
        rhei_core::parse(&metadata_raw)
            .map_err(|err| {
                miette!(
                    help = plan_authoring_help(),
                    "failed to parse plan for transition metadata: {}", err.message
                )
            })?
            .metadata
    } else {
        manifest.and_then(|index| index.metadata)
    };

    // Compare-and-swap: verify the task's current state matches `from`.
    // This runs before the transition-legality check so a wrong `--from`
    // produces the actionable "task is in state X" error instead of the
    // less informative "transition not allowed" error.
    if current_state != from {
        if let Some(task_handle) = &task_handle {
            let _ = fs2::FileExt::unlock(task_handle);
        }
        let _ = fs2::FileExt::unlock(&metadata_handle);
        return Err(miette!(
            help = task_moved_help(),
            "conflict: Task {} is in state '{}', expected '{}'",
            files.artifact_id,
            current_state_raw,
            from
        ));
    }
    if let Err(err) = ensure_task_profile_allows_state(
        machine,
        files.artifact_id,
        &task_info.task.kind,
        task_info.level,
        to,
    ) {
        if let Some(task_handle) = &task_handle {
            let _ = fs2::FileExt::unlock(task_handle);
        }
        let _ = fs2::FileExt::unlock(&metadata_handle);
        return Err(err);
    }

    // Now that we know the task really is in `from`, check whether the
    // declared transitions permit `from -> to`.
    let matching_rule =
        machine.transitions().iter().find(|rule| rule.from.0 == from && rule.to.0 == to).or_else(
            || machine.transitions().iter().find(|rule| rule.from.0 == "*" && rule.to.0 == to),
        );
    let Some(matching_rule) = matching_rule else {
        if let Some(task_handle) = &task_handle {
            let _ = fs2::FileExt::unlock(task_handle);
        }
        let _ = fs2::FileExt::unlock(&metadata_handle);
        return Err(miette!(
            help = "the machine declares no such edge. List the edges with: rhei states",
            "transition from '{}' to '{}' is not allowed by the state machine",
            from,
            to
        ));
    };

    let normalized_metadata = ensure_current_state_visit_count(
        metadata.as_ref(),
        &metadata_key,
        from,
        &current_state_raw,
        machine,
    );
    let metadata_for_checks = normalized_metadata.as_ref().or(metadata.as_ref());

    if !transition_rule_is_applicable(
        matching_rule,
        machine,
        metadata_for_checks,
        &metadata_key,
        from,
        &current_state_raw,
    )? {
        if let Some(task_handle) = &task_handle {
            let _ = fs2::FileExt::unlock(task_handle);
        }
        let _ = fs2::FileExt::unlock(&metadata_handle);
        let reason = describe_blocked_transition(
            matching_rule,
            machine,
            metadata_for_checks,
            &metadata_key,
            from,
            &current_state_raw,
        );
        let alternatives = applicable_alternatives(
            machine,
            metadata_for_checks,
            &metadata_key,
            from,
            &current_state_raw,
        );
        let suffix = if alternatives.is_empty() {
            "No other transitions from this state are currently applicable.".to_string()
        } else {
            format!(
                "Currently applicable transitions from '{}': {}.",
                from,
                alternatives.join(", ")
            )
        };
        return Err(miette!(
            help = "the edge exists but its condition is unmet. Inspect the machine with: rhei states",
            "transition from '{}' to '{}' is not currently applicable: {}. {}",
            from,
            to,
            reason,
            suffix
        ));
    }

    // Descendants-first runs once the edge is declared and applicable, before
    // any callback: no "close your subtree" for a move that was never
    // available anyway. §FS-rhei-transition-cmd.3
    if let Err(err) = ensure_descendants_terminal_for_terminal_entry(
        machine,
        &task_info.task,
        task_id_str,
        files.artifact_id,
        to,
        &callback_paths.plan_path,
    ) {
        if let Some(task_handle) = &task_handle {
            let _ = fs2::FileExt::unlock(task_handle);
        }
        let _ = fs2::FileExt::unlock(&metadata_handle);
        return Err(err);
    }

    let from_state_def = machine
        .states
        .get(from)
        .ok_or_else(|| miette!(
            help = internal_error_help(),
            "state '{}' missing from loaded machine", from
        ))?;

    let from_invocations = resolve_agent_invocations_for_task(
        machine,
        from,
        &settings,
        &default_run_options(),
        Some(&task_info.task),
    )
    .unwrap_or_default();
    let callback_contexts = callback_contexts_for_state(from_state_def, &from_invocations);

    // Parse the plan once for callback-context serialization. Failure here
    // means we fall back to a minimal payload rather than aborting — the
    // transition should still run even if the plan is only partially
    // structured.
    let plan_for_context = rhei_core::parse(&metadata_raw).ok();

    // Accumulated `transitionData` that flows from on_leave callbacks to
    // on_enter. Starts from the engine-seeded payload (e.g. timeout data)
    // and each callback's `data` merges last-write-wins.
    let mut transition_data: serde_json::Value = origin
        .seed_data
        .clone()
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
    // A callback may request a redirect via `next_state`. The first such
    // request wins; later callbacks are still executed against the original
    // `to` for rejection checks, but their redirects are ignored.
    let mut redirect_next_state: Option<String> = None;

    // Execute on_leave callback before the state change.
    if !no_callbacks {
        if let Some(ref cb) = matching_rule.on_leave {
            let executor = ShellCallbackExecutor;
            for (model, agent) in callback_contexts {
                let context_json = build_transition_context_json(
                    plan_for_context.as_ref(),
                    &callback_paths.plan_path,
                    task_id_str,
                    files.artifact_id,
                    from,
                    to,
                    origin.triggered_by.unwrap_or("user"),
                    &transition_data,
                    &callback_paths.working_dir,
                );
                let ctx = CallbackContext {
                    // §FS-rhei-panta.6: callbacks see the qualified id; the
                    // rhei-local heading id rides along for file edits.
                    task_id: files.artifact_id,
                    task_id_local: task_id_str,
                    from_state: from,
                    to_state: to,
                    plan_path: &callback_paths.plan_path,
                    callback_cwd: &callback_paths.working_dir,
                    model,
                    agent,
                    context_json: Some(&context_json),
                };
                let result = executor.execute(cb, &ctx).map_err(|e| miette!(
                    help = state_machine_help(),
                    "{e}"
                ))?;
                if !result.success {
                    if let Some(task_handle) = &task_handle {
                        let _ = fs2::FileExt::unlock(task_handle);
                    }
                    let _ = fs2::FileExt::unlock(&metadata_handle);
                    let message = result
                        .error
                        .clone()
                        .unwrap_or_else(|| "transition rejected by callback".to_string());
                    return Err(miette!(
                        help = callback_command_help(),
                        "on_leave callback '{}' rejected the transition: {message}",
                        cb.0
                    ));
                }
                if let Some(data) = result.data.as_ref() {
                    merge_transition_data(&mut transition_data, data);
                }
                if let Some(redirect) = result.next_state.clone() {
                    if redirect_next_state.is_none() {
                        redirect_next_state = Some(redirect);
                    }
                }
            }
        }
    }

    // Resolve redirects before committing state: validate the redirect is a
    // declared transition from the current state. A redirect to the same
    // target is a no-op.
    let (effective_to, effective_rule) = if let Some(redirect) = redirect_next_state.as_deref() {
        if redirect == to {
            (to.to_string(), matching_rule)
        } else if !machine.is_valid_state(redirect) {
            if let Some(task_handle) = &task_handle {
                let _ = fs2::FileExt::unlock(task_handle);
            }
            let _ = fs2::FileExt::unlock(&metadata_handle);
            return Err(miette!(
                help = callback_command_help(),
                "on_leave callback redirected to unknown state '{}'", redirect
            ));
        } else if let Err(err) = ensure_task_profile_allows_state(
            machine,
            files.artifact_id,
            &task_info.task.kind,
            task_info.level,
            redirect,
        ) {
            if let Some(task_handle) = &task_handle {
                let _ = fs2::FileExt::unlock(task_handle);
            }
            let _ = fs2::FileExt::unlock(&metadata_handle);
            return Err(err);
        } else if let Some(rule) =
            machine.transitions().iter().find(|r| r.from.0 == from && r.to.0 == redirect).or_else(
                || machine.transitions().iter().find(|r| r.from.0 == "*" && r.to.0 == redirect),
            )
        {
            (redirect.to_string(), rule)
        } else {
            if let Some(task_handle) = &task_handle {
                let _ = fs2::FileExt::unlock(task_handle);
            }
            let _ = fs2::FileExt::unlock(&metadata_handle);
            return Err(miette!(
                help = callback_command_help(),
                "on_leave callback redirected to '{}', but no transition from '{}' to '{}' is declared",
                redirect,
                from,
                redirect
            ));
        }
    } else {
        (to.to_string(), matching_rule)
    };
    let to = effective_to.as_str();
    let matching_rule = effective_rule;

    // §FS-rhei-transition-cmd.3.1: re-check against the effective target so a
    // `nextState` redirect cannot smuggle a terminal entry past the guard.
    if let Err(err) = ensure_descendants_terminal_for_terminal_entry(
        machine,
        &task_info.task,
        task_id_str,
        files.artifact_id,
        to,
        &callback_paths.plan_path,
    ) {
        if let Some(task_handle) = &task_handle {
            let _ = fs2::FileExt::unlock(task_handle);
        }
        let _ = fs2::FileExt::unlock(&metadata_handle);
        return Err(err);
    }

    let to_state_def = machine
        .states
        .get(to)
        .ok_or_else(|| miette!(
            help = internal_error_help(),
            "state '{}' missing from loaded machine", to
        ))?;

    let mut updated_metadata =
        update_metadata_for_transition(metadata_for_checks, &metadata_key, to, machine)
            .or_else(|| normalized_metadata.clone());
    if from_state_def.poll.is_some() && to != from {
        updated_metadata = clear_poll_state_metadata(
            updated_metadata.as_ref().or(metadata_for_checks),
            &metadata_key,
            from,
        );
    }
    let from_visit_count = Some(render_visit_count(
        metadata_for_checks,
        &metadata_key,
        from,
        &current_state_raw,
        machine,
    ));
    let to_visit_count = updated_metadata
        .as_ref()
        .map(|meta| task_visit_count(Some(meta), &metadata_key, to))
        .filter(|count| *count > 0);

    // §AR-rhei-panta.2: artifact templates render the project-qualified id
    // against the owning rhei's execution root, matching the paths agents and
    // ready-checks were shown.
    if !origin.skip_source_outputs {
        ensure_state_outputs_exist_for_transition(
            files.artifact_root,
            Some(&task_info.task),
            files.artifact_id,
            from,
            from_state_def,
            from_visit_count,
            machine,
            &settings,
        )?;
    }
    ensure_state_inputs_exist_for_transition(
        files.artifact_root,
        Some(&task_info.task),
        files.artifact_id,
        to,
        to_state_def,
        to_visit_count,
        machine,
        &settings,
        &format!("Task {} cannot enter state {}.", files.artifact_id, to),
    )?;
    // A caller that knows the outcome carries the message; otherwise the
    // engine's own account stands in, but only where it is true.
    // §FS-rhei-states.3.3 §FS-rhei-run.3
    let recorded_message = origin.result_message.clone().or_else(|| {
        let lands_terminal = machine.states.get(to).map(|def| def.terminal).unwrap_or(false);
        if lands_terminal && !task_result_is_present(files.artifact_root, files.artifact_id) {
            origin.terminal_result_fallback.clone()
        } else {
            None
        }
    });

    // The terminal result is the one artifact contract of a `final: true` state
    // that no machine declares, checked here beside the declared ones and
    // against the same effective target. §FS-rhei-states.3.3
    if let Err(err) = ensure_terminal_result_available(
        machine,
        files.artifact_root,
        files.artifact_id,
        from,
        to,
        recorded_message.as_deref(),
        &callback_paths.plan_path,
    ) {
        if let Some(task_handle) = &task_handle {
            let _ = fs2::FileExt::unlock(task_handle);
        }
        let _ = fs2::FileExt::unlock(&metadata_handle);
        return Err(err);
    }

    let rendered_to_state = format_task_state_value(to, to_visit_count, machine);
    let metadata_raw_updated = if task_file == metadata_file {
        let new_task_raw = rewrite_task_state(&task_raw, task_id_str, &rendered_to_state)?;
        if let Some(updated_metadata) = updated_metadata.as_ref() {
            rewrite_frontmatter(&new_task_raw, updated_metadata)?
        } else {
            new_task_raw
        }
    } else if let Some(updated_metadata) = updated_metadata.as_ref() {
        rewrite_frontmatter(&metadata_raw, updated_metadata)?
    } else {
        metadata_raw.clone()
    };

    let task_raw_updated = if task_file == metadata_file {
        None
    } else {
        Some(rewrite_task_state(&task_raw, task_id_str, &rendered_to_state)?)
    };

    // Atomic write(s): write to temp file in the same directory, then rename.
    write_file_atomic(metadata_file, &metadata_raw_updated)?;
    if let Some(ref task_raw_updated) = task_raw_updated {
        write_file_atomic(task_file, task_raw_updated)?;
    }

    // Execute on_enter callback after the state change (not model-looped).
    let triggered_by = origin.triggered_by.unwrap_or(if redirect_next_state.is_some() {
        "callback"
    } else {
        "user"
    });
    let on_enter_context_json = build_transition_context_json(
        plan_for_context.as_ref(),
        &callback_paths.plan_path,
        task_id_str,
        files.artifact_id,
        from,
        to,
        triggered_by,
        &transition_data,
        &callback_paths.working_dir,
    );
    let callback_ctx = CallbackContext {
        task_id: files.artifact_id,
        task_id_local: task_id_str,
        from_state: from,
        to_state: to,
        plan_path: &callback_paths.plan_path,
        callback_cwd: &callback_paths.working_dir,
        model: None,
        agent: None,
        context_json: Some(&on_enter_context_json),
    };
    if !no_callbacks {
        if let Some(ref cb) = matching_rule.on_enter {
            let executor = ShellCallbackExecutor;
            let result = executor.execute(cb, &callback_ctx).map_err(|e| miette!(
                help = state_machine_help(),
                "{e}"
            ))?;
            if !result.success {
                // Spec §Example 8: on_enter failure rolls back the state
                // write to the original, then the error_handling policy
                // applies. We implement the rollback; policy execution is
                // a follow-up.
                let rollback_err = write_file_atomic(metadata_file, &metadata_raw).err();
                let task_rollback_err = if task_raw_updated.is_some() {
                    write_file_atomic(task_file, &task_raw).err()
                } else {
                    None
                };
                if let Some(task_handle) = &task_handle {
                    let _ = fs2::FileExt::unlock(task_handle);
                }
                let _ = fs2::FileExt::unlock(&metadata_handle);
                let message =
                    result.error.clone().unwrap_or_else(|| "on_enter callback failed".to_string());
                if rollback_err.is_some() || task_rollback_err.is_some() {
                    return Err(miette!(
                        help = callback_command_help(),
                        "on_enter callback '{}' failed ({message}); rollback also failed — plan file may be inconsistent",
                        cb.0
                    ));
                }
                return Err(miette!(
                    help = callback_command_help(),
                    "on_enter callback '{}' failed: {message}", cb.0
                ));
            }
        }
    }

    // Inside the lock, after `on_enter` had its chance to roll the write back,
    // so no caller can apply a transition and forget the ledger or the result.
    // §FS-rhei-complete.3 §FS-rhei-transition-cmd.3
    let record = record_transition_result(
        files.artifact_root,
        files.task_file,
        task_id_str,
        machine,
        files.artifact_id,
        from,
        to,
        recorded_message.as_deref(),
    );

    if let Some(task_handle) = task_handle {
        let _ = fs2::FileExt::unlock(&task_handle);
    }
    let _ = fs2::FileExt::unlock(&metadata_handle);
    record?;
    Ok(to.to_string())
}

/// Extract the current state and node-policy inputs for a task from raw
/// markdown content.
///
/// Tries full-plan parsing first, falls back to workspace task-file parsing.
fn find_task_transition_info(
    raw: &str,
    file_path: &Path,
    workspace_structure: Option<&rhei_core::ast::Structure>,
    target_id: &TaskId,
    task_id_str: &str,
) -> MietteResult<TransitionTaskInfo> {
    // Try full plan parse.
    if let Ok(rhei) = rhei_core::parse(raw) {
        if let Some(task) = find_task_by_id(&rhei.tasks, target_id) {
            return Ok(TransitionTaskInfo {
                task: task.clone(),
                level: task.id.depth() as u8,
            });
        }
    }

    // Try workspace task-file parse.
    let workspace_tasks = match workspace_structure {
        Some(structure) => rhei_core::parser::parse_workspace_tasks_with_structure(raw, structure),
        None => rhei_core::parser::parse_workspace_tasks(raw),
    };
    if let Ok(tasks) = workspace_tasks {
        if let Some(task) = find_task_by_id(&tasks, target_id) {
            return Ok(TransitionTaskInfo {
                task: task.clone(),
                level: task.id.depth() as u8,
            });
        }
    }

    Err(miette!(
        help = task_id_help(),
        "task '{}' not found in {}", task_id_str, file_path.display()
    ))
}

// ─── Agent Configuration ──────────────────────────────────────────────
