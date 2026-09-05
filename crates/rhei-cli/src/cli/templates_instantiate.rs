    #[allow(clippy::too_many_arguments)]
    pub(super) fn instantiate_command(
        template: Option<&str>,
        input_args: &[String],
        execute_args: &[String],
        set_values: &[String],
        set_files: &[String],
        values_files: &[PathBuf],
        output: Option<&Path>,
        execute: bool,
        dry_run: bool,
        keep_on_error: bool,
        list_inputs: bool,
    ) -> MietteResult<()> {
        if execute && dry_run {
            return Err(miette!(
                help = "--dry-run renders and validates without writing anything, so there is \
                        nothing for --execute to run. Drop one of them.",
                "--execute cannot be used together with --dry-run"
            ));
        }

        let Some(template) = template else {
            // §FS-rhei-templates.6.1.2: an omitted template lists available templates.
            return templates_command(false, "all", None);
        };

        let resolved_template = resolve_template_reference(template)?;
        let template_dir = resolved_template.path();
        let manifest = load_template_manifest(template_dir)?;

        if list_inputs {
            print_template_inputs(&manifest, template);
            return Ok(());
        }

        let layout = detect_template_layout(template_dir)?;
        let template_input_args =
            template_input_args_without_execute_args(input_args, execute_args)?;
        let resolved_values = collect_template_inputs(
            &manifest,
            template,
            values_files,
            &template_input_args,
            set_values,
            set_files,
        )?;
        let cwd = std::env::current_dir()
            .map_err(|err| {
                miette!(
                    help = cwd_help(),
                    "failed to determine working directory: {err}"
                )
            })?;
        let template_name = template_dir.file_name().ok_or_else(|| {
            miette!(
                help = "pass an explicit destination with --output <dir>",
                "template path '{}' has no directory name",
                template_dir.display()
            )
        })?;
        // Inside a project the default home is the project itself; defaulting
        // to the working directory dropped the workspace where discovery never
        // looks, so no command listed it. §FS-rhei-templates.6.2
        let default_output =
            enclosing_project_for_new_rhei(&cwd).unwrap_or(cwd).join(template_name);
        let explicit_output = output.is_some();
        let output_dir = output.map(Path::to_path_buf).unwrap_or(default_output);

        if !dry_run && output_dir.exists() {
            return Err(instantiate_output_exists_error(
                &output_dir,
                template,
                &template_input_args,
                explicit_output,
            ));
        }

        let scratch = if dry_run {
            Some(
                tempfile::tempdir()
                    .map_err(|err| miette!(
                        help = "--dry-run renders into a temp directory. Check that $TMPDIR exists and is writable.",
                        "failed to create temporary output directory: {err}"
                    ))?,
            )
        } else {
            None
        };
        let target_dir = scratch
            .as_ref()
            .map(|dir| dir.path().join("instantiate-output"))
            .unwrap_or_else(|| output_dir.clone());

        // §FS-rhei-errors.4: a --dry-run target is scratch space the user never
        // chose and never sees, so failures there name template-relative paths.
        let materialized =
            match materialize_template(
                template_dir,
                layout,
                &target_dir,
                &resolved_values,
                dry_run,
            ) {
                Ok(materialized) => materialized,
                Err(err) => {
                    if !dry_run {
                        let _ = remove_path(&target_dir, false);
                    }
                    return Err(err);
                }
            };

        let entrypoint = materialized.entrypoint();
        let state_machine_path = materialized.state_machine_path();

        // Place relative to the owning project before validating. The
        // template's machine needs no reconciling: a member rhei's own
        // declaration overrides the project default. §FS-rhei-templates.6.2

        // The hoist below runs before validation, so whatever discards the
        // output takes the hoist with it. §FS-rhei-templates.6.2
        let discard_output = |hoisted: Option<&HoistedSettings>| {
            if !dry_run && !keep_on_error {
                let _ = remove_path(&target_dir, false);
                if let Some(hoisted) = hoisted {
                    hoisted.undo();
                }
            }
        };
        let placement = match plan_project_placement(&output_dir, &materialized.output_dir) {
            Ok(placement) => placement,
            Err(err) => {
                discard_output(None);
                return Err(err);
            }
        };

        let mut hoisted_settings = None;
        if !dry_run {
            if let Some(project) = placement.project() {
                match hoist_workspace_settings_into_project(&materialized.output_dir, project) {
                    Ok(hoisted) => hoisted_settings = hoisted,
                    Err(err) => {
                        discard_output(None);
                        return Err(err);
                    }
                }
            }
        }

        // A member rhei is only correct in the project's terms — its machine and
        // settings resolve there — so that is what gets validated. Validating
        // the workspace in isolation is what let a project-breaking result be
        // reported as "Validation succeeded".
        let validation = match placement.project() {
            Some(project) if !dry_run => run_validation_once(project, None),
            _ => run_validation_once(&entrypoint, state_machine_path.as_deref()),
        };
        if let Err(err) = validation {
            discard_output(hoisted_settings.as_ref());
            return Err(err);
        }

        if dry_run {
            println!(
                "Dry run OK: '{}' would be instantiated into '{}'.",
                manifest.name,
                display_path(&output_dir).display()
            );
            print_instantiated_workspace_summary(
                &materialized,
                &output_dir,
                state_machine_path.as_deref(),
                true,
            )?;
            print_template_instantiation_command(
                template,
                &template_input_args,
                set_values,
                set_files,
                values_files,
                &output_dir,
            );
            return Ok(());
        }

        println!(
            "Instantiated template '{}' into '{}'.",
            manifest.name,
            display_path(&output_dir).display()
        );
        report_project_placement(&placement, hoisted_settings.as_ref());
        if matches!(placement, ProjectPlacement::Standalone) {
            report_standalone_versioning(&output_dir);
        }
        print_instantiated_workspace_summary(
            &materialized,
            &output_dir,
            state_machine_path.as_deref(),
            false,
        )?;
        print_template_instantiation_command(
            template,
            &template_input_args,
            set_values,
            set_files,
            values_files,
            &output_dir,
        );

        if execute {
            let opts = parse_execute_run_options(&entrypoint, execute_args)?;
            return run_command(&entrypoint, state_machine_path.as_deref(), opts);
        }

        Ok(())
    }

    /// A standalone workspace inside a git repository is tracked content
    /// unless the user says otherwise — unlike `panta/`, which init ignores.
    // §FS-rhei-templates.6.2: note the untracked workspace; never edit
    // `.gitignore` — committed workspaces (examples) are the other use.
    fn report_standalone_versioning(output_dir: &Path) {
        let parent = match output_dir.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent,
            _ => Path::new("."),
        };
        let toplevel = std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(parent)
            .output();
        let Ok(toplevel) = toplevel else {
            return; // no git on PATH: nothing worth guessing about
        };
        if !toplevel.status.success() {
            return; // not inside a git repository
        }
        let ignored = std::process::Command::new("git")
            .args(["check-ignore", "-q", "--"])
            .arg(output_dir.file_name().unwrap_or_default())
            .current_dir(parent)
            .status();
        match ignored {
            Ok(status) if status.code() == Some(1) => {}
            _ => return, // already ignored, or git could not tell
        }
        let repo_root = PathBuf::from(String::from_utf8_lossy(&toplevel.stdout).trim());
        let entry = rhei_core::platform::canonical_path(output_dir)
            .ok()
            .and_then(|dir| dir.strip_prefix(&repo_root).map(|rel| rel.to_path_buf()).ok())
            .unwrap_or_else(|| output_dir.to_path_buf());
        println!(
            "Note: this standalone workspace is inside a git repository and is not gitignored, \
             so its planning state (including `runtime/`) is repository content. Commit it to \
             version the workspace, or add `{}/` to .gitignore to keep it working material — \
             the stance `rhei init` takes for `panta/`.",
            entry.display()
        );
    }

    /// Render `path` for the report: relative to the working directory when it
    /// sits inside it. The report is read — and its commands pasted — from that
    /// directory, so `panta/product-management` beats the absolute spelling.
    // §FS-rhei-templates.6.1.3: report paths inside the working directory are relative.
    pub(super) fn display_path(path: &Path) -> PathBuf {
        if path.is_relative() {
            return path.to_path_buf();
        }
        let Ok(cwd) = std::env::current_dir() else {
            return path.to_path_buf();
        };
        if let Some(relative) = strip_to_relative(path, &cwd) {
            return relative;
        }
        // `current_dir` reports the resolved path, so a symlinked parent makes
        // the comparison above miss and the report falls back to absolute
        // spelling for a path that is in fact inside the working directory —
        // macOS resolves `/tmp` and `/var` into `/private`, so every report
        // written from a temporary directory there took that fallback.
        let (Ok(resolved_path), Ok(resolved_cwd)) =
            (rhei_core::platform::canonical_path(path), rhei_core::platform::canonical_path(&cwd))
        else {
            return path.to_path_buf();
        };
        strip_to_relative(&resolved_path, &resolved_cwd).unwrap_or_else(|| path.to_path_buf())
    }

    /// `path` expressed relative to `base`, or `None` when it is not under it.
    fn strip_to_relative(path: &Path, base: &Path) -> Option<PathBuf> {
        match path.strip_prefix(base) {
            Ok(rel) if rel.as_os_str().is_empty() => Some(PathBuf::from(".")),
            Ok(rel) => Some(rel.to_path_buf()),
            Err(_) => None,
        }
    }

    /// Say what joining a project did to it. The hoist moves a settings file —
    /// a write outside the output directory, so it may not happen silently.
    /// §FS-rhei-templates.6.2
    fn report_project_placement(placement: &ProjectPlacement, hoisted: Option<&HoistedSettings>) {
        let Some(project) = placement.project() else {
            return;
        };
        println!("Added to the Panta project at {}.", display_path(project).display());
        if let Some(hoisted) = hoisted {
            println!(
                "  Merged the template's agent settings into {}.",
                display_path(&project_settings_write_path(project)).display()
            );
            if !hoisted.added.is_empty() {
                println!("    added: {}", hoisted.added.join(", "));
            }
            if !hoisted.kept.is_empty() {
                println!(
                    "    kept your existing values for: {} (the template's differ)",
                    hoisted.kept.join(", ")
                );
            }
        }
    }

    fn template_input_args_without_execute_args(
        input_args: &[String],
        execute_args: &[String],
    ) -> MietteResult<Vec<String>> {
        if execute_args.is_empty() {
            return Ok(input_args.to_vec());
        }
        if input_args.len() < execute_args.len() {
            return Err(miette!(
                help = internal_error_help(),
                "internal error: execute arguments were not present in parsed template inputs"
            ));
        }

        let split_at = input_args.len() - execute_args.len();
        if input_args[split_at..] != *execute_args {
            return Err(miette!(
                help = internal_error_help(),
                "internal error: execute arguments did not match trailing parsed template inputs"
            ));
        }
        Ok(input_args[..split_at].to_vec())
    }

    fn parse_execute_run_options(
        entrypoint: &Path,
        execute_args: &[String],
    ) -> MietteResult<RunOptions> {
        if execute_args.is_empty() {
            return Ok(default_run_options());
        }

        let mut args =
            vec!["rhei".to_string(), "run".to_string(), entrypoint.display().to_string()];
        args.extend(execute_args.iter().cloned());

        let cli = Cli::try_parse_from(args).map_err(|err| {
            miette!(
                help = "everything after --execute is passed to `rhei run`. See its flags with: \
                        rhei run --help",
                "{}",
                err.to_string()
            )
        })?;
        let Commands::Run { standalone, agent, program, snapshot, .. } = cli.command else {
            return Err(miette!(
                help = internal_error_help(),
                "internal error: execute arguments did not parse as run options"
            ));
        };
        Ok((standalone, agent, program, snapshot).into())
    }

    fn print_instantiated_workspace_summary(
        materialized: &MaterializedTemplate,
        display_output_dir: &Path,
        state_machine_path: Option<&Path>,
        dry_run: bool,
    ) -> MietteResult<()> {
        let entrypoint = materialized.entrypoint();
        let mut loaded = load_plan(&entrypoint)?;
        let resolved =
            resolve_state_machines_for_loaded_plan(&entrypoint, &loaded, state_machine_path)?;
        // A member loads through its project (§FS-rhei-panta.6), but the
        // summary reports what *this instantiation* created — narrow to the
        // new rhei when siblings are present. §FS-rhei-templates.6.1.3
        let entry_rhei_id = materialized
            .output_dir
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        if loaded.rhei_ids.len() > 1 && loaded.rhei_ids.contains(&entry_rhei_id) {
            loaded
                .rhei
                .tasks
                .retain(|task| task.id.to_string().starts_with(&format!("{entry_rhei_id}.")));
        }
        let machine = resolved
            .per_rhei
            .get(&entry_rhei_id)
            .map(|entry| entry.machine.clone())
            .unwrap_or_else(|| resolved.default.machine.clone());
        let resolved = ResolvedStateMachine { machine, path: resolved.default.path.clone() };
        let tasks = flatten_tasks(&loaded.rhei);

        let display_output_dir = display_path(display_output_dir);
        println!();
        println!("=== Instantiation Summary ===");
        println!("Output: {}", display_output_dir.display());
        println!("Tasks: {}", tasks.len());
        println!("States: {}", format_state_counts(&loaded.rhei));
        println!();

        println!("Files:");
        println!("  {}/", display_output_dir.display());
        print_output_tree(&materialized.output_dir, "  ")?;

        println!();
        println!("Task tree:");
        for task in &loaded.rhei.tasks {
            print_task_tree(task, 1);
        }

        println!();
        println!("Recent task definitions:");
        let last_task_count = tasks.len().min(5);
        for (index, task) in
            tasks.iter().skip(tasks.len().saturating_sub(last_task_count)).enumerate()
        {
            if index > 0 {
                println!();
            }
            println!("--- {} ---", format_task_summary_line(task));
            println!("{}", render_task_definition(task));
        }

        println!();
        println!("Stopped:");
        println!(
            "  {}",
            describe_instantiation_stop(&loaded.rhei, &resolved.machine, &entrypoint, dry_run)
        );

        Ok(())
    }

    fn print_output_tree(root: &Path, prefix: &str) -> MietteResult<()> {
        let mut entries = fs::read_dir(root)
            .map_err(|err| file_io_report(root, "failed to read instantiated output tree", err))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| miette!(
                help = "check that the instantiated output directory is readable.",
                "failed to read dir entry in '{}': {err}", root.display()
            ))?;
        entries.sort_by_key(|entry| entry.file_name());

        let count = entries.len();
        for (idx, entry) in entries.into_iter().enumerate() {
            let path = entry.path();
            let file_type = entry.file_type().map_err(|err| {
                file_io_report(&path, "failed to read instantiated output entry", err)
            })?;
            let is_last = idx + 1 == count;
            let connector = if is_last { "`-- " } else { "|-- " };
            let child_prefix = if is_last { "    " } else { "|   " };
            let name = entry.file_name().to_string_lossy().to_string();

            if file_type.is_dir() {
                println!("{prefix}{connector}{name}/");
                print_output_tree(&path, &format!("{prefix}{child_prefix}"))?;
            } else {
                println!("{prefix}{connector}{name}");
            }
        }

        Ok(())
    }

    fn print_task_tree(task: &rhei_core::ast::Task, depth: usize) {
        println!("{}- {}", "  ".repeat(depth), format_task_summary_line(task));
        for child in &task.children {
            print_task_tree(child, depth + 1);
        }
    }

    fn format_task_summary_line(task: &rhei_core::ast::Task) -> String {
        format!("{} {}: {} [{}]", title_case_kind(&task.kind), task.id, task.title, task.state)
    }

    fn render_task_definition(task: &rhei_core::ast::Task) -> String {
        // Heading level mirrors the on-disk plan, where headings are
        // rhei-local: the qualification segment adds no nesting.
        let heading_level = usize::from(task.profile_level()).saturating_add(2).max(3);
        let mut lines = vec![
            format!(
                "{} {} {}: {}",
                "#".repeat(heading_level),
                title_case_kind(&task.kind),
                task.id,
                task.title
            ),
            format!("**State:** {}", task.state),
        ];

        if !task.prior.is_empty() {
            // Echo each reference as authored: the kind keyword belongs to the
            // referenced node, so inventing `Task` here would misprint plans
            // with custom node kinds. §FS-rhei-plan-language.3.1
            let priors = task
                .prior
                .iter()
                .enumerate()
                .map(|(position, id)| {
                    match task.prior_kinds.get(position).and_then(|k| k.as_deref()) {
                        Some(kind) => format!("{} {id}", title_case_kind(kind)),
                        None => id.to_string(),
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!("**Prior:** {priors}"));
        }

        if let Some(assignee) = task.assignee.as_deref() {
            lines.push(format!("**Assignee:** {assignee}"));
        }

        let content = task.content.trim();
        if !content.is_empty() {
            lines.push(String::new());
            lines.push(content.to_string());
        }

        lines.join("\n")
    }

    fn describe_instantiation_stop(
        rhei: &rhei_core::ast::Rhei,
        machine: &rhei_validator::StateMachine,
        entrypoint: &Path,
        dry_run: bool,
    ) -> String {
        let tasks = flatten_tasks(rhei);
        if dry_run {
            return "dry run stopped after rendering and validation; no files were written to the requested output path.".to_string();
        }
        if tasks.is_empty() {
            return "instantiation stopped after validation because the rendered workspace has no tasks.".to_string();
        }

        let terminal =
            tasks.iter().filter(|task| is_terminal_state(task.state.as_str(), machine)).count();
        if terminal == tasks.len() {
            return format!(
                "instantiation stopped with the plan already complete: {terminal}/{} tasks are terminal.",
                tasks.len()
            );
        }

        let gating = tasks
            .iter()
            .copied()
            .filter(|task| {
                let state = normalized_state_name(task.state.as_str(), machine);
                machine.states.get(&state).map(|def| def.gating).unwrap_or(false)
            })
            .collect::<Vec<_>>();
        if !gating.is_empty() {
            let labels = gating
                .iter()
                .take(3)
                .map(|task| format_task_summary_line(task))
                .collect::<Vec<_>>()
                .join(", ");
            let suffix = if gating.len() > 3 {
                format!(" (+{} more)", gating.len() - 3)
            } else {
                String::new()
            };
            return format!("instantiation stopped at a human gate: {labels}{suffix}.");
        }

        let ready = ready_tasks_from_flat(&tasks, machine);
        if let Some(task) = ready.first() {
            let target = display_path(entrypoint);
            return format!(
                "instantiation stopped before execution; next ready task is {}. Run `rhei run {}` or claim it with `rhei next {}`.",
                format_task_summary_line(task),
                target.display(),
                target.display()
            );
        }

        let blocked = blocked_tasks_from_flat(&tasks, machine);
        if !blocked.is_empty() {
            let labels = blocked
                .iter()
                .take(3)
                .map(|task| format_task_summary_line(task))
                .collect::<Vec<_>>()
                .join(", ");
            let suffix = if blocked.len() > 3 {
                format!(" (+{} more)", blocked.len() - 3)
            } else {
                String::new()
            };
            return format!("instantiation stopped with tasks blocked by incomplete prerequisites: {labels}{suffix}.");
        }

        "instantiation stopped after validation; no claimable task was found.".to_string()
    }

    fn ready_tasks_from_flat<'a>(
        tasks: &[&'a rhei_core::ast::Task],
        machine: &rhei_validator::StateMachine,
    ) -> Vec<&'a rhei_core::ast::Task> {
        let state_map: HashMap<&TaskId, String> = tasks
            .iter()
            .map(|task| (&task.id, normalized_state_name(task.state.as_str(), machine)))
            .collect();

        tasks
            .iter()
            .copied()
            .filter(|task| {
                let state = normalized_state_name(task.state.as_str(), machine);
                let gating = machine.states.get(&state).map(|def| def.gating).unwrap_or(false);
                !gating && !is_terminal_state(task.state.as_str(), machine)
            })
            .filter(|task| {
                task.prior.iter().all(|dep| {
                    state_map
                        .get(dep)
                        .map(|state| dependency_is_satisfied(state, machine))
                        .unwrap_or(false)
                })
            })
            .collect()
    }

    fn blocked_tasks_from_flat<'a>(
        tasks: &[&'a rhei_core::ast::Task],
        machine: &rhei_validator::StateMachine,
    ) -> Vec<&'a rhei_core::ast::Task> {
        let state_map: HashMap<&TaskId, String> = tasks
            .iter()
            .map(|task| (&task.id, normalized_state_name(task.state.as_str(), machine)))
            .collect();

        tasks
            .iter()
            .copied()
            .filter(|task| !is_terminal_state(task.state.as_str(), machine))
            .filter(|task| {
                task.prior.iter().any(|dep| {
                    !state_map
                        .get(dep)
                        .map(|state| dependency_is_satisfied(state, machine))
                        .unwrap_or(false)
                })
            })
            .collect()
    }

    fn print_template_instantiation_command(
        template: &str,
        input_args: &[String],
        set_values: &[String],
        set_files: &[String],
        values_files: &[PathBuf],
        output_dir: &Path,
    ) {
        println!("Instantiate this template with:");
        println!(
            "  {}",
            format_template_instantiation_command(
                template,
                input_args,
                set_values,
                set_files,
                values_files,
                Some(output_dir),
                &[],
            )
        );
    }

    /// Rebuild the `rhei instantiate` invocation the user made, plus any
    /// `extra_inputs`, so a suggestion pastes as-is. §FS-rhei-errors.1.2
    fn format_template_instantiation_command(
        template: &str,
        input_args: &[String],
        set_values: &[String],
        set_files: &[String],
        values_files: &[PathBuf],
        output_dir: Option<&Path>,
        extra_inputs: &[String],
    ) -> String {
        let mut parts = vec!["rhei".to_string(), "instantiate".to_string(), template.to_string()];
        for values_file in values_files {
            parts.push("--values".to_string());
            parts.push(values_file.display().to_string());
        }
        parts.extend(input_args.iter().cloned());
        parts.extend(extra_inputs.iter().cloned());
        for value in set_values {
            parts.push("--set".to_string());
            parts.push(value.clone());
        }
        for value in set_files {
            parts.push("--set-file".to_string());
            parts.push(value.clone());
        }
        if let Some(output_dir) = output_dir {
            parts.push("--output".to_string());
            parts.push(display_path(output_dir).display().to_string());
        }

        // §FS-rhei-errors.2: printed commands are pasted into a shell, and a
        // selector like `codex[yolo]:openai:gpt-5.5` is a zsh glob unquoted.
        shell_command(&parts)
    }

    /// The error for `rhei instantiate` when the output directory is taken.
    ///
    /// Instantiating the same template twice is the ordinary way to review two
    /// specs, audit two subjects, or run two release checklists, and it is the
    /// most likely second thing anyone does with a template. The bare
    /// `output path '…' already exists` was a dead end at exactly that moment:
    /// it named no fix, and the reason the collision happens — the default
    /// output directory is the template's own name, and a rhei's id *is* its
    /// directory name — is invisible from the message.
    // §FS-rhei-templates.6.2
    fn instantiate_output_exists_error(
        output_dir: &Path,
        template: &str,
        input_args: &[String],
        explicit_output: bool,
    ) -> Report {
        if explicit_output {
            return miette!(
                help = format!(
                    "pass a --output path that does not exist yet, or remove that one: rm -rf {}",
                    shell_quote(&output_dir.display().to_string())
                ),
                "output path '{}' already exists",
                output_dir.display()
            );
        }
        let suggestion = relative_to_cwd(&next_free_sibling(output_dir));
        let mut parts = vec!["rhei".to_string(), "instantiate".to_string(), template.to_string()];
        parts.extend(input_args.iter().cloned());
        parts.push("--output".to_string());
        parts.push(suggestion.display().to_string());
        miette!(
            help = format!("name it for what it is about:\n  {}", shell_command(&parts)),
            "'{}' already exists, so template '{}' has already been instantiated here under \
             that name. A second copy needs its own directory, because the directory name \
             becomes the rhei id every one of its ticket ids is prefixed with.",
            output_dir.display(),
            template
        )
    }

    /// The first `<name>-<n>` beside `path` that nothing occupies, as a
    /// copy-pasteable starting point when the caller has no better name.
    fn next_free_sibling(path: &Path) -> PathBuf {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return path.to_path_buf();
        };
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        (2u32..100)
            .map(|n| parent.join(format!("{name}-{n}")))
            .find(|candidate| !candidate.exists())
            .unwrap_or_else(|| parent.join(format!("{name}-copy")))
    }

    /// `path` written relative to the working directory when it sits beneath
    /// it, so a suggested command is short enough to read and paste.
    fn relative_to_cwd(path: &Path) -> PathBuf {
        std::env::current_dir()
            .ok()
            .and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf))
            .unwrap_or_else(|| path.to_path_buf())
    }
