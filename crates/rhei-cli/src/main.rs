use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use miette::{miette, NamedSource, Report, Result as MietteResult, SourceSpan};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

/// Default state-machine file used by validation commands when `--state-machine`
/// is not provided.
const DEFAULT_STATE_MACHINE_PATH: &str = "docs/state-machine.yaml";

/// Command-line interface for the markdown plan compiler.
#[derive(Parser, Debug)]
#[command(
    name = "rhei",
    author,
    version,
    about = "Validate and compile markdown plans into structured outputs",
    long_about = None,
    arg_required_else_help = true
)]
struct Cli {
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        default_value = DEFAULT_STATE_MACHINE_PATH,
        help = "Path to the state machine YAML used by validation commands"
    )]
    state_machine: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

/// Supported CLI subcommands.
#[derive(Subcommand, Debug)]
enum Commands {
    /// Validate a markdown plan against the configured state machine
    Validate {
        /// Re-run validation when the plan or state machine changes
        #[arg(long)]
        watch: bool,
        /// Path to the markdown plan file
        input: PathBuf,
    },
    /// Render a markdown plan into a selected output format
    Render {
        /// Path to the markdown plan file
        input: PathBuf,
        /// Output format
        #[arg(long, value_enum)]
        format: RenderFormat,
        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,
        /// Disable ANSI color in progress output
        #[arg(long)]
        no_color: bool,
        /// Omit metadata in GitHub markdown output
        #[arg(long)]
        no_metadata: bool,
        /// Omit subtask content in GitHub markdown output
        #[arg(long)]
        no_content: bool,
    },
    /// Print versions for the CLI and related crates
    Version,
}

/// Output formats supported by the [`Render`](Commands::Render) subcommand.
#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum RenderFormat {
    Json,
    Github,
    Progress,
}


/// Program entry point.
///
/// Delegates to [`run()`](run) so tests can exercise the fallible logic directly.
fn main() {
    if let Err(err) = run() {
        eprintln!("{err:?}");
        std::process::exit(1);
    }
}

/// Parse CLI arguments and execute the requested command.
fn run() -> MietteResult<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Validate { watch, input } => {
            validate_command(&input, &cli.state_machine, watch)
        }
        Commands::Render {
            input,
            format,
            pretty,
            no_color,
            no_metadata,
            no_content,
        } => render_command(
            &input,
            format,
            pretty,
            no_color,
            no_metadata,
            no_content,
        ),
        Commands::Version => {
            print_versions();
            Ok(())
        }
    }
}

/// Read the markdown plan source file from disk.
fn read_input_file(path: &Path) -> MietteResult<String> {
    fs::read_to_string(path).map_err(|err| file_io_report(path, "failed to read input file", err))
}

/// Read and parse a markdown plan file into a [`rhei_core::ast::Saga`](rhei_core::ast::Saga).
fn parse_input_file(path: &Path) -> MietteResult<rhei_core::ast::Saga> {
    let input = read_input_file(path)?;
    rhei_core::parse(&input).map_err(|err| parse_report(path, &input, &err))
}

/// Execute the `validate` subcommand once or in watch mode.
fn validate_command(input: &Path, state_machine: &Path, watch: bool) -> MietteResult<()> {
    if watch {
        watch_validation_command(input, state_machine)
    } else {
        run_validation_once(input, state_machine)
    }
}

/// Parse a plan, load the selected state machine, and print validation results.
fn run_validation_once(input: &Path, state_machine: &Path) -> MietteResult<()> {
    let saga = parse_input_file(input)?;
    let report =
        rhei_validator::validate_from_machine_file(&saga, state_machine).map_err(|err| {
            file_io_report(
                state_machine,
                "failed to load state machine",
                err,
            )
        })?;

    print_validation_report(&report.warnings);

    if report.has_errors() {
        return Err(validation_report(input, state_machine, &report.errors));
    }

    Ok(())
}

/// Print success output and any non-fatal validation warnings.
fn print_validation_report(warnings: &[String]) {
    println!("Validation succeeded");
    for warning in warnings {
        println!("warning: {warning}");
    }
}

