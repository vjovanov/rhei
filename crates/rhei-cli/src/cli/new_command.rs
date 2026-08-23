// `rhei new` — create a rhei under Panta, or a ticket inside one with
// `--under`. The two modes share everything after the decision of *what* to
// write: the write itself, the validation that follows it, the rollback when
// that fails, and the report.

// §FS-rhei-new

/// A decided create: the path, the exact bytes, and what to say about it.
/// Nothing is written until [`apply_new_write`] runs, which is what makes
/// `--dry-run` an early return rather than a second code path.
// §FS-rhei-new.5
struct NewWrite {
    /// `rhei` or `ticket` — the word the report and `--json` use.
    kind: &'static str,
    /// Project-wide id of what was created (`billing`, `auth.4`).
    id: String,
    title: String,
    path: PathBuf,
    /// The new ticket's state; `None` for a rhei, which has none.
    state: Option<String>,
    /// Full contents to write to `path`.
    contents: String,
    /// What a reader cares to see: a whole new file, or just the inserted
    /// ticket. §FS-rhei-new.5.4
    preview: String,
    /// Directories that must exist first, outermost first.
    dirs: Vec<PathBuf>,
    /// The command that naturally follows, when there is one.
    next_hint: Option<String>,
    /// What the created node will do that the flags do not say on their face,
    /// one line each. §FS-rhei-new.5.4
    notes: Vec<String>,
}

fn new_command(options: &NewOptions) -> MietteResult<()> {
    reject_unusable_title(&options.title)?;
    reject_mode_confusion(options)?;
    let description = resolve_new_description(options)?;
    // A member rhei widens to the project it belongs to, exactly as every
    // other command resolves its target: only there do `--under basin` and a
    // cross-rhei `**Prior:**` resolve. §FS-rhei-new.1.1 §FS-rhei-new.2.1
    let resolved = resolve_plan_target(options.project.clone())?;
    // Prose on stdout, and `--json` promises one object there. §FS-rhei-new.5.4
    if !options.json {
        report_new_widened(&resolved);
    }
    let target = resolved.path().to_path_buf();
    // Held for the whole invocation: numbering, writing, verifying, and rolling
    // back all touch the same file, so a narrower lock still loses tickets.
    // §FS-rhei-new.4
    let _create_lock = lock_new_create(&target)?;

    let write = match options.under.as_deref() {
        Some(parent) => new_ticket_write(&target, options, parent, description.as_deref())?,
        None => new_rhei_write(&target, options, description.as_deref())?,
    };

    if options.dry_run {
        report_new_dry_run(&write, options.json);
        return Ok(());
    }
    apply_new_write(&target, &write, options.keep_on_error)?;
    report_new_write(&write, options.json);
    Ok(())
}

/// Refuse a `TITLE` that cannot become a heading.
///
/// The title is written into a `# Rhei:` or `### Task 4:` line, so an empty one
/// produces a heading with nothing after the colon and an embedded newline
/// produces two lines where the parser expects one. Both come back as a parse
/// error with a code frame pointing into a file that the rollback has since
/// removed — the failure a description check exists to prevent, one argument
/// over.
// §FS-rhei-new.3.4
fn reject_unusable_title(title: &str) -> MietteResult<()> {
    if title.trim().is_empty() {
        return Err(miette!(
help = "give the thing a name: rhei new \"Rotate signing keys\" --under auth.",

            "TITLE is empty: `rhei new` writes it into the heading it creates, and a heading \
             with nothing after the colon is not a node the plan language can read"
        ));
    }
    if title.contains('\n') || title.contains('\r') {
        return Err(miette!(
help = "keep the title to one line and put the rest in --description, which is written under the heading as prose.",

            "TITLE runs over more than one line: a node's title is the rest of its heading \
             line, so the second line would be read as plan content rather than as part of \
             the title"
        ));
    }
    Ok(())
}

/// Refuse a flag that belongs to the mode the invocation is not in.
///
/// Ignoring it would be worse than failing: the command would report success
/// and the field the user asked for would simply not be there.
// §FS-rhei-new.5.3
fn reject_mode_confusion(options: &NewOptions) -> MietteResult<()> {
    let rhei_flags = [
        ("--dir", options.dir),
        ("--states", options.states.is_some()),
        ("--max-levels", options.max_levels.is_some()),
        ("--node-kinds", !options.node_kinds.is_empty()),
    ];
    let ticket_flags = [
        ("--kind", options.kind.is_some()),
        ("--state", options.state.is_some()),
        ("--prior", !options.prior.is_empty()),
        ("--provides", !options.provides.is_empty()),
        ("--consumes", !options.consumes.is_empty()),
        ("--assignee", options.assignee.is_some()),
        ("--model", options.model.is_some()),
        ("--target", options.target.is_some()),
    ];

    if options.under.is_some() {
        if let Some((flag, _)) = rhei_flags.into_iter().find(|(_, given)| *given) {
            return Err(miette!(
help = "`--under` creates a ticket; drop it to create a rhei, or drop the rhei-only flag.",

                "{flag} configures a new rhei, but --under creates a ticket"
            ));
        }
        return Ok(());
    }
    if let Some((flag, _)) = ticket_flags.into_iter().find(|(_, given)| *given) {
        return Err(miette!(
help = "name where the ticket goes, for example: --under <rhei-id>.",

            "{flag} configures a new ticket, but without --under a rhei is created"
        ));
    }
    Ok(())
}

