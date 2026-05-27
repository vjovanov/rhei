#[derive(Clone, Debug)]
struct BatchRunOptions {
    glob: String,
    batch_state_machine: Option<PathBuf>,
    batch_workflow_state_machine: Option<PathBuf>,
    tickets_dir: Option<PathBuf>,
    parallelism: usize,
    inner_parallelism: usize,
    sleep: Option<String>,
    continue_on_error: bool,
    dry_run: bool,
    dashboard: bool,
    no_dashboard: bool,
    tui: bool,
    no_tui: bool,
    agent: Option<String>,
    agent_mode: Option<String>,
    model: Option<String>,
}

#[derive(Clone)]
struct BatchRunConfig {
    report_dir: PathBuf,
    state_machine: Option<PathBuf>,
    parallelism: usize,
    inner_parallelism: usize,
    sleep_secs: Option<u64>,
    continue_on_error: bool,
    nested_dashboard_enabled: bool,
    agent: Option<String>,
    agent_mode: Option<String>,
    model: Option<String>,
    sink: Arc<dyn rhei_tui::EventSink>,
    parent_dashboard_url: Option<String>,
}

#[derive(Clone, Debug)]
struct DiscoveredBatchPlan {
    index: usize,
    path: PathBuf,
    normalized_path: String,
}

