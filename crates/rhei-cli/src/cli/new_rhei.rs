// `rhei new <title>` with no `--under`: a new rhei under Panta. §FS-rhei-new.2

/// Decide the file (or workspace) a new rhei becomes. Nothing is written here.
fn new_rhei_write(
    target: &Path,
    options: &NewOptions,
    description: Option<&str>,
) -> MietteResult<NewWrite> {
    // A rhei is a *member* of a project; a lone plan has nowhere to put a
    // second one. §FS-rhei-new.2.1
    let Some(project_dir) = workspace::panta_project_dir(target) else {
        return Err(miette!(
help = "create a project first: `rhei init` writes index.panta.md, and `rhei new` fills it.",

            "{} is not a Panta project, so there is nowhere to add a rhei: a project is a \
             directory holding {}, and its rheis live beside that manifest",
            display_path(target),
            workspace::PANTA_INDEX_FILE
        ));
    };
    let id = resolve_new_rhei_id(&options.title, options.id.as_deref())?;
    reject_existing_rhei(&project_dir, &id)?;

    let header = RheiHeader {
        title: &options.title,
        states: options.states.as_deref(),
        max_levels: options.max_levels,
        node_kinds: &options.node_kinds,
        description,
    };

    let (path, contents, dirs) = if options.dir {
        let rhei_dir = project_dir.join(&id);
        let tasks_dir = rhei_dir.join("tasks");
        // The workspace index carries no `## Tasks`: its tickets live in
        // `tasks/` files. §FS-rhei-plan-language.1.2
        let contents = render_rhei_file(&header, false);
        (rhei_dir.join("index.rhei.md"), contents, vec![rhei_dir, tasks_dir])
    } else {
        let contents = render_rhei_file(&header, true);
        (project_dir.join(format!("{id}.rhei.md")), contents, Vec::new())
    };

    Ok(NewWrite {
        kind: "rhei",
        id: id.clone(),
        title: options.title.trim().to_string(),
        path,
        state: None,
        preview: contents.clone(),
        contents,
        dirs,
        next_hint: Some(format!("`rhei new \"<first ticket>\" --under {id}`")),
        notes: Vec::new(),
    })
}

/// Refuse an id already taken by a rhei entry, in either layout. Discovery is
/// filesystem-based, so the filesystem is what has to be free. §FS-rhei-new.4
fn reject_existing_rhei(project_dir: &Path, id: &str) -> MietteResult<()> {
    let file = project_dir.join(format!("{id}.rhei.md"));
    let dir = project_dir.join(id);
    let taken = if file.is_file() {
        Some(file)
    } else if dir.is_dir() {
        Some(dir)
    } else {
        None
    };
    match taken {
        Some(path) => Err(miette!(
help = "pick another id with --id, or add a ticket to the existing rhei with `--under`.",

            "rhei '{id}' already exists at {}",
            display_path(&path)
        )),
        None => Ok(()),
    }
}
