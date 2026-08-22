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
}

fn new_command(options: &NewOptions) -> MietteResult<()> {
    reject_mode_confusion(options)?;
    let description = resolve_new_description(options)?;
    let target = resolve_plan_path(options.project.clone())?;

    let write = match options.under.as_deref() {
        Some(parent) => new_ticket_write(&target, options, parent, description.as_deref())?,
        None => new_rhei_write(&target, options, description.as_deref())?,
    };

    if options.dry_run {
        report_new_dry_run(&write);
        return Ok(());
    }
    apply_new_write(&target, &write, options.keep_on_error)?;
    report_new_write(&write, options.json);
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

/// The description body, from `--description` or `--description-file` (`-`
/// reads standard input). §FS-rhei-new.1.1
fn resolve_new_description(options: &NewOptions) -> MietteResult<Option<String>> {
    if let Some(description) = &options.description {
        return Ok(Some(description.clone()));
    }
    let Some(path) = &options.description_file else {
        return Ok(None);
    };
    if path.as_os_str() == "-" {
        let mut body = String::new();
        std::io::stdin()
            .read_to_string(&mut body)
            .map_err(|err| miette!(
help = "`--description-file -` reads the description from standard input; pipe it in, or pass a path.",
                "failed to read the description from standard input: {err}"))?;
        return Ok(Some(body));
    }
    let body = fs::read_to_string(path).map_err(|err| file_io_report(path, "failed to read", err))?;
    Ok(Some(body))
}

/// Write the create, then validate the project it landed in. A create that
/// leaves the project unloadable has not succeeded, so the write is undone
/// unless `--keep-on-error`. §FS-rhei-new.5.1 §FS-rhei-new.5.2
fn apply_new_write(target: &Path, write: &NewWrite, keep_on_error: bool) -> MietteResult<()> {
    let previous = fs::read_to_string(&write.path).ok();
    let created_dirs: Vec<PathBuf> =
        write.dirs.iter().filter(|dir| !dir.exists()).cloned().collect();
    for dir in &write.dirs {
        fs::create_dir_all(dir).map_err(|err| file_io_report(dir, "failed to create", err))?;
    }
    fs::write(&write.path, &write.contents)
        .map_err(|err| file_io_report(&write.path, "failed to write", err))?;

    // Warnings are deliberately dropped: a rhei created seconds ago holding no
    // tickets is exactly what was asked for, and saying so here would make the
    // normal path noisier than the failing one. §FS-rhei-new.5.1
    let Err(err) = validation_warnings_or_error(target, None) else {
        return Ok(());
    };
    if keep_on_error {
        eprintln!(
            "warning: kept {} — the project is left failing validation",
            display_path(&write.path)
        );
        return Err(err);
    }
    roll_back_new_write(&write.path, previous.as_deref(), &created_dirs);
    // Say it before the validator's own report: a create that reports only a
    // validation error reads as though something half-landed. §FS-rhei-new.5.2
    eprintln!(
        "note: nothing was written — the create was rolled back because the project would \
         not validate with it. Re-run with `--keep-on-error` to inspect it."
    );
    Err(err)
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

fn report_new_dry_run(write: &NewWrite) {
    println!("Would create {} {} at {}", write.kind, write.id, display_path(&write.path));
    println!();
    print!("{}", write.preview);
}

fn report_new_write(write: &NewWrite, json: bool) {
    if json {
        let mut value = serde_json::json!({
            "kind": write.kind,
            "id": write.id,
            "title": write.title,
            "path": display_path(&write.path),
        });
        if let Some(state) = &write.state {
            value["state"] = serde_json::Value::String(state.clone());
        }
        println!("{value}");
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
    if let Some(hint) = &write.next_hint {
        println!("Next: {hint}");
    }
}