#[derive(Clone, Debug)]
struct NestedRunCommand {
    display_args: Vec<String>,
    process_args: Vec<OsString>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum BatchValidationResult {
    Passed,
    Failed,
    NotRun,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum BatchPlanStatus {
    Succeeded,
    Failed,
    Skipped,
}

#[derive(Clone, Debug, Serialize)]
struct BatchPlanRecord {
    index: usize,
    path: String,
    normalized_path: String,
    started_at: Option<String>,
    ended_at: Option<String>,
    validation: BatchValidationResult,
    validation_warnings: Vec<String>,
    run_exit_code: Option<i32>,
    status: BatchPlanStatus,
    status_message: String,
    command: String,
    command_args: Vec<String>,
    log_path: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct BatchSummary {
    total: usize,
    succeeded: usize,
    failed: usize,
    skipped: usize,
    started_at: String,
    ended_at: String,
    elapsed_seconds: u64,
}

#[derive(Serialize)]
struct BatchReport<'a> {
    summary: &'a BatchSummary,
    plans: &'a [BatchPlanRecord],
}

struct ActiveBatchFrontend {
    sink: Arc<dyn rhei_tui::EventSink>,
    dashboard: Option<Arc<rhei_tui::DashboardSink>>,
    _frontend: Option<rhei_tui::Frontend>,
}

impl ActiveBatchFrontend {
    fn dashboard_url(&self) -> Option<String> {
        self.dashboard.as_ref().map(|dashboard| dashboard.url().to_string())
    }

    fn write_frozen_dashboard(&self) {
        let Some(dashboard) = &self.dashboard else {
            return;
        };
        match dashboard.write_frozen_dashboard() {
            Ok(path) => self.sink.emit(rhei_tui::RunEvent::Message {
                level: rhei_tui::MessageLevel::Info,
                text: format!("Final batch dashboard: {}", path.display()),
            }),
            Err(err) => self.sink.emit(rhei_tui::RunEvent::Message {
                level: rhei_tui::MessageLevel::Warn,
                text: format!("warning: could not write final batch dashboard: {err}"),
            }),
        }
    }
}

/// Execute `rhei batch-run`: validate and run generated plans as nested `rhei run` commands.
fn batch_run_command(
    plans_dir: &Path,
    state_machine_path: Option<&Path>,
    opts: BatchRunOptions,
) -> MietteResult<()> {
    if opts.parallelism == 0 {
        return Err(miette!("--parallelism must be at least 1"));
    }

    let sleep_secs = opts
        .sleep
        .as_deref()
        .map(parse_batch_sleep_secs)
        .transpose()?;
    let plans = discover_batch_plans(plans_dir, &opts.glob)?;
    let child_state_machine = opts.batch_state_machine.as_deref().or(state_machine_path);

    if let Some(batch_workflow_state_machine) = opts.batch_workflow_state_machine.as_deref() {
        return batch_run_state_machine_command(
            plans_dir,
            &plans,
            child_state_machine,
            batch_workflow_state_machine,
            &opts,
        );
    }

    if opts.dry_run {
        print_batch_dry_run(&plans, child_state_machine, &opts);
        return Ok(());
    }

    let report_dir = create_batch_report_dir(plans_dir)?;
    fs::create_dir_all(report_dir.join("logs")).map_err(|err| {
        miette!("failed to create batch log directory '{}': {err}", report_dir.join("logs").display())
    })?;

    let frontend = start_batch_frontend(&report_dir, &opts, opts.parallelism, plans.len());
    if opts.parallelism > 1 {
        frontend.sink.emit(rhei_tui::RunEvent::Message {
            level: rhei_tui::MessageLevel::Warn,
            text: "warning: --parallelism > 1 may run plans that edit the same worktree concurrently"
                .to_string(),
        });
    }

    let config = BatchRunConfig {
        report_dir: report_dir.clone(),
        state_machine: child_state_machine.map(Path::to_path_buf),
        parallelism: opts.parallelism,
        inner_parallelism: opts.inner_parallelism,
        sleep_secs,
        continue_on_error: opts.continue_on_error,
        nested_dashboard_enabled: !opts.no_dashboard,
        agent: opts.agent,
        agent_mode: opts.agent_mode,
        model: opts.model,
        sink: frontend.sink.clone(),
        parent_dashboard_url: frontend.dashboard_url(),
    };

    let (summary, records) = run_batch_plans(&plans, config);
    write_batch_reports(&report_dir, &summary, &records)?;

    frontend.sink.emit(rhei_tui::RunEvent::Message {
        level: rhei_tui::MessageLevel::Info,
        text: format!(
            "Batch complete: {} succeeded, {} failed, {} skipped ({} total).",
            summary.succeeded, summary.failed, summary.skipped, summary.total
        ),
    });
    frontend.sink.emit(rhei_tui::RunEvent::Message {
        level: rhei_tui::MessageLevel::Info,
        text: format!("Batch report: {}", report_dir.display()),
    });
    frontend.write_frozen_dashboard();
    drop(frontend);

    if summary.failed > 0 {
        return Err(miette!(
            "batch run failed: {} plan(s) failed; report: {}",
            summary.failed,
            report_dir.display()
        ));
    }

    Ok(())
}

fn start_batch_frontend(
    report_dir: &Path,
    opts: &BatchRunOptions,
    parallelism: usize,
    total_plans: usize,
) -> ActiveBatchFrontend {
    let frontend_kind = if opts.tui {
        rhei_tui::FrontendKind::Tui
    } else if opts.no_tui {
        rhei_tui::FrontendKind::Stdout
    } else {
        rhei_tui::FrontendKind::Auto
    };
    let frontend_parallel = parallelism.max(1).min(u16::MAX as usize) as u16;
    let frontend =
        rhei_tui::select_frontend(report_dir, frontend_kind, frontend_parallel, total_plans);

    // §FS-rhei-batch-run.3.2: batch-run defaults to a terminal UI, while the
    // parent browser dashboard is opt-in.
    let dashboard = if opts.dashboard && !opts.no_dashboard {
        match rhei_tui::DashboardSink::start(
            report_dir.to_path_buf(),
            frontend_parallel,
            total_plans,
        ) {
            Ok(dashboard) => Some(Arc::new(dashboard)),
            Err(err) => {
                frontend.sink.emit(rhei_tui::RunEvent::Message {
                    level: rhei_tui::MessageLevel::Warn,
                    text: format!("warning: could not start batch dashboard: {err}"),
                });
                None
            }
        }
    } else {
        None
    };

    let sink: Arc<dyn rhei_tui::EventSink> = if let Some(dashboard) = &dashboard {
        Arc::new(rhei_tui::Tee::new(vec![frontend.sink.clone(), dashboard.clone()]))
    } else {
        frontend.sink.clone()
    };

    ActiveBatchFrontend { sink, dashboard, _frontend: Some(frontend) }
}

fn batch_run_state_machine_command(
    plans_dir: &Path,
    plans: &[DiscoveredBatchPlan],
    nested_state_machine_path: Option<&Path>,
    batch_workflow_state_machine_path: &Path,
    opts: &BatchRunOptions,
) -> MietteResult<()> {
    if opts.dry_run {
        print_batch_state_machine_dry_run(
            plans,
            nested_state_machine_path,
            batch_workflow_state_machine_path,
            opts,
        );
        return Ok(());
    }

    let report_dir = create_batch_report_dir(plans_dir)?;
    let workspace_dir = report_dir.join("workspace");
    let tickets_dir = opts
        .tickets_dir
        .clone()
        .or_else(|| infer_batch_tickets_dir(plans_dir));
    materialize_batch_workspace(
        &workspace_dir,
        plans,
        batch_workflow_state_machine_path,
        nested_state_machine_path,
        tickets_dir.as_deref(),
    )?;
    let generated_state_machine = materialize_batch_state_machine(
        &workspace_dir,
        batch_workflow_state_machine_path,
        nested_state_machine_path,
        opts,
    )?;

    let run_opts = RunOptions {
        standalone: StandaloneExecutionFlags {
            dry_run: false,
            no_callbacks: false,
            continue_on_error: opts.continue_on_error,
            parallel: opts.parallelism,
            tui: opts.tui,
            no_tui: opts.no_tui,
            dashboard: opts.dashboard,
            no_dashboard: opts.no_dashboard || !opts.dashboard,
        },
        agent: AgentExecutionFlags {
            no_agent: false,
            agent: opts.agent.clone(),
            agent_mode: opts.agent_mode.clone(),
            model: opts.model.clone(),
        },
        program: ProgramExecutionFlags {
            no_program: false,
            program_timeout: None,
        },
        snapshot: SnapshotExecutionFlags::default(),
    };

    let result = run_command(&workspace_dir, Some(&generated_state_machine), run_opts);
    if result.is_ok() {
        if let Some(sleep) = opts
            .sleep
            .as_deref()
            .map(parse_batch_sleep_secs)
            .transpose()?
        {
            std::thread::sleep(Duration::from_secs(sleep));
        }
    }
    result
}

fn print_batch_state_machine_dry_run(
    plans: &[DiscoveredBatchPlan],
    nested_state_machine_path: Option<&Path>,
    batch_workflow_state_machine_path: &Path,
    opts: &BatchRunOptions,
) {
    println!("Discovered {} plan(s):", plans.len());
    println!(
        "Would materialize a parent batch workspace and run it with workflow state machine: {}",
        batch_workflow_state_machine_path.display()
    );
    let inferred_tickets_dir = infer_batch_tickets_dir_for_display(plans.first());
    let tickets_dir = opts.tickets_dir.as_deref().or(inferred_tickets_dir.as_deref());
    if let Some(tickets_dir) = tickets_dir {
        println!("Tickets directory: {}", tickets_dir.display());
    }
    if let Some(nested_state_machine_path) = nested_state_machine_path {
        println!(
            "Nested plan state machine: {}",
            nested_state_machine_path.display()
        );
    }
    for plan in plans {
        let task_id = batch_task_id(plan.index, plan);
        let status =
            if source_plan_is_terminal(&plan.path, nested_state_machine_path).unwrap_or(false) {
                " (already terminal; generated task starts completed)"
            } else {
                ""
            };
        println!(
            "{}. {} -> Task {}{}",
            plan.index + 1,
            plan.normalized_path,
            task_id,
            status
        );
    }
    if state_machine_declares_state(batch_workflow_state_machine_path, "create-pr").unwrap_or(false)
    {
        println!("Final task: create-pr");
    }
    println!(
        "Command: rhei --state-machine {} run <batch-workspace> --parallel {}{}{}",
        shell_quote(&batch_workflow_state_machine_path.display().to_string()),
        opts.parallelism,
        if opts.dashboard { " --dashboard" } else { "" },
        if opts.no_tui { " --no-tui" } else { "" },
    );
}

fn materialize_batch_workspace(
    workspace_dir: &Path,
    plans: &[DiscoveredBatchPlan],
    batch_state_machine_path: &Path,
    nested_state_machine_path: Option<&Path>,
    tickets_dir: Option<&Path>,
) -> MietteResult<()> {
    // §FS-rhei-batch-run.3.1: batch state-machine mode runs a generated
    // Directory Workspace whose tasks consume discovered plans as artifacts.
    fs::create_dir_all(workspace_dir.join("tasks")).map_err(|err| {
        miette!("failed to create batch workspace '{}': {err}", workspace_dir.display())
    })?;
    fs::create_dir_all(workspace_dir.join("inputs/generated-plans")).map_err(|err| {
        miette!(
            "failed to create batch plan input directory '{}': {err}",
            workspace_dir.join("inputs/generated-plans").display()
        )
    })?;
    fs::create_dir_all(workspace_dir.join("inputs/tickets")).map_err(|err| {
        miette!(
            "failed to create batch ticket input directory '{}': {err}",
            workspace_dir.join("inputs/tickets").display()
        )
    })?;
    fs::create_dir_all(workspace_dir.join("inputs/source-plans")).map_err(|err| {
        miette!(
            "failed to create batch source-plan input directory '{}': {err}",
            workspace_dir.join("inputs/source-plans").display()
        )
    })?;
    copy_batch_settings(workspace_dir, plans)?;

    let initial_state = batch_initial_state(batch_state_machine_path)
        .unwrap_or_else(|| "execute-plan".to_string());
    let completed_state = batch_successful_terminal_state(batch_state_machine_path)
        .unwrap_or_else(|| "completed".to_string());
    let has_create_pr = state_machine_declares_state(batch_state_machine_path, "create-pr")?;
    let mut task_ids = Vec::new();
    let mut used_ids = HashSet::new();

    for plan in plans {
        let mut task_id = batch_task_id(plan.index, plan);
        if !used_ids.insert(task_id.clone()) {
            task_id = format!("{:02}-{task_id}", plan.index + 1);
            used_ids.insert(task_id.clone());
        }
        task_ids.push(task_id.clone());

        let plan_input = workspace_dir
            .join("inputs/generated-plans")
            .join(format!("{task_id}.rhei.md"));
        fs::copy(&plan.path, &plan_input).map_err(|err| {
            miette!(
                "failed to copy plan '{}' to '{}': {err}",
                plan.path.display(),
                plan_input.display()
            )
        })?;
        // §FS-rhei-batch-run.3.1: copied plan runs sync successful terminal state back to source.
        let source_plan = workspace_dir.join("inputs/source-plans").join(format!("{task_id}.txt"));
        fs::write(&source_plan, format!("{}\n", absolute_source_plan_path(&plan.path).display()))
            .map_err(|err| {
                miette!(
                    "failed to write source-plan pointer '{}': {err}",
                    source_plan.display()
                )
            })?;

        if let Some(ticket) = find_batch_ticket(tickets_dir, &task_id, plan) {
            let ticket_input = workspace_dir.join("inputs/tickets").join(format!("{task_id}.md"));
            fs::copy(&ticket, &ticket_input).map_err(|err| {
                miette!(
                    "failed to copy ticket '{}' to '{}': {err}",
                    ticket.display(),
                    ticket_input.display()
                )
            })?;
        }

        let task_file = workspace_dir
            .join("tasks")
            .join(format!("{:02}-{task_id}.md", plan.index + 1));
        // §FS-rhei-batch-run.3.1: already-terminal source plans start completed in the parent.
        let state = if source_plan_is_terminal(&plan.path, nested_state_machine_path).unwrap_or(false)
        {
            completed_state.as_str()
        } else {
            initial_state.as_str()
        };
        fs::write(
            &task_file,
            render_batch_plan_task(&task_id, plan, state),
        )
        .map_err(|err| miette!("failed to write '{}': {err}", task_file.display()))?;
    }

    if has_create_pr {
        let task_file = workspace_dir.join("tasks/99-create-pr.md");
        fs::write(&task_file, render_batch_create_pr_task(&task_ids))
            .map_err(|err| miette!("failed to write '{}': {err}", task_file.display()))?;
    }

    let index = r#"# Rhei: Generated Batch Run

Run discovered generated plans as a state-machine-backed batch workspace.

Generated inputs live under `inputs/generated-plans/` and `inputs/tickets/`.
"#;
    fs::write(workspace_dir.join("index.rhei.md"), index)
        .map_err(|err| miette!("failed to write batch workspace index: {err}"))?;

    Ok(())
}

fn materialize_batch_state_machine(
    workspace_dir: &Path,
    batch_state_machine_path: &Path,
    nested_state_machine_path: Option<&Path>,
    opts: &BatchRunOptions,
) -> MietteResult<PathBuf> {
    let yaml = fs::read_to_string(batch_state_machine_path).map_err(|err| {
        miette!(
            "failed to read batch state machine '{}': {err}",
            batch_state_machine_path.display()
        )
    })?;
    let mut value: YamlValue = serde_yaml::from_str(&yaml).map_err(|err| {
        miette!(
            "failed to parse batch state machine '{}': {err}",
            batch_state_machine_path.display()
        )
    })?;
    let execute_state = batch_initial_state(batch_state_machine_path)
        .unwrap_or_else(|| "execute-plan".to_string());
    if let Some(state) = value
        .get_mut("states")
        .and_then(YamlValue::as_mapping_mut)
        .and_then(|states| states.get_mut(YamlValue::String(execute_state.clone())))
        .and_then(YamlValue::as_mapping_mut)
    {
        state.insert(
            YamlValue::String("program".to_string()),
            generated_batch_execute_program(nested_state_machine_path, opts)?,
        );
        ensure_source_plan_input(state);
    }

    let target = workspace_dir.join("states.yaml");
    let rendered = serde_yaml::to_string(&value)
        .map_err(|err| miette!("failed to render generated batch state machine: {err}"))?;
    fs::write(&target, rendered)
        .map_err(|err| miette!("failed to write '{}': {err}", target.display()))?;
    Ok(target)
}

fn ensure_source_plan_input(state: &mut YamlMapping) {
    let inputs_key = YamlValue::String("inputs".to_string());
    let source_name = YamlValue::String("source-plan".to_string());
    let input = YamlValue::Mapping(YamlMapping::from_iter([
        (YamlValue::String("name".to_string()), source_name.clone()),
        (
            YamlValue::String("path".to_string()),
            YamlValue::String("inputs/source-plans/{task_id}.txt".to_string()),
        ),
        (
            YamlValue::String("description".to_string()),
            YamlValue::String("Original discovered plan path to update after success.".to_string()),
        ),
    ]));

    let Some(inputs) = state.get_mut(&inputs_key) else {
        state.insert(inputs_key, YamlValue::Sequence(vec![input]));
        return;
    };
    let Some(inputs) = inputs.as_sequence_mut() else {
        return;
    };
    let already_present = inputs.iter().any(|entry| {
        entry
            .as_mapping()
            .and_then(|mapping| mapping.get(YamlValue::String("name".to_string())))
            == Some(&source_name)
    });
    if !already_present {
        inputs.push(input);
    }
}

fn generated_batch_execute_program(
    nested_state_machine_path: Option<&Path>,
    opts: &BatchRunOptions,
) -> MietteResult<YamlValue> {
    // §FS-rhei-batch-run.3.1: generated execution states surface nested dashboard
    // output to the parent batch TUI instead of hiding it only in report files.
    let nested_state_machine = nested_state_machine_path.map(canonical_shell_path).transpose()?;
    let mut validate_args = vec!["rhei".to_string()];
    if let Some(path) = &nested_state_machine {
        validate_args.push("--state-machine".to_string());
        validate_args.push(path.clone());
    }
    validate_args.push("validate".to_string());
    validate_args.push("\"${RHEI_INPUT_IMPLEMENTATION_PLAN_PATH}\"".to_string());

    let mut run_args = vec!["rhei".to_string()];
    if let Some(path) = &nested_state_machine {
        run_args.push("--state-machine".to_string());
        run_args.push(path.clone());
    }
    run_args.push("run".to_string());
    run_args.push("\"${RHEI_INPUT_IMPLEMENTATION_PLAN_PATH}\"".to_string());
    run_args.push("--parallel".to_string());
    run_args.push(opts.inner_parallelism.to_string());
    run_args.push(if opts.no_dashboard {
        "--no-dashboard".to_string()
    } else {
        "--dashboard".to_string()
    });
    run_args.push("--no-tui".to_string());
    if let Some(agent) = &opts.agent {
        run_args.push("--agent".to_string());
        run_args.push(shell_quote(agent));
    }
    if let Some(agent_mode) = &opts.agent_mode {
        run_args.push("--agent-mode".to_string());
        run_args.push(shell_quote(agent_mode));
    }
    if let Some(model) = &opts.model {
        run_args.push("--model".to_string());
        run_args.push(shell_quote(model));
    }

    let script = format!(
        r###"set +e
mkdir -p runtime/execution
report="runtime/execution/${{RHEI_TASK_ID}}.md"
status_file="$(mktemp)"

{{
  echo "# Execution Report: ${{RHEI_TASK_ID}}"
  echo
  echo "- Plan: \`${{RHEI_INPUT_IMPLEMENTATION_PLAN_PATH}}\`"
  echo "- Ticket: \`${{RHEI_INPUT_TICKET_PATH}}\`"
  echo
  echo "## Ticket"
  echo
  if [ -f "${{RHEI_INPUT_TICKET_PATH}}" ]; then
    sed -n '1,220p' "${{RHEI_INPUT_TICKET_PATH}}"
  else
    echo "No matching ticket was copied for this plan."
  fi
  echo
  echo "## Validation"
  echo
}} > "${{report}}"

( {validate_command}; printf '%s\n' "$?" > "${{status_file}}" ) 2>&1 | tee -a "${{report}}"
validation_code="$(cat "${{status_file}}")"
if [ "${{validation_code}}" -ne 0 ]; then
  {{
    echo
    echo "Validation failed with exit code ${{validation_code}}."
  }} >> "${{report}}"
  rm -f "${{status_file}}"
  exit "${{validation_code}}"
fi

{{
  echo
  echo "## Run"
  echo
}} >> "${{report}}"

( {run_command}; printf '%s\n' "$?" > "${{status_file}}" ) 2>&1 | tee -a "${{report}}"
run_code="$(cat "${{status_file}}")"

if [ "${{run_code}}" -eq 0 ] && [ -n "${{RHEI_INPUT_SOURCE_PLAN_PATH:-}}" ] && [ -f "${{RHEI_INPUT_SOURCE_PLAN_PATH}}" ]; then
  source_plan="$(sed -n '1p' "${{RHEI_INPUT_SOURCE_PLAN_PATH}}")"
  if [ -n "${{source_plan}}" ]; then
    cp "${{RHEI_INPUT_IMPLEMENTATION_PLAN_PATH}}" "${{source_plan}}"
    echo "Synced completed plan back to ${{source_plan}}." | tee -a "${{report}}"
  fi
fi

{{
  echo
  echo "## Exit"
  echo
  echo "Nested Rhei run exited with code ${{run_code}}."
}} >> "${{report}}"

rm -f "${{status_file}}"
exit "${{run_code}}"
"###,
        validate_command = validate_args.join(" "),
        run_command = run_args.join(" "),
    );

    Ok(YamlValue::Mapping(YamlMapping::from_iter([(
        YamlValue::String("command".to_string()),
        YamlValue::Sequence(vec![
            YamlValue::String("sh".to_string()),
            YamlValue::String("-c".to_string()),
            YamlValue::String(script),
        ]),
    )])))
}

fn canonical_shell_path(path: &Path) -> MietteResult<String> {
    let canonical = path
        .canonicalize()
        .map_err(|err| miette!("failed to read state machine '{}': {err}", path.display()))?;
    Ok(shell_quote(&canonical.display().to_string()))
}

fn copy_batch_settings(workspace_dir: &Path, plans: &[DiscoveredBatchPlan]) -> MietteResult<()> {
    let Some(settings) = plans
        .first()
        .and_then(|plan| plan.path.parent())
        .map(|plans_dir| plans_dir.join(PROJECT_SETTINGS_RELATIVE_PATH))
        .filter(|path| path.is_file())
    else {
        return Ok(());
    };

    for target_root in [workspace_dir, &workspace_dir.join("inputs/generated-plans")] {
        let target = target_root.join(PROJECT_SETTINGS_RELATIVE_PATH);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                miette!("failed to create settings directory '{}': {err}", parent.display())
            })?;
        }
        fs::copy(&settings, &target).map_err(|err| {
            miette!(
                "failed to copy settings '{}' to '{}': {err}",
                settings.display(),
                target.display()
            )
        })?;
    }

    Ok(())
}