/// Watch the plan and state-machine files and re-run validation on relevant changes.
fn watch_validation_command(input: &Path, state_machine: &Path) -> MietteResult<()> {
    let watched_paths = canonical_watched_paths(input, state_machine);
    let watch_roots = watch_roots(input, state_machine);

    println!(
        "Watch mode started for '{}' and '{}'",
        input.display(),
        state_machine.display()
    );

    run_validation_iteration(input, state_machine);

    let (tx, rx) = mpsc::channel();
    let mut watcher: RecommendedWatcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        Config::default(),
    )
    .map_err(|err| miette!("failed to initialize file watcher: {err}"))?;

    for root in &watch_roots {
        watcher
            .watch(root, RecursiveMode::NonRecursive)
            .map_err(|err| miette!("failed to watch '{}': {err}", root.display()))?;
    }

    loop {
        let event = match rx.recv() {
            Ok(Ok(event)) => event,
            Ok(Err(err)) => {
                eprintln!("watch error: {err}");
                continue;
            }
            Err(err) => return Err(miette!("watch channel disconnected: {err}")),
        };

        if !should_revalidate(&event, &watched_paths) {
            continue;
        }

        while debounce_has_relevant_event(&rx, &watched_paths) {}

        println!("--- change detected, revalidating ---");
        run_validation_iteration(input, state_machine);
    }
}

/// Run one validation iteration in watch mode, writing any failure to stderr.
fn run_validation_iteration(input: &Path, state_machine: &Path) {
    if let Err(err) = run_validation_once(input, state_machine) {
        eprintln!("{err:?}");
    }
}

fn debounce_has_relevant_event(
    rx: &mpsc::Receiver<notify::Result<Event>>,
    watched_paths: &[PathBuf],
) -> bool {
    match rx.recv_timeout(Duration::from_millis(250)) {
        Ok(Ok(event)) => should_revalidate(&event, watched_paths),
        Ok(Err(err)) => {
            eprintln!("watch error: {err}");
            false
        }
        Err(RecvTimeoutError::Timeout) => false,
        Err(RecvTimeoutError::Disconnected) => false,
    }
}

fn should_revalidate(event: &Event, watched_paths: &[PathBuf]) -> bool {
    if !is_relevant_event_kind(&event.kind) {
        return false;
    }

    event.paths.iter().any(|path| path_matches(path, watched_paths))
}

fn is_relevant_event_kind(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_)
            | EventKind::Modify(_)
            | EventKind::Remove(_)
            | EventKind::Any
    )
}

fn path_matches(path: &Path, watched_paths: &[PathBuf]) -> bool {
    watched_paths.iter().any(|watched| paths_equivalent(path, watched))
}

fn paths_equivalent(candidate: &Path, watched: &Path) -> bool {
    if let Some(normalized_candidate) = normalize_path(candidate) {
        return normalized_candidate == watched;
    }

    let candidate_file_name = candidate.file_name();
    let watched_file_name = watched.file_name();

    candidate_file_name.is_some()
        && candidate_file_name == watched_file_name
        && candidate.components().last() == watched.components().last()
}

fn canonical_watched_paths(input: &Path, state_machine: &Path) -> Vec<PathBuf> {
    [input, state_machine]
        .into_iter()
        .map(|path| normalize_path(path).unwrap_or_else(|| path.to_path_buf()))
        .collect()
}

fn watch_roots(input: &Path, state_machine: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    for path in [input, state_machine] {
        let root = path.parent().unwrap_or_else(|| Path::new("."));
        let normalized = normalize_path(root).unwrap_or_else(|| root.to_path_buf());
        if !roots.iter().any(|existing| existing == &normalized) {
            roots.push(normalized);
        }
    }

    roots
}

fn normalize_path(path: &Path) -> Option<PathBuf> {
    path.canonicalize().ok()
}

/// Execute the `render` subcommand for the selected output format.
fn render_command(
    input: &Path,
    format: RenderFormat,
    pretty: bool,
    no_color: bool,
    no_metadata: bool,
    no_content: bool,
) -> MietteResult<()> {
    let saga = parse_input_file(input)?;
    let rendered = render_saga(&saga, format, pretty, no_color, no_metadata, no_content)
        .map_err(|err| miette!("{err}"))?;
    println!("{rendered}");
    Ok(())
}

/// Render a parsed saga into the requested output representation.
fn render_saga(
    saga: &rhei_core::ast::Saga,
    format: RenderFormat,
    pretty: bool,
    no_color: bool,
    no_metadata: bool,
    no_content: bool,
) -> Result<String> {
    match format {
        RenderFormat::Json => {
            if pretty {
                Ok(rhei_output::to_json_string_pretty(saga))
            } else {
                let value = rhei_output::to_json_value(saga);
                serde_json::to_string(&value).context("failed to serialize JSON output")
            }
        }
        RenderFormat::Github => Ok(rhei_output::GithubIssuesOutput {
            include_content: !no_content,
            include_metadata: !no_metadata,
        }
        .to_markdown(saga)),
        RenderFormat::Progress => Ok(rhei_output::ProgressReportOutput {
            color: !no_color,
            show_dependencies: true,
        }
        .to_string(saga)),
    }
}

