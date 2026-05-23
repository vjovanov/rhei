// `rhei viz` — render a self-contained Flow page for a plan or workspace,
// resolving the machine and building the same VizModel bundle as the live
// dashboard, inlined into the one asset. §FS-rhei-viz.7.2

fn viz_command(
    input: &Path,
    state_machine: Option<&Path>,
    output: Option<&Path>,
    open: bool,
) -> MietteResult<()> {
    let key = input
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("plan")
        .to_string();

    let plans = rhei_viz::collect_plans(input, &key, state_machine)
        .map_err(|err| miette!("failed to collect plans from {}: {err}", input.display()))?;
    if plans.is_empty() {
        return Err(miette!("no .rhei.md plans found at {}", input.display()));
    }

    let html = rhei_viz_model::render_static(&plans);
    let out = output.map(Path::to_path_buf).unwrap_or_else(|| default_viz_output(input));
    std::fs::write(&out, html).map_err(|err| miette!("failed to write {}: {err}", out.display()))?;
    println!("Wrote flow visualization to {}", out.display());

    if open {
        open_in_browser(&out)?;
    }
    Ok(())
}

/// Default output: `rhei-viz.html` inside a workspace directory, otherwise the
/// plan file with its extension swapped to `.html`.
fn default_viz_output(input: &Path) -> PathBuf {
    if input.is_dir() {
        input.join("rhei-viz.html")
    } else {
        input.with_extension("html")
    }
}

fn open_in_browser(path: &Path) -> MietteResult<()> {
    let (program, lead): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("open", &[])
    } else if cfg!(target_os = "windows") {
        ("cmd", &["/C", "start", ""])
    } else {
        ("xdg-open", &[])
    };
    std::process::Command::new(program)
        .args(lead)
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|err| miette!("failed to open browser: {err}"))
}