fn render_batch_plan_task(task_id: &str, plan: &DiscoveredBatchPlan, state: &str) -> String {
    format!(
        "### Task {task_id}: Execute {title}\n\
         **State:** {state}\n\n\
         Consumes:\n\
         - Plan: `inputs/generated-plans/{task_id}.rhei.md`\n\
         - Ticket: `inputs/tickets/{task_id}.md`\n\n\
         Execute generated plan `{source}` as part of this batch.\n",
        title = plan.normalized_path,
        source = plan.normalized_path,
    )
}

fn render_batch_create_pr_task(prior_task_ids: &[String]) -> String {
    let priors = prior_task_ids
        .iter()
        .map(|task_id| format!("Task {task_id}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "### Task create-pr: Create pull request for completed batch work\n\
         **State:** create-pr\n\
         **Prior:** {priors}\n\n\
         Create a pull request after every generated implementation plan in this\n\
         batch workspace has completed successfully.\n"
    )
}

fn batch_task_id(index: usize, plan: &DiscoveredBatchPlan) -> String {
    let stem = plan
        .path
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("plan")
        .trim_end_matches(".rhei");
    let stripped = stem
        .trim_start_matches(|ch: char| ch.is_ascii_digit())
        .trim_start_matches('-');
    let raw = if stripped.is_empty() { stem } else { stripped };
    let mut id = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            id.push(ch.to_ascii_lowercase());
        } else {
            id.push('-');
        }
    }
    let id = id.trim_matches('-');
    if id.is_empty() {
        format!("plan-{}", index + 1)
    } else {
        id.to_string()
    }
}