/// Print versions for the CLI and the crates surfaced by this command.
fn print_versions() {
    println!("rhei-cli {}", env!("CARGO_PKG_VERSION"));
    println!("rhei-core {}", rhei_core::version());
    println!("rhei-validator {}", rhei_validator::version());
    println!("rhei-output {}", rhei_output::version());
}

/// Convert a parser error into a [`miette::Report`] with source context when possible.
fn parse_report(path: &Path, input: &str, err: &rhei_core::parser::ParseError) -> Report {
    let message = format!("failed to parse '{}': {}", path.display(), err.message);

    if let Some(line_number) = err.line {
        let has_span = line_span(input, line_number).is_some();
        let line_text = line_text(input, line_number);
        let diagnostic = miette!(
            "{message}\nhelp: parser reported line {line_number}: {}",
            line_text.unwrap_or("<line unavailable>")
        );
        if has_span {
            return diagnostic.with_source_code(NamedSource::new(
                path.display().to_string(),
                input.to_string(),
            ));
        }
        return diagnostic;
    }

    miette!(message).with_source_code(NamedSource::new(
        path.display().to_string(),
        input.to_string(),
    ))
}

/// Convert file I/O failures into a consistent diagnostic message.
fn file_io_report(path: &Path, action: &str, err: impl std::fmt::Display) -> Report {
    miette!("{action} '{}': {err}", path.display())
}

/// Convert validation errors into a single CLI-facing diagnostic report.
fn validation_report(input: &Path, state_machine: &Path, errors: &[String]) -> Report {
    let details = format_validation_errors(errors);
    miette!(
        "validation failed for '{}' using state machine '{}'\n{}",
        input.display(),
        state_machine.display(),
        details
    )
}

