/// Add a template directory to the reusable template library so it can later be
/// instantiated by name with `rhei instantiate <name>`. §FS-rhei-templates.6.3
pub(super) fn add_template_command(
    source: &str,
    project: bool,
    link: bool,
    force: bool,
) -> MietteResult<()> {
    let source_dir = PathBuf::from(source);
    if !source_dir.is_dir() {
        return Err(miette!("template source '{}' is not a directory", source_dir.display()));
    }

    // Validate that the source is a real template: the manifest must load (which
    // also enforces that the manifest `name` matches the directory name) and the
    // plan layout (single-file vs directory workspace) must be detectable.
    let manifest = load_template_manifest(&source_dir)?;
    detect_template_layout(&source_dir)?;

    // The manifest `name` is the template identity used by discovery and
    // instantiation, so the library entry must be registered under it verbatim.
    let template_name = manifest.name.as_str();

    let (root, scope) = if project {
        (find_project_root()?.join(".agents").join("rhei").join("templates"), "project")
    } else {
        (home_dir()?.join(".agents").join("rhei").join("templates"), "user")
    };
    let dest = root.join(template_name);

    let canonical_source = source_dir.canonicalize().map_err(|err| {
        miette!("failed to resolve template source '{}': {err}", source_dir.display())
    })?;

    if dest.symlink_metadata().is_ok() {
        if dest.canonicalize().ok().as_deref() == Some(canonical_source.as_path()) {
            return Err(miette!(
                "template '{}' already refers to this source ('{}')",
                template_name,
                canonical_source.display()
            ));
        }
        if !force {
            // §FS-rhei-templates.6.3: refuse to clobber an existing library entry.
            return Err(miette!(
                "template '{}' already exists at '{}'. Pass --force to overwrite.",
                template_name,
                dest.display()
            ));
        }
        remove_path(&dest, false)?;
    }

    fs::create_dir_all(&root)
        .map_err(|err| miette!("failed to create template root '{}': {err}", root.display()))?;

    if link {
        // An absolute symlink keeps the library entry tracking the live source.
        link_skill(&canonical_source, &dest, false)?;
    } else {
        copy_dir_recursive(&canonical_source, &dest)?;
    }

    let verb = if link { "Linked" } else { "Added" };
    println!("{verb} template '{template_name}' to the {scope} library.");
    println!("  {}", dest.display());
    println!();
    println!("Instantiate it with:");
    println!("  rhei instantiate {template_name}");

    Ok(())
}