fn infer_batch_tickets_dir(plans_dir: &Path) -> Option<PathBuf> {
    let candidate = plans_dir.parent()?.join("tickets");
    candidate.is_dir().then_some(candidate)
}

fn infer_batch_tickets_dir_for_display(plan: Option<&DiscoveredBatchPlan>) -> Option<PathBuf> {
    let plan = plan?;
    infer_batch_tickets_dir(plan.path.parent()?)
}

fn find_batch_ticket(
    tickets_dir: Option<&Path>,
    task_id: &str,
    plan: &DiscoveredBatchPlan,
) -> Option<PathBuf> {
    let tickets_dir = tickets_dir?;
    let task_ticket = tickets_dir.join(format!("{task_id}.md"));
    if task_ticket.is_file() {
        return Some(task_ticket);
    }
    let stem = plan.path.file_stem().and_then(OsStr::to_str)?;
    let numbered_ticket = tickets_dir.join(format!("{stem}.md"));
    numbered_ticket.is_file().then_some(numbered_ticket)
}

fn batch_initial_state(batch_state_machine_path: &Path) -> Option<String> {
    let yaml = fs::read_to_string(batch_state_machine_path).ok()?;
    let value: YamlValue = serde_yaml::from_str(&yaml).ok()?;
    let profiles = value.get("profiles")?.as_mapping()?;
    let default = profiles
        .get(YamlValue::String("default".to_string()))
        .or_else(|| profiles.values().next())?;
    default
        .get("initial")
        .and_then(YamlValue::as_str)
        .map(str::to_string)
}

