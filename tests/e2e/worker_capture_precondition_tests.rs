//! Whether the shipped worker skill's capture instructions can be followed by
//! the agent they are written for.
//!
//! `rhei-plan-worker` is handed one plan and forbidden from looking for a
//! project around it, so it is routinely at work on a lone `.rhei.md`. Its
//! *Capturing Work You Did Not Come For* section prescribes `rhei new --under
//! basin` for out-of-scope work it turns up. The basin exists only inside a
//! Panta project, so on a lone plan the command refuses and offers `rhei init`
//! — the one remedy the same section reserves for a human. An agent that
//! followed the document literally was left with no path for work it had just
//! been told to capture.
//!
//! The section therefore has to carry both halves: the precondition, and what
//! to do without a project. The assertions match on substance and never on
//! wording, so the section stays free to say each of them its own way, and both
//! have to be said in one breath rather than assembled from separate sentences.
//! §FS-rhei-new.3.5

use std::fs;

use super::*;

/// The skill an agent reads before it works a plan.
const SKILL: &str = "crates/rhei-cli/skills/rhei-plan-worker/SKILL.md";

/// The section that prescribes the capture. The statements belong here rather
/// than anywhere else in the file: the reader who has found the `rhei new`
/// line is the reader who is about to run it.
const CAPTURE_SECTION: &str = "## Capturing Work You Did Not Come For";

/// The point that specifies the precondition, which the section has to cite so
/// the two cannot drift apart unnoticed.
const SPEC_POINT: &str = "FS-rhei-new.3.5";

/// The body under `heading`, up to the next `## ` heading.
fn section(document: &str, heading: &str) -> Option<String> {
    let mut lines = document.lines();
    lines.find(|line| line.trim_end() == heading)?;
    Some(lines.take_while(|line| !line.starts_with("## ")).collect::<Vec<_>>().join("\n"))
}

/// The section's text broken at every `.`, lowercased.
///
/// A claim has to be made inside one of these runs, which is what keeps two
/// unrelated sentences from satisfying an assertion between them. Splitting on
/// the bare character rather than on sentence boundaries only ever makes the
/// runs shorter — `--under basin`, `.rhei.md` and a citation's own `3.5` all
/// cut one early — so nothing passes that a reader would not accept.
fn claims(section: &str) -> Vec<String> {
    section.split('.').map(str::to_ascii_lowercase).collect()
}

/// Is the capture still prescribed here at all?
///
/// The gate is a pair: deleting the prescription would satisfy the two
/// assertions below while leaving a worker with no capture instruction, which
/// is not the fix.
fn prescribes_the_basin_capture(section: &str) -> bool {
    section.contains("--under basin")
}

/// Does some claim tie the capture to a Panta project?
///
/// What counts is "project" spoken about restrictively — only there, required,
/// needed, or its absence named. A statement phrased around the lone plan the
/// worker is usually holding counts too.
fn states_the_project_precondition(section: &str) -> bool {
    const RESTRICTIONS: [&str; 8] =
        ["only", "require", "need", "outside", "without", "no project", "not in", "lone plan"];
    claims(section).iter().any(|claim| {
        claim.contains("project") && RESTRICTIONS.iter().any(|word| claim.contains(word))
    })
}

/// Does some claim say what a worker does when there is no project?
///
/// The remedy the CLI offers is `rhei init`, which this same skill reserves for
/// a human, so the only thing left is to hand the capture back: say it in the
/// result and carry on. Any verb for saying it satisfies this.
fn says_what_to_do_without_a_project(section: &str) -> bool {
    const SAYING: [&str; 5] = ["report", "record", "note", "surface", "say so"];
    claims(section)
        .iter()
        .any(|claim| claim.contains("result") && SAYING.iter().any(|word| claim.contains(word)))
}

/// The section that tells a worker to capture with `--under basin` says when
/// that is possible, and what to do when it is not.
///
/// It said neither. It prescribed the capture unconditionally, and the refusal
/// a lone plan answers with points at `rhei init` — which the next paragraph of
/// the same section forbids the worker from running.
// §FS-rhei-new.3.5
#[test]
fn the_worker_skill_states_the_capture_precondition_and_its_fallback() {
    let skill = fs::read_to_string(repo_root().join(SKILL)).expect("read the plan-worker skill");
    let section = section(&skill, CAPTURE_SECTION).unwrap_or_else(|| {
        panic!("`{CAPTURE_SECTION}` should be the section of {SKILL} prescribing the capture")
    });

    assert!(
        prescribes_the_basin_capture(&section),
        "`{CAPTURE_SECTION}` must still prescribe `rhei new --under basin` — dropping the \
         capture is not the fix; got:\n{section}"
    );
    assert!(
        states_the_project_precondition(&section),
        "and it must say that the basin exists only inside a Panta project, in whatever \
         wording suits it, so a worker holding a lone plan knows the command will refuse; \
         got:\n{section}"
    );
    assert!(
        says_what_to_do_without_a_project(&section),
        "and it must say what to do without one — report the capture in the result rather \
         than create a project, since this same section reserves that for a human; \
         got:\n{section}"
    );
    assert!(
        section.contains(SPEC_POINT),
        "and it must cite {SPEC_POINT}, the point that specifies the precondition, so the \
         document and the command cannot drift apart; got:\n{section}"
    );
}