/// Explain that a create aimed at a member rhei writes into its whole project.
///
/// `rhei validate`'s wording ends "validating the whole project", which is a
/// sentence about a command the user did not run; a create has to say what it
/// is about to do instead.
// §FS-rhei-new.5.4 §FS-rhei-validate.1.1
fn report_new_widened(target: &PlanTarget) {
    let Some(id) = target.implied_scope.first() else {
        return;
    };
    println!(
        "Scope: rhei '{id}' belongs to the project at {}, and its state machine, settings, and \
         cross-rhei **Prior:** resolve only there — creating into that project.",
        display_path(target.path())
    );
}

/// Write the create, then decide whether it succeeded: the errors it added to
/// the ones the project already had, and whether the new id reads back. The
/// write is undone when it did not, unless `--keep-on-error`.
// §FS-rhei-new.5.1 §FS-rhei-new.5.2
fn apply_new_write(target: &Path, write: &NewWrite, keep_on_error: bool) -> MietteResult<()> {
    // The pass runs before the write as well, so what follows it can be read as
    // a difference rather than as a verdict on the whole project.
    // §FS-rhei-new.5.2
    let inherited = create_validation_errors(target);
    // The ids the project already holds, so a write that made one of them stop
    // existing can be undone whatever splice produced the loss.
    // §FS-rhei-new.5.1
    let before = create_plan_ids(target);

    let previous = fs::read_to_string(&write.path).ok();
    let created_dirs: Vec<PathBuf> =
        write.dirs.iter().filter(|dir| !dir.exists()).cloned().collect();
    for dir in &write.dirs {
        fs::create_dir_all(dir).map_err(|err| file_io_report(dir, "failed to create", err))?;
    }
    write_plan_file_atomically(&write.path, &write.contents)?;

    // Warnings are deliberately dropped: a rhei created seconds ago holding no
    // tickets is exactly what was asked for, and saying so here would make the
    // normal path noisier than the failing one. §FS-rhei-new.5.1
    let Some(failure) = new_write_failure(target, write, &inherited, before.as_ref()) else {
        report_inherited_validation_failure(&inherited);
        return Ok(());
    };
    if keep_on_error {
        eprintln!(
            "warning: kept {} — the project is left failing validation",
            display_path(&write.path)
        );
        return Err(failure.report);
    }
    roll_back_new_write(&write.path, previous.as_deref(), &created_dirs);
    // Say it before the validator's own report: a create that reports only a
    // validation error reads as though something half-landed. §FS-rhei-new.5.2
    eprintln!(
        "note: nothing was written — the create was rolled back because {}. Re-run with \
         `--keep-on-error` to inspect it.",
        failure.reason
    );
    Err(failure.report)
}

/// Say that the project was failing validation before this create, and that the
/// failure is not the create's.
///
/// Kept as a warning rather than an error: `rhei new` is the on-ramp, and a
/// project with one broken rhei is exactly the project someone is adding a
/// working one to. Silence would be worse than noise here — the next command
/// will fail, and the user would read that failure as this create's doing.
// §FS-rhei-new.5.2
fn report_inherited_validation_failure(inherited: &[String]) {
    if inherited.is_empty() {
        return;
    }
    let count = inherited.len();
    let noun = if count == 1 { "error" } else { "errors" };
    eprintln!(
        "warning: the project was already failing validation before this create \
         ({count} {noun}), and it fails the same way after it — the write is kept, and \
         those errors are not this create's. Run `rhei validate` to see them."
    );
}

/// Undo a write: restore a modified file byte-for-byte, remove a created one,
/// and remove the directories this create made, deepest first. Best effort by
/// design — the validation error is what the user needs to see, and a failed
/// cleanup must not replace it.
// §FS-rhei-new.5.2
fn roll_back_new_write(path: &Path, previous: Option<&str>, created_dirs: &[PathBuf]) {
    match previous {
        Some(previous) => {
            let _ = fs::write(path, previous);
        }
        None => {
            let _ = fs::remove_file(path);
        }
    }
    for dir in created_dirs.iter().rev() {
        let _ = fs::remove_dir(dir);
    }
}

/// Report a `--dry-run`. Under `--json` it is the same object the real create
/// emits, plus the fact that nothing was written and the block that would have
/// been: a flag that selects the output format keeps working under a flag that
/// only selects whether the write happens.
// §FS-rhei-new.5.4
fn report_new_dry_run(write: &NewWrite, json: bool) {
    if json {
        let mut value = new_write_json(write);
        value["dry_run"] = serde_json::Value::Bool(true);
        value["markdown"] = serde_json::Value::String(write.preview.clone());
        println!("{value}");
        return;
    }
    println!("Would create {} {} at {}", write.kind, write.id, display_path(&write.path));
    println!();
    print!("{}", write.preview);
}

/// The facts `--json` reports, shared by the real create and the dry run so the
/// two can never drift. §FS-rhei-new.5.4
fn new_write_json(write: &NewWrite) -> serde_json::Value {
    let mut value = serde_json::json!({
        "kind": write.kind,
        "id": write.id,
        "title": write.title,
        "path": display_path(&write.path),
    });
    if let Some(state) = &write.state {
        value["state"] = serde_json::Value::String(state.clone());
    }
    value
}

fn report_new_write(write: &NewWrite, json: bool) {
    if json {
        println!("{}", new_write_json(write));
        return;
    }
    match &write.state {
        Some(state) => println!(
            "Created ticket {} \"{}\" [{}] in {}",
            write.id,
            write.title,
            state,
            display_path(&write.path)
        ),
        None => println!(
            "Created rhei \"{}\" as `{}` at {}",
            write.title,
            write.id,
            display_path(&write.path)
        ),
    }
    for note in &write.notes {
        println!("Note: {note}");
    }
    if let Some(hint) = &write.next_hint {
        println!("Next: {hint}");
    }
}