fn state_machine_declares_state(path: &Path, state_name: &str) -> MietteResult<bool> {
    let yaml = fs::read_to_string(path)
        .map_err(|err| miette!("failed to read batch state machine '{}': {err}", path.display()))?;
    let value: YamlValue = serde_yaml::from_str(&yaml)
        .map_err(|err| miette!("failed to parse batch state machine '{}': {err}", path.display()))?;
    let Some(states) = value.get("states").and_then(YamlValue::as_mapping) else {
        return Ok(false);
    };
    Ok(states.contains_key(YamlValue::String(state_name.to_string())))
}

fn batch_successful_terminal_state(path: &Path) -> Option<String> {
    let yaml = fs::read_to_string(path).ok()?;
    let machine = rhei_validator::StateMachine::from_yaml_str(&yaml).ok()?;
    if machine.states.get("completed").map(|state| state.terminal).unwrap_or(false) {
        return Some("completed".to_string());
    }
    machine
        .states
        .iter()
        .find_map(|(name, state)| (state.terminal && name != "cancelled").then(|| name.clone()))
}

fn absolute_source_plan_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(path)
        }
    })
}

fn source_plan_is_terminal(plan_path: &Path, state_machine_path: Option<&Path>) -> MietteResult<bool> {
    let loaded = load_plan_for_validation(plan_path)?;
    let resolved = resolve_state_machine_for_loaded_plan(plan_path, &loaded, state_machine_path)?;
    let mut tasks = Vec::new();
    collect_plan_tasks(&loaded.rhei.tasks, &mut tasks);
    Ok(!tasks.is_empty()
        && tasks
            .iter()
            .all(|task| is_terminal_state(task.state.as_str(), &resolved.machine)))
}

fn parse_batch_sleep_secs(value: &str) -> MietteResult<u64> {
    rhei_validator::parse_duration_secs(value)
        .ok_or_else(|| miette!("invalid --sleep duration '{value}' (expected e.g. 30s, 5m, 1h)"))
}

fn discover_batch_plans(plans_dir: &Path, glob: &str) -> MietteResult<Vec<DiscoveredBatchPlan>> {
    let root = plans_dir
        .canonicalize()
        .map_err(|err| miette!("failed to read plans directory '{}': {err}", plans_dir.display()))?;
    if !root.is_dir() {
        return Err(miette!("plans path '{}' is not a directory", plans_dir.display()));
    }

    let normalized_glob = glob.replace('\\', "/");
    let match_relative = normalized_glob.contains('/');
    let regex = compile_simple_glob(&normalized_glob)?;
    let mut paths = Vec::new();
    collect_batch_plan_paths(&root, &root, &regex, match_relative, &mut paths)?;
    paths.sort_by(|a, b| a.normalized_path.cmp(&b.normalized_path));
    for (index, plan) in paths.iter_mut().enumerate() {
        plan.index = index;
    }
    Ok(paths)
}

fn collect_batch_plan_paths(
    root: &Path,
    dir: &Path,
    regex: &Regex,
    match_relative: bool,
    plans: &mut Vec<DiscoveredBatchPlan>,
) -> MietteResult<()> {
    let entries = fs::read_dir(dir)
        .map_err(|err| miette!("failed to read directory '{}': {err}", dir.display()))?;

    for entry in entries {
        let entry =
            entry.map_err(|err| miette!("failed to read entry in '{}': {err}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|err| miette!("failed to inspect '{}': {err}", path.display()))?;
        if file_type.is_dir() {
            collect_batch_plan_paths(root, &path, regex, match_relative, plans)?;
        } else if file_type.is_file() {
            let normalized_path = normalized_relative_path(root, &path);
            let target = if match_relative {
                normalized_path.as_str()
            } else {
                path.file_name().and_then(OsStr::to_str).unwrap_or_default()
            };
            if regex.is_match(target) {
                plans.push(DiscoveredBatchPlan { index: 0, path, normalized_path });
            }
        }
    }

    Ok(())
}

fn compile_simple_glob(pattern: &str) -> MietteResult<Regex> {
    let mut regex = String::from("^");
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '*' if chars.peek() == Some(&'*') => {
                let _ = chars.next();
                regex.push_str(".*");
            }
            '*' => regex.push_str("[^/]*"),
            '?' => regex.push_str("[^/]"),
            other => regex.push_str(&regex::escape(&other.to_string())),
        }
    }
    regex.push('$');
    Regex::new(&regex).map_err(|err| miette!("invalid --glob pattern '{pattern}': {err}"))
}

fn normalized_relative_path(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            std::path::Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
            std::path::Component::ParentDir => parts.push("..".to_string()),
            std::path::Component::CurDir => {}
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {}
        }
    }
    parts.join("/")
}

