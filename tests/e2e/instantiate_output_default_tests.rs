//! Where an instantiated workspace lands when `--output` is omitted, as the
//! documents that describe the flag say it does.
//!
//! The default is specified and implemented: `<project>/<template-name>/`
//! inside a Panta project, `./<template-name>/` outside one, because a
//! workspace written to the working directory instead landed where discovery
//! never looks — the command reported success and `rhei list` still said the
//! project had no tickets. Two documents describe the flag and neither named
//! that path. The template-writer skill's `--output` row carried the refusal
//! constraint ("must not already exist") without ever saying which path it
//! refuses to overwrite, and `rhei instantiate --help` printed `Output
//! directory` against a specification that writes the help string out in full.
//! A reader of either could not tell a user where the workspace would appear,
//! nor reason about a refusal aimed at a path they had never been shown.
//!
//! Both tests match on substance and never on wording: any statement naming
//! the path shape satisfies them, so each document stays free to phrase the
//! fact its own way. §FS-rhei-templates.6.2

use std::fs;
use std::path::Path;

use super::*;

/// The skill an agent reads before it writes or instantiates a template.
const SKILL: &str = "crates/rhei-cli/skills/rhei-template-writer/SKILL.md";

/// The section of that skill which documents the `rhei instantiate` flags. The
/// statement belongs here rather than anywhere else in the file: the reader who
/// has found the `--output` row is the reader who needs it.
const FLAG_SECTION: &str = "## Instantiation CLI Surface";

/// The checked-in-example convention, which the skill uses for an unrelated
/// purpose and which would otherwise match while teaching a reader nothing
/// about where instantiation writes.
const EXAMPLE_CONVENTION: &str = "examples/<template-name>";

/// Does this text name the path shape an instantiated workspace lands at?
///
/// What counts is a template-name placeholder used as a directory component:
/// `<project>/<template-name>/`, `./<template-name>/` and `{template_name}/`
/// all satisfy it, while the bare `<template>` of a usage line does not. The
/// match is deliberately vocabulary-free — a document that states the fact
/// without the words "default" or "omitted" has stated it just as well, so
/// nothing here pins a sentence.
fn names_the_default_output_path(text: &str) -> bool {
    text.lines()
        .filter(|line| !line.contains(EXAMPLE_CONVENTION))
        .any(names_a_template_named_directory)
}

fn names_a_template_named_directory(line: &str) -> bool {
    for (open, close) in [('<', '>'), ('{', '}')] {
        let mut cursor = 0;
        while let Some(start) = line[cursor..].find(open).map(|at| cursor + at) {
            let Some(end) = line[start..].find(close).map(|at| start + at) else { break };
            let after = end + close.len_utf8();
            let inner = &line[start + open.len_utf8()..end];
            let beside_a_slash = line[..start].ends_with('/') || line[after..].starts_with('/');
            if beside_a_slash && is_template_placeholder(inner) {
                return true;
            }
            cursor = after;
        }
    }
    false
}

fn is_template_placeholder(inner: &str) -> bool {
    inner.contains("template")
        && inner.chars().all(|c| c.is_ascii_lowercase() || c == '_' || c == '-')
}

/// The body under `heading`, up to the next `## ` heading.
fn section(document: &str, heading: &str) -> Option<String> {
    let mut lines = document.lines();
    lines.find(|line| line.trim_end() == heading)?;
    Some(lines.take_while(|line| !line.starts_with("## ")).collect::<Vec<_>>().join("\n"))
}

/// One flag's entry in a long help listing, its wrapping flattened away.
///
/// Clap indents an entry's description under the flag line and separates
/// entries by a blank line, so the entry runs until the next line that is
/// neither blank nor part of the description. Flattening it onto one line is
/// what keeps a wrap from splitting the path the assertion is looking for.
fn help_entry(help: &str, flag: &str) -> Option<String> {
    const DESCRIPTION_INDENT: &str = "          ";
    let mut lines = help.lines().skip_while(|line| !line.trim_start().starts_with(flag));
    let flag_line = lines.next()?;
    let described =
        lines.take_while(|line| line.trim().is_empty() || line.starts_with(DESCRIPTION_INDENT));
    Some(
        std::iter::once(flag_line)
            .chain(described)
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn instantiate_help(cwd: &Path) -> String {
    let output = rhei_command(cwd.join(".home"))
        .current_dir(cwd)
        .args(["instantiate", "--help"])
        .output()
        .expect("`rhei instantiate --help` should run");
    assert!(
        output.status.success(),
        "`rhei instantiate --help` should succeed; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The skill's own account of the instantiation flags says where an omitted
/// `--output` writes, and cites the point that specifies it.
///
/// It said neither. Every `rhei instantiate` in the skill that writes anything
/// passes `--output`, so a reader who followed the skill literally never
/// learned the flag was optional, let alone where leaving it off would put the
/// workspace.
// §FS-rhei-templates.6.2
#[test]
fn the_skill_names_where_an_omitted_output_writes() {
    let skill =
        fs::read_to_string(repo_root().join(SKILL)).expect("read the template-writer skill");
    let section = section(&skill, FLAG_SECTION).unwrap_or_else(|| {
        panic!(
            "`{FLAG_SECTION}` should be the section of {SKILL} documenting the instantiate flags"
        )
    });

    assert!(
        names_the_default_output_path(&section),
        "`{FLAG_SECTION}` must name where an instantiated workspace lands when \
         --output is omitted — <project>/<template-name>/ inside a Panta project, \
         else ./<template-name>/ — in whatever wording suits it; got:\n{section}"
    );
    assert!(
        section.contains("FS-rhei-templates.6.2"),
        "and it must cite the point that specifies that default, so the two \
         cannot drift apart unnoticed; got:\n{section}"
    );
}

/// And so does the help clap renders, which the specification writes out in
/// full rather than describing.
///
/// The doc comment behind the flag read `Output directory` and stopped there,
/// leaving a `--help` reader exactly where the skill's reader was: told that a
/// path must not already exist, and never told which path.
// §FS-rhei-templates.6.1
#[test]
fn instantiate_help_names_the_default_output_directory() {
    let dir = unique_temp_dir("instantiate-help-output-default");
    let help = instantiate_help(&dir);
    let entry = help_entry(&help, "--output")
        .unwrap_or_else(|| panic!("`instantiate --help` should list --output; got:\n{help}"));

    assert!(
        names_the_default_output_path(&entry),
        "`rhei instantiate --help` must document where --output defaults to — \
         <project>/<template-name>/ inside a Panta project, else \
         ./<template-name>/; got:\n{entry}"
    );
}
