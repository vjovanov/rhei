use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use miette::{miette, LabeledSpan, NamedSource, Report, Result as MietteResult, SourceSpan};
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_STATE_MACHINE_PATH: &str = "docs/state-machine.yaml";

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

#[derive(Subcommand, Debug)]
enum Commands {
    /// Validate a markdown plan against the configured state machine
    Validate {
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

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum RenderFormat {
    Json,
    Github,
    Progress,
}


fn main() {
    if let Err(err) = run() {
        eprintln!("{err:?}");
        std::process::exit(1);
    }
}

fn run() -> MietteResult<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Validate { input } => validate_command(&input, &cli.state_machine),
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

fn read_input_file(path: &Path) -> MietteResult<String> {
    fs::read_to_string(path).map_err(|err| file_io_report(path, "failed to read input file", err))
}

fn parse_input_file(path: &Path) -> MietteResult<rhei_core::ast::Saga> {
    let input = read_input_file(path)?;
    rhei_core::parse(&input).map_err(|err| parse_report(path, &input, &err))
}

fn validate_command(input: &Path, state_machine: &Path) -> MietteResult<()> {
    let saga = parse_input_file(input)?;
    let report =
        rhei_validator::validate_from_machine_file(&saga, state_machine).map_err(|err| {
            file_io_report(
                state_machine,
                "failed to load state machine",
                err,
            )
        })?;

    if report.has_errors() {
        for warning in &report.warnings {
            eprintln!("warning: {warning}");
        }

        return Err(validation_report(input, state_machine, &report.errors));
    }

    println!("Validation succeeded");
    for warning in &report.warnings {
        println!("warning: {warning}");
    }

    Ok(())
}

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

fn print_versions() {
    println!("rhei-cli {}", env!("CARGO_PKG_VERSION"));
    println!("rhei-core {}", rhei_core::version());
    println!("rhei-validator {}", rhei_validator::version());
    println!("rhei-output {}", rhei_output::version());
}

fn parse_report(path: &Path, input: &str, err: &rhei_core::parser::ParseError) -> Report {
    let message = format!("failed to parse '{}': {}", path.display(), err.message);

    if let Some(line_number) = err.line {
        let span = line_span(input, line_number);
        let line_text = line_text(input, line_number);
        let diagnostic = miette!(
            "{message}\nhelp: parser reported line {line_number}: {}",
            line_text.unwrap_or("<line unavailable>")
        );
        if let Some(span) = span {
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

fn file_io_report(path: &Path, action: &str, err: impl std::fmt::Display) -> Report {
    miette!("{action} '{}': {err}", path.display())
}

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
            Commands::Validate { input } => {
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
    fn clap_command_factory_builds() {
        Cli::command().debug_assert();
    }
}