fn print_batch_dry_run(
    plans: &[DiscoveredBatchPlan],
    state_machine_path: Option<&Path>,
    opts: &BatchRunOptions,
) {
    println!("Discovered {} plan(s):", plans.len());
    for plan in plans {
        let command = nested_run_command(
            &plan.path,
            state_machine_path,
            opts.inner_parallelism,
            !opts.no_dashboard,
            opts.agent.as_deref(),
            opts.agent_mode.as_deref(),
            opts.model.as_deref(),
        );
        println!("{}. {}", plan.index + 1, plan.normalized_path);
        if source_plan_is_terminal(&plan.path, state_machine_path).unwrap_or(false) {
            println!("   already terminal; would skip nested run");
        } else {
            println!("   {}", format_nested_command(&command.display_args));
        }
    }
}

fn create_batch_report_dir(plans_dir: &Path) -> MietteResult<PathBuf> {
    let canonical = plans_dir
        .canonicalize()
        .map_err(|err| miette!("failed to read plans directory '{}': {err}", plans_dir.display()))?;
    let runtime_root = batch_runtime_root(&canonical);
    let timestamp = format_iso8601_utc(std::time::SystemTime::now())
        .trim_end_matches('Z')
        .replace(['-', ':'], "")
        .replace('T', "-");
    let report_dir =
        runtime_root.join("batch-runs").join(format!("{timestamp}-{}", std::process::id()));
    fs::create_dir_all(&report_dir).map_err(|err| {
        miette!("failed to create batch report directory '{}': {err}", report_dir.display())
    })?;
    Ok(report_dir)
}

fn batch_runtime_root(plans_dir: &Path) -> PathBuf {
    for ancestor in plans_dir.ancestors() {
        if ancestor.file_name().and_then(OsStr::to_str) == Some("runtime") {
            return ancestor.to_path_buf();
        }
    }
    plans_dir.join("runtime")
}

