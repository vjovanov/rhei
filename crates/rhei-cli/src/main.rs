use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "rhei",
    author,
    version,
    about = "Rhei - Markdown plan compiler scaffold",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// A no-op placeholder subcommand for scaffolding and verification
    Noop,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Noop) => {
            println!("noop executed");
        }
        None => {
            println!("{}", rhei_core::help_text());
        }
    }

    Ok(())
}
