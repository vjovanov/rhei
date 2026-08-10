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
            return Err(miette!("--execute cannot be used together with --dry-run"));
        }

        let Some(template) = template else {
            // §FS-rhei-templates.6.1.2: an omitted template lists available templates.
            return templates_command(false, "all");
        };

        let resolved_template = resolve_template_reference(template)?;
        let template_dir = resolved_template.path();
        let manifest = load_template_manifest(template_dir)?;

        if list_inputs {
            print_template_inputs(&manifest);
            return Ok(());
        }

        let layout = detect_template_layout(template_dir)?;
        let template_input_args =
            template_input_args_without_execute_args(input_args, execute_args)?;
        let resolved_values = collect_template_inputs(
            &manifest,
            values_files,
            &template_input_args,
            set_values,
            set_files,
        )?;
        let cwd = std::env::current_dir()
            .map_err(|err| miette!("failed to determine working directory: {err}"))?;
        let template_name = template_dir.file_name().ok_or_else(|| {
            miette!("template path '{}' has no directory name", template_dir.display())
        })?;
        // Inside a project the default home is the project itself; defaulting
        // to the working directory dropped the workspace where discovery never
        // looks, so no command listed it. §FS-rhei-templates.6.2
        let default_output =
            enclosing_project_for_new_rhei(&cwd).unwrap_or(cwd).join(template_name);
        let output_dir = output.map(Path::to_path_buf).unwrap_or(default_output);

        if !dry_run && output_dir.exists() {
            return Err(miette!("output path '{}' already exists", output_dir.display()));
        }

        let scratch = if dry_run {
            Some(
                tempfile::tempdir()
                    .map_err(|err| miette!("failed to create temporary output directory: {err}"))?,
            )
        } else {
            None
        };
        let target_dir = scratch
            .as_ref()
            .map(|dir| dir.path().join("instantiate-output"))
            .unwrap_or_else(|| output_dir.clone());

        let materialized =
            match materialize_template(template_dir, layout, &target_dir, &resolved_values) {
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

        // Reconcile with the owning project before validating: a machine
        // collision is a placement problem with a way out, not a load error
        // every later command repeats. §FS-rhei-templates.6.2
        let workspace_machine = workspace_declared_machine(&entrypoint);
        let discard_output = || {
            if !dry_run && !keep_on_error {
                let _ = remove_path(&target_dir, false);
            }
        };
        let placement = match plan_project_placement(
            &output_dir,
            &materialized.output_dir,
            workspace_machine.as_deref(),
        ) {
            Ok(placement) => placement,
            Err(err) => {
                discard_output();
                return Err(err);
            }
        };

        let mut adopted_machine = None;
        let mut hoisted_settings = None;
        let mut manifest_backup: Option<(PathBuf, String)> = None;
        if !dry_run {
            if let ProjectPlacement::Adopts { project, machine } = &placement {
                let manifest = project.join("index.panta.md");
                manifest_backup =
                    fs::read_to_string(&manifest).ok().map(|content| (manifest, content));
                if let Err(err) = adopt_project_machine(project, machine) {
                    discard_output();
                    return Err(err);
                }
                adopted_machine = Some(machine.clone());
            }
            if let Some(project) = placement.project() {
                match hoist_workspace_settings_into_project(&materialized.output_dir, project) {
                    Ok(hoisted) => hoisted_settings = hoisted,
                    Err(err) => {
                        discard_output();
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
            discard_output();
            if let Some((manifest, content)) = manifest_backup {
                let _ = fs::write(manifest, content);
            }
            return Err(err);
        }

        if dry_run {
            println!(
                "Dry run OK: '{}' would be instantiated into '{}'.",
                manifest.name,
                output_dir.display()
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

        println!("Instantiated template '{}' into '{}'.", manifest.name, output_dir.display());
        report_project_placement(
            &placement,
            adopted_machine.as_deref(),
            hoisted_settings.as_ref(),
        );
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

    /// Say what joining a project did to it. Adoption edits `index.panta.md`
    /// and the hoist moves a settings file: both are writes outside the output
    /// directory, so neither may happen silently. §FS-rhei-templates.6.2
    fn report_project_placement(
        placement: &ProjectPlacement,
        adopted_machine: Option<&str>,
        hoisted: Option<&HoistedSettings>,
    ) {
        let Some(project) = placement.project() else {
            return;
        };
        println!("Added to the Panta project at {}.", project.display());
        if let Some(machine) = adopted_machine {
            println!(
                "  Adopted state machine '{machine}' as the project default in index.panta.md."
            );
        }
        if let Some(hoisted) = hoisted {
            println!(
                "  Merged the template's agent settings into {}.",
                project.join(".agents/rhei/settings.json").display()
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
                "internal error: execute arguments were not present in parsed template inputs"
            ));
        }

        let split_at = input_args.len() - execute_args.len();
        if input_args[split_at..] != *execute_args {
            return Err(miette!(
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

        let cli = Cli::try_parse_from(args).map_err(|err| miette!("{}", err.to_string()))?;
        let Commands::Run { standalone, agent, program, snapshot, .. } = cli.command else {
            return Err(miette!("internal error: execute arguments did not parse as run options"));
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
        let loaded = load_plan(&entrypoint)?;
        let resolved =
            resolve_state_machine_for_loaded_plan(&entrypoint, &loaded, state_machine_path)?;
        let tasks = flatten_tasks(&loaded.rhei);

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
            .map_err(|err| miette!("failed to read dir entry in '{}': {err}", root.display()))?;
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
            let priors =
                task.prior.iter().map(|id| format!("Task {id}")).collect::<Vec<_>>().join(", ");
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
            return format!(
                "instantiation stopped before execution; next ready task is {}. Run `rhei run {}` or claim it with `rhei next {}`.",
                format_task_summary_line(task),
                entrypoint.display(),
                entrypoint.display()
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
                output_dir,
            )
        );
    }

    fn format_template_instantiation_command(
        template: &str,
        input_args: &[String],
        set_values: &[String],
        set_files: &[String],
        values_files: &[PathBuf],
        output_dir: &Path,
    ) -> String {
        let mut parts = vec!["rhei".to_string(), "instantiate".to_string(), template.to_string()];
        for values_file in values_files {
            parts.push("--values".to_string());
            parts.push(values_file.display().to_string());
        }
        parts.extend(input_args.iter().cloned());
        for value in set_values {
            parts.push("--set".to_string());
            parts.push(value.clone());
        }
        for value in set_files {
            parts.push("--set-file".to_string());
            parts.push(value.clone());
        }
        parts.push("--output".to_string());
        parts.push(output_dir.display().to_string());

        parts.iter().map(|part| shell_quote(part)).collect::<Vec<_>>().join(" ")
    }

    fn shell_quote(value: &str) -> String {
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