fn run_batch_plans(
    plans: &[DiscoveredBatchPlan],
    config: BatchRunConfig,
) -> (BatchSummary, Vec<BatchPlanRecord>) {
    let batch_started_wall = std::time::SystemTime::now();
    let batch_started = Instant::now();
    let worker_count = config.parallelism.min(plans.len()).max(1);
    let frontend_parallel = worker_count.min(u16::MAX as usize) as u16;

    config.sink.emit(rhei_tui::RunEvent::RunStarted {
        workspace: config.report_dir.clone(),
        parallel: frontend_parallel,
        total_tasks: plans.len(),
    });
    if let Some(url) = &config.parent_dashboard_url {
        config.sink.emit(rhei_tui::RunEvent::RunLink {
            label: "Dashboard".to_string(),
            url: url.clone(),
        });
    }
    config.sink.emit(rhei_tui::RunEvent::PassStarted {
        pass: 1,
        ready: plans.iter().map(|plan| plan.normalized_path.clone()).collect(),
    });

    if plans.is_empty() {
        let summary = BatchSummary {
            total: 0,
            succeeded: 0,
            failed: 0,
            skipped: 0,
            started_at: format_iso8601_utc(batch_started_wall),
            ended_at: format_iso8601_utc(std::time::SystemTime::now()),
            elapsed_seconds: batch_started.elapsed().as_secs(),
        };
        config.sink.emit(rhei_tui::RunEvent::PassEnded { pass: 1, progressed: false });
        config.sink.emit(rhei_tui::RunEvent::RunFinished {
            summary: rhei_tui::RunSummary {
                agents_spawned: 0,
                programs_spawned: 0,
                terminal_tasks: 0,
                total_tasks: 0,
                accounting: None,
            },
        });
        return (summary, Vec::new());
    }

    let queue = Arc::new(Mutex::new(VecDeque::from(plans.to_vec())));
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let config = Arc::new(config);
    let (tx, rx) = mpsc::channel();
    let mut workers = Vec::new();

    for worker_index in 0..worker_count {
        let queue = Arc::clone(&queue);
        let stop = Arc::clone(&stop);
        let config = Arc::clone(&config);
        let tx = tx.clone();
        workers.push(std::thread::spawn(move || {
            let slot = worker_index.min(u16::MAX as usize) as u16;
            loop {
                if !config.continue_on_error && stop.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                let plan = {
                    let mut queue = queue.lock().expect("batch queue lock poisoned");
                    if !config.continue_on_error
                        && stop.load(std::sync::atomic::Ordering::SeqCst)
                    {
                        None
                    } else {
                        queue.pop_front()
                    }
                };
                let Some(plan) = plan else {
                    break;
                };

                let started_at = Instant::now();
                let started_wall = std::time::SystemTime::now();
                let log_path = batch_plan_log_path(&config.report_dir, &plan);
                config.sink.emit(rhei_tui::RunEvent::SlotAssigned {
                    slot,
                    task: plan.normalized_path.clone(),
                    from: "queued".to_string(),
                    to: "running".to_string(),
                    agent: None,
                    template_context: None,
                    log_path: log_path.clone(),
                    started_at,
                    wall_clock: started_wall,
                });

                let record = execute_batch_plan(&plan, &config, slot, started_at, started_wall);
                let failed = matches!(record.status, BatchPlanStatus::Failed);
                config.sink.emit(rhei_tui::RunEvent::SlotReleased {
                    slot,
                    task: plan.normalized_path.clone(),
                    from: "queued".to_string(),
                    to: batch_plan_final_state(&record).to_string(),
                    log_path,
                    outcome: batch_plan_outcome(&record),
                    finished_at: Instant::now(),
                    wall_clock: std::time::SystemTime::now(),
                    exit_code: record.run_exit_code,
                    duration_ms: started_at.elapsed().as_millis() as u64,
                });
                if failed && !config.continue_on_error {
                    stop.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                let should_sleep = config.sleep_secs.unwrap_or(0) > 0
                    && {
                        let queue_has_more =
                            !queue.lock().expect("batch queue lock poisoned").is_empty();
                        queue_has_more
                            && (config.continue_on_error
                                || !stop.load(std::sync::atomic::Ordering::SeqCst))
                    };
                let _ = tx.send(record);
                if should_sleep {
                    std::thread::sleep(Duration::from_secs(config.sleep_secs.unwrap_or(0)));
                }
            }
        }));
    }
    drop(tx);

    let mut records: Vec<BatchPlanRecord> = rx.into_iter().collect();
    for worker in workers {
        let _ = worker.join();
    }

    let completed: HashSet<usize> = records.iter().map(|record| record.index).collect();
    for plan in plans {
        if !completed.contains(&plan.index) {
            records.push(skipped_batch_record(plan, &config));
        }
    }
    records.sort_by_key(|record| record.index);

    let succeeded = records
        .iter()
        .filter(|record| matches!(record.status, BatchPlanStatus::Succeeded))
        .count();
    let failed = records
        .iter()
        .filter(|record| matches!(record.status, BatchPlanStatus::Failed))
        .count();
    let skipped = records
        .iter()
        .filter(|record| matches!(record.status, BatchPlanStatus::Skipped))
        .count();
    let summary = BatchSummary {
        total: records.len(),
        succeeded,
        failed,
        skipped,
        started_at: format_iso8601_utc(batch_started_wall),
        ended_at: format_iso8601_utc(std::time::SystemTime::now()),
        elapsed_seconds: batch_started.elapsed().as_secs(),
    };
    config.sink.emit(rhei_tui::RunEvent::PassEnded {
        pass: 1,
        progressed: succeeded + failed > 0,
    });
    config.sink.emit(rhei_tui::RunEvent::RunFinished {
        summary: rhei_tui::RunSummary {
            agents_spawned: 0,
            programs_spawned: 0,
            terminal_tasks: succeeded,
            total_tasks: records.len(),
            accounting: None,
        },
    });
    (summary, records)
}

fn execute_batch_plan(
    plan: &DiscoveredBatchPlan,
    config: &BatchRunConfig,
    slot: u16,
    _started_at: Instant,
    started_wall: std::time::SystemTime,
) -> BatchPlanRecord {
    let command = nested_run_command(
        &plan.path,
        config.state_machine.as_deref(),
        config.inner_parallelism,
        config.nested_dashboard_enabled,
        config.agent.as_deref(),
        config.agent_mode.as_deref(),
        config.model.as_deref(),
    );
    let command_display = format_nested_command(&command.display_args);
    let log_path = batch_plan_log_path(&config.report_dir, plan);

    let mut record = BatchPlanRecord {
        index: plan.index,
        path: plan.path.display().to_string(),
        normalized_path: plan.normalized_path.clone(),
        started_at: Some(format_iso8601_utc(started_wall)),
        ended_at: None,
        validation: BatchValidationResult::NotRun,
        validation_warnings: Vec::new(),
        run_exit_code: None,
        status: BatchPlanStatus::Failed,
        status_message: String::new(),
        command: command_display.clone(),
        command_args: command.display_args.clone(),
        log_path: Some(log_path.display().to_string()),
    };

    let mut log = match fs::File::create(&log_path) {
        Ok(file) => file,
        Err(err) => {
            record.ended_at = Some(format_iso8601_utc(std::time::SystemTime::now()));
            record.status_message = format!("failed to create plan log: {err}");
            record.log_path = None;
            return record;
        }
    };
    let _ = writeln!(log, "plan: {}", plan.path.display());
    let _ = writeln!(log, "normalized_path: {}", plan.normalized_path);
    let _ = writeln!(log, "command: {command_display}");
    let _ = writeln!(log, "started: {}", record.started_at.as_deref().unwrap_or_default());

    // §FS-rhei-batch-run.3: every plan is validated before invoking nested `rhei run`.
    match validate_plan_once(&plan.path, config.state_machine.as_deref()) {
        Ok(warnings) => {
            record.validation = BatchValidationResult::Passed;
            record.validation_warnings = warnings;
            let _ = writeln!(log, "validation: passed");
            for warning in &record.validation_warnings {
                let _ = writeln!(log, "validation_warning: {warning}");
            }
        }
        Err(err) => {
            record.validation = BatchValidationResult::Failed;
            record.ended_at = Some(format_iso8601_utc(std::time::SystemTime::now()));
            record.status_message = format!("validation failed: {err}");
            let _ = writeln!(log, "validation: failed");
            let _ = writeln!(log, "{err:?}");
            return record;
        }
    }
    drop(log);

    // §FS-rhei-batch-run.2: repeated direct batches skip plans already in terminal states.
    if source_plan_is_terminal(&plan.path, config.state_machine.as_deref()).unwrap_or(false) {
        record.status = BatchPlanStatus::Skipped;
        record.status_message = "plan already terminal; skipped nested run".to_string();
        append_batch_log_footer(&log_path, &record.status_message);
        record.ended_at = Some(format_iso8601_utc(std::time::SystemTime::now()));
        return record;
    }

    // §FS-rhei-batch-run.3: execute by invoking existing `rhei run` behavior.
    match invoke_nested_run(&command, &log_path, plan, slot, &config.sink) {
        Ok(status) => {
            record.run_exit_code = status.code();
            if status.success() {
                record.status = BatchPlanStatus::Succeeded;
                record.status_message = "run succeeded".to_string();
            } else {
                record.status_message = match status.code() {
                    Some(code) => format!("run exited with code {code}"),
                    None => "run terminated without an exit code".to_string(),
                };
            }
            append_batch_log_footer(&log_path, &record.status_message);
        }
        Err(err) => {
            record.status_message = format!("failed to invoke nested run: {err}");
            append_batch_log_footer(&log_path, &record.status_message);
        }
    }

    record.ended_at = Some(format_iso8601_utc(std::time::SystemTime::now()));
    record
}

fn skipped_batch_record(plan: &DiscoveredBatchPlan, config: &BatchRunConfig) -> BatchPlanRecord {
    let command = nested_run_command(
        &plan.path,
        config.state_machine.as_deref(),
        config.inner_parallelism,
        config.nested_dashboard_enabled,
        config.agent.as_deref(),
        config.agent_mode.as_deref(),
        config.model.as_deref(),
    );
    BatchPlanRecord {
        index: plan.index,
        path: plan.path.display().to_string(),
        normalized_path: plan.normalized_path.clone(),
        started_at: None,
        ended_at: None,
        validation: BatchValidationResult::NotRun,
        validation_warnings: Vec::new(),
        run_exit_code: None,
        status: BatchPlanStatus::Skipped,
        status_message: "skipped after earlier failure".to_string(),
        command: format_nested_command(&command.display_args),
        command_args: command.display_args,
        log_path: None,
    }
}

fn nested_run_command(
    plan_path: &Path,
    state_machine_path: Option<&Path>,
    inner_parallelism: usize,
    dashboard_enabled: bool,
    agent: Option<&str>,
    agent_mode: Option<&str>,
    model: Option<&str>,
) -> NestedRunCommand {
    let mut display_args = Vec::new();
    let mut process_args = Vec::new();

    if let Some(state_machine_path) = state_machine_path {
        push_nested_arg(&mut display_args, &mut process_args, "--state-machine");
        push_nested_path_arg(&mut display_args, &mut process_args, state_machine_path);
    }
    push_nested_arg(&mut display_args, &mut process_args, "run");
    push_nested_path_arg(&mut display_args, &mut process_args, plan_path);
    push_nested_arg(&mut display_args, &mut process_args, "--parallel");
    push_nested_arg(
        &mut display_args,
        &mut process_args,
        inner_parallelism.to_string(),
    );
    if dashboard_enabled {
        push_nested_arg(&mut display_args, &mut process_args, "--dashboard");
    } else {
        push_nested_arg(&mut display_args, &mut process_args, "--no-dashboard");
    }
    push_nested_arg(&mut display_args, &mut process_args, "--no-tui");
    if let Some(agent) = agent {
        push_nested_arg(&mut display_args, &mut process_args, "--agent");
        push_nested_arg(&mut display_args, &mut process_args, agent);
    }
    if let Some(agent_mode) = agent_mode {
        push_nested_arg(&mut display_args, &mut process_args, "--agent-mode");
        push_nested_arg(&mut display_args, &mut process_args, agent_mode);
    }
    if let Some(model) = model {
        push_nested_arg(&mut display_args, &mut process_args, "--model");
        push_nested_arg(&mut display_args, &mut process_args, model);
    }

    NestedRunCommand { display_args, process_args }
}

fn push_nested_arg<S: Into<String>>(
    display_args: &mut Vec<String>,
    process_args: &mut Vec<OsString>,
    arg: S,
) {
    let arg = arg.into();
    process_args.push(OsString::from(&arg));
    display_args.push(arg);
}

fn push_nested_path_arg(
    display_args: &mut Vec<String>,
    process_args: &mut Vec<OsString>,
    path: &Path,
) {
    process_args.push(path.as_os_str().to_os_string());
    display_args.push(path.display().to_string());
}

fn invoke_nested_run(
    command: &NestedRunCommand,
    log_path: &Path,
    plan: &DiscoveredBatchPlan,
    slot: u16,
    sink: &Arc<dyn rhei_tui::EventSink>,
) -> Result<std::process::ExitStatus, String> {
    let exe = std::env::current_exe().map_err(|err| err.to_string())?;
    let log = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(log_path)
        .map_err(|err| format!("open nested run log '{}': {err}", log_path.display()))?;
    let log = Arc::new(Mutex::new(log));
    let mut child = std::process::Command::new(exe)
        .args(&command.process_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|err| err.to_string())?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let mut readers = Vec::new();
    if let Some(stdout) = stdout {
        readers.push(spawn_nested_output_reader(
            stdout,
            Arc::clone(&log),
            Arc::clone(sink),
            slot,
            plan.normalized_path.clone(),
            rhei_tui::AgentStream::Stdout,
        ));
    }
    if let Some(stderr) = stderr {
        readers.push(spawn_nested_output_reader(
            stderr,
            Arc::clone(&log),
            Arc::clone(sink),
            slot,
            plan.normalized_path.clone(),
            rhei_tui::AgentStream::Stderr,
        ));
    }

    let status = child.wait().map_err(|err| err.to_string())?;
    for reader in readers {
        let _ = reader.join();
    }
    Ok(status)
}

fn spawn_nested_output_reader<R>(
    reader: R,
    log: Arc<Mutex<fs::File>>,
    sink: Arc<dyn rhei_tui::EventSink>,
    slot: u16,
    task: String,
    stream: rhei_tui::AgentStream,
) -> std::thread::JoinHandle<()>
where
    R: Read + Send + 'static,
{
    std::thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            let read = match reader.read_until(b'\n', &mut buffer) {
                Ok(read) => read,
                Err(err) => {
                    sink.emit(rhei_tui::RunEvent::Message {
                        level: rhei_tui::MessageLevel::Warn,
                        text: format!("warning: failed reading nested output for {task}: {err}"),
                    });
                    break;
                }
            };
            if read == 0 {
                break;
            }
            if let Ok(mut log) = log.lock() {
                let _ = log.write_all(&buffer);
            }
            let line = String::from_utf8_lossy(&buffer)
                .trim_end_matches(['\r', '\n'])
                .to_string();
            if let Some(url) = line.strip_prefix("Dashboard: ") {
                sink.emit(rhei_tui::RunEvent::RunLink {
                    label: format!("{task} dashboard"),
                    url: url.to_string(),
                });
            }
            sink.emit(rhei_tui::RunEvent::AgentOutput {
                slot,
                task: task.clone(),
                stream,
                line,
                wall_clock: std::time::SystemTime::now(),
            });
        }
    })
}