fn format_validation_errors(errors: &[String]) -> String {
    errors
        .iter()
        .map(|error| format!("  - {error}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn line_text(input: &str, line_number: usize) -> Option<&str> {
    input.lines().nth(line_number.saturating_sub(1))
}

fn line_span(input: &str, line_number: usize) -> Option<SourceSpan> {
    let mut offset = 0usize;

    for (idx, line) in input.lines().enumerate() {
        if idx + 1 == line_number {
            return Some((offset, line.len().max(1)).into());
        }

        offset += line.len() + 1;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_validate_command_with_input() {
        let cli = Cli::try_parse_from(["rhei", "validate", "docs/markdown-plan-compiler.md"])
            .expect("cli should parse");

        assert_eq!(cli.state_machine, PathBuf::from(DEFAULT_STATE_MACHINE_PATH));
        match cli.command {
            Commands::Validate { watch, input } => {
                assert!(!watch);
                assert_eq!(input, PathBuf::from("docs/markdown-plan-compiler.md"));
            }
            other => panic!("expected validate command, got {other:?}"),
        }
    }

    #[test]
    fn parses_validate_watch_command_with_input() {
        let cli = Cli::try_parse_from([
            "rhei",
            "validate",
            "--watch",
            "docs/markdown-plan-compiler.md",
        ])
        .expect("cli should parse");

        assert_eq!(cli.state_machine, PathBuf::from(DEFAULT_STATE_MACHINE_PATH));
        match cli.command {
            Commands::Validate { watch, input } => {
                assert!(watch);
                assert_eq!(input, PathBuf::from("docs/markdown-plan-compiler.md"));
            }
            other => panic!("expected validate command, got {other:?}"),
        }
    }

    #[test]
    fn parses_render_json_pretty() {
        let cli = Cli::try_parse_from([
            "rhei",
            "render",
            "docs/markdown-plan-compiler.md",
            "--format",
            "json",
            "--pretty",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Render {
                input,
                format,
                pretty,
                no_color,
                no_metadata,
                no_content,
            } => {
                assert_eq!(input, PathBuf::from("docs/markdown-plan-compiler.md"));
                assert_eq!(format, RenderFormat::Json);
                assert!(pretty);
                assert!(!no_color);
                assert!(!no_metadata);
                assert!(!no_content);
            }
            other => panic!("expected render command, got {other:?}"),
        }
    }

    #[test]
    fn parses_render_github_toggles() {
        let cli = Cli::try_parse_from([
            "rhei",
            "render",
            "docs/markdown-plan-compiler.md",
            "--format",
            "github",
            "--no-metadata",
            "--no-content",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Render {
                format,
                no_metadata,
                no_content,
                ..
            } => {
                assert_eq!(format, RenderFormat::Github);
                assert!(no_metadata);
                assert!(no_content);
            }
            other => panic!("expected render command, got {other:?}"),
        }
    }

    #[test]
    fn parses_render_progress_no_color() {
        let cli = Cli::try_parse_from([
            "rhei",
            "render",
            "docs/markdown-plan-compiler.md",
            "--format",
            "progress",
            "--no-color",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Render {
                format, no_color, ..
            } => {
                assert_eq!(format, RenderFormat::Progress);
                assert!(no_color);
            }
            other => panic!("expected render command, got {other:?}"),
        }
    }

    #[test]
    fn parses_version_command() {
        let cli = Cli::try_parse_from(["rhei", "version"]).expect("cli should parse");

        match cli.command {
            Commands::Version => {}
            other => panic!("expected version command, got {other:?}"),
        }
    }

    #[test]
    fn render_saga_json_smoke() {
        let saga = rhei_core::parse(
            r#"# Saga: Smoke

## Tasks

### Task 1: Alpha
**State:** pending
"#,
        )
        .expect("parse should succeed");

        let rendered =
            render_saga(&saga, RenderFormat::Json, true, false, false, false).expect("render ok");

        assert!(rendered.contains("\"title\": \"Smoke\""));
        assert!(rendered.contains("\"tasks\""));
    }

    #[test]
    fn parse_diagnostic_includes_line_info_when_available() {
        let input = "first line\nbad line\nthird line";
        let err = rhei_core::parser::ParseError {
            message: "unexpected token".to_string(),
            line: Some(2),
        };

        let report = parse_report(Path::new("broken.md"), input, &err);
        let rendered = format!("{report:?}");

        assert!(rendered.contains("failed to parse 'broken.md': unexpected token"));
        assert!(rendered.contains("line 2"));
        assert!(rendered.contains("bad line"));
    }

    #[test]
    fn validation_failure_formatting_aggregates_multiple_errors() {
        let rendered = format_validation_errors(&[
            "Task 1 is missing mandatory **State:** metadata".to_string(),
            "Task 2 depends on missing Task 9".to_string(),
        ]);

        assert!(rendered.contains("- Task 1 is missing mandatory **State:** metadata"));
        assert!(rendered.contains("- Task 2 depends on missing Task 9"));
        assert_eq!(rendered.lines().count(), 2);
    }

    #[test]
    fn path_matches_normalizes_paths() {
        let watched = canonical_watched_paths(
            Path::new("docs/markdown-plan-compiler.md"),
            Path::new("docs/state-machine.yaml"),
        );

        assert!(path_matches(
            Path::new("./docs/markdown-plan-compiler.md"),
            &watched
        ));
        assert!(path_matches(Path::new("docs/state-machine.yaml"), &watched));
        assert!(!path_matches(Path::new("docs/plan-language-spec.md"), &watched));
    }

    #[test]
    fn paths_equivalent_falls_back_for_nonexistent_relative_paths() {
        assert!(paths_equivalent(
            Path::new("./docs/markdown-plan-compiler.md"),
            Path::new("/tmp/project/docs/markdown-plan-compiler.md"),
        ));
        assert!(!paths_equivalent(
            Path::new("./docs/plan-language-spec.md"),
            Path::new("/tmp/project/docs/markdown-plan-compiler.md"),
        ));
    }

    #[test]
    fn should_revalidate_filters_irrelevant_events() {
        let watched = canonical_watched_paths(
            Path::new("docs/markdown-plan-compiler.md"),
            Path::new("docs/state-machine.yaml"),
        );

        let event = Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![PathBuf::from("./docs/markdown-plan-compiler.md")],
            attrs: Default::default(),
        };
        assert!(should_revalidate(&event, &watched));

        let event = Event {
            kind: EventKind::Access(notify::event::AccessKind::Read),
            paths: vec![PathBuf::from("./docs/markdown-plan-compiler.md")],
            attrs: Default::default(),
        };
        assert!(!should_revalidate(&event, &watched));
    }

    #[test]
    fn clap_command_factory_builds() {
        Cli::command().debug_assert();
    }
}
