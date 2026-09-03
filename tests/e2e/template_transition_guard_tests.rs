//! Guard: no built-in template may print a `rhei transition` invocation that
//! the CLI would reject. `--from` is the compare-and-swap guard the command is
//! built on and is a required argument, so a template that shows an invocation
//! without it hands the reader a command that always exits 2 (#117).
//!
//! §FS-rhei-transition-cmd.2: the options table marks `--from <STATE>` as
//! required, and §FS-rhei-transition-cmd says why — only the caller whose
//! expected `--from` matches the task's actual state wins the race.

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

/// How far past `rhei transition` an invocation is read when no backtick ends
/// it first, which is what a fenced code block looks like after normalization.
const COMMAND_WINDOW: usize = 240;

/// Every run of whitespace becomes one space, so a command wrapped across
/// lines (and across a YAML block scalar's indentation) reads as one command.
fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The invocation that starts at `rhei transition`: up to the backtick that
/// closes its code span, or `COMMAND_WINDOW` characters, whichever comes first.
fn command_span(normalized: &str, start: usize) -> &str {
    let rest = &normalized[start..];
    let end = rest.find('`').unwrap_or(COMMAND_WINDOW).min(COMMAND_WINDOW).min(rest.len());
    &rest[..end]
}

fn text_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read template directory") {
        let entry = entry.expect("template entry");
        let path = entry.path();
        if entry.file_type().expect("file type").is_dir() {
            text_files(&path, out);
        } else if fs::read_to_string(&path).is_ok() {
            out.push(path);
        }
    }
}

/// A mention of the command by name (`cancel it with `rhei transition``) is
/// not an invocation; one that shows `--to` is, and it must show `--from` too.
#[test]
fn every_template_transition_invocation_names_from() {
    let templates_root = repo_root().join("crates/rhei-cli/templates");
    let mut files = Vec::new();
    text_files(&templates_root, &mut files);
    assert!(!files.is_empty(), "no template files found under {}", templates_root.display());

    let mut offenders = Vec::new();
    for path in &files {
        let normalized = normalize_whitespace(&fs::read_to_string(path).expect("read template"));
        for (offset, _) in normalized.match_indices("rhei transition") {
            let command = command_span(&normalized, offset);
            if command.contains("--to") && !command.contains("--from") {
                let rel = path.strip_prefix(&templates_root).unwrap_or(path);
                offenders.push(format!("{}: {command}", rel.display()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "these templates print a `rhei transition` invocation without the required \
         `--from` (the CLI exits 2 on it):\n{}",
        offenders.join("\n")
    );
}