fn append_batch_log_footer(log_path: &Path, message: &str) {
    if let Ok(mut file) = fs::OpenOptions::new().append(true).open(log_path) {
        let _ = writeln!(file, "\nended: {}", format_iso8601_utc(std::time::SystemTime::now()));
        let _ = writeln!(file, "status: {message}");
    }
}

fn batch_plan_log_name(plan: &DiscoveredBatchPlan) -> String {
    let mut slug = String::new();
    for ch in plan.normalized_path.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            slug.push(ch);
        } else {
            slug.push('-');
        }
    }
    format!("{:04}-{slug}.log", plan.index + 1)
}

fn batch_plan_log_path(report_dir: &Path, plan: &DiscoveredBatchPlan) -> PathBuf {
    report_dir.join("logs").join(batch_plan_log_name(plan))
}

fn batch_plan_final_state(record: &BatchPlanRecord) -> &'static str {
    match record.status {
        BatchPlanStatus::Succeeded => "completed",
        BatchPlanStatus::Failed => "failed",
        BatchPlanStatus::Skipped => "skipped",
    }
}

fn batch_plan_outcome(record: &BatchPlanRecord) -> rhei_tui::TaskOutcome {
    match record.status {
        BatchPlanStatus::Succeeded => rhei_tui::TaskOutcome::Completed,
        BatchPlanStatus::Failed => {
            rhei_tui::TaskOutcome::Failed(record.status_message.clone())
        }
        BatchPlanStatus::Skipped => rhei_tui::TaskOutcome::Cancelled,
    }
}

fn format_nested_command(args: &[String]) -> String {
    let mut parts = vec!["rhei".to_string()];
    parts.extend(args.iter().map(|arg| shell_quote(arg)));
    parts.join(" ")
}

fn write_batch_reports(
    report_dir: &Path,
    summary: &BatchSummary,
    records: &[BatchPlanRecord],
) -> MietteResult<()> {
    write_batch_json(&report_dir.join("summary.json"), summary)?;
    write_batch_json(&report_dir.join("plans.json"), records)?;
    let report = BatchReport { summary, plans: records };
    write_batch_json(&report_dir.join("batch-report.json"), &report)
}

fn write_batch_json<T: Serialize + ?Sized>(path: &Path, value: &T) -> MietteResult<()> {
    let rendered = serde_json::to_string_pretty(value)
        .map_err(|err| miette!("failed to render '{}': {err}", path.display()))?;
    fs::write(path, rendered).map_err(|err| miette!("failed to write '{}': {err}", path.display()))
}
