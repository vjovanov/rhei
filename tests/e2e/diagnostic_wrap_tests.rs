//! The one thing an end-to-end assertion may not depend on: where the renderer
//! happened to wrap.
//!
//! Captured stderr is never a tty, so miette renders every diagnostic wrapped
//! at eighty columns. It breaks only at spaces, which is exactly what
//! §FS-rhei-errors.2 asks of it, so the sentence arrives whole and the binary
//! is right. What is wrong is reading a rendered line as if it were the
//! sentence: a phrase that straddles the break is invisible to `contains`, and
//! where the break lands moves with a pid, a temporary path or a run id. The
//! failure is then a property of the machine rather than of the change under
//! test, which is the worst shape a test failure can take.

use std::process::{ExitStatus, Output};

use super::{assert_stderr_contains, raw_stderr, repo_root, rhei_command, stderr, unique_temp_dir};
use super::{CliRun, TestDir};

/// Captured output carrying `text` on stderr, so a rule about the reading can
/// be stated against the exact bytes it is a rule about.
fn captured(text: &str) -> Output {
    Output { status: ExitStatus::default(), stdout: Vec::new(), stderr: text.as_bytes().to_vec() }
}

/// A missing plan path long enough to occupy a rendered line by itself.
///
/// Forcing the width by construction is what makes this test say the same
/// thing everywhere. Left to the environment, the wrap column is decided by
/// the length of the temp directory the runner handed out, and the test would
/// be green on the machines that never see the defect — which is how this
/// reached two open issues and three supervised runs before it was caught.
fn missing_plan_path_wider_than_the_render(dir: &TestDir) -> String {
    let mut name = String::new();
    let mut path = dir.join("plan.rhei.md").to_string_lossy().into_owned();
    while path.len() < 96 {
        name.push('w');
        path = dir.join(format!("{name}.rhei.md")).to_string_lossy().into_owned();
    }
    path
}

/// A sentence rendered across two lines still reads as one sentence.
/// §FS-rhei-errors.2
#[test]
fn a_wrapped_diagnostic_reads_as_the_sentence_the_binary_printed() {
    const SENTENCE: &str = "or omit the argument to use the enclosing project";

    let dir = unique_temp_dir("diagnostic-wrap");
    let home = dir.join(".home");
    let missing = missing_plan_path_wider_than_the_render(&dir);
    let out = rhei_command(&home).arg("next").arg(&missing).output().expect("rhei next should run");

    let rendered = raw_stderr(&out);
    assert!(
        rendered.contains("no plan or project at"),
        "the missing-path diagnostic was not reached, so this test proves nothing: {rendered}"
    );
    assert!(
        !rendered.contains(SENTENCE),
        "the wrap no longer falls inside the sentence, so this test has gone vacuous: \
         lengthen the path or pick a phrase that still straddles a break. Rendered:\n{rendered}"
    );
    assert!(
        stderr(&out).contains(SENTENCE),
        "the guidance is one sentence and must read as one: {}",
        stderr(&out)
    );
}

/// Undoing the wrap must not make a phrase out of two things the binary said
/// separately: the message and its help are different blocks, and a `contains`
/// that runs from one into the other asserts text nobody printed.
#[test]
fn undoing_the_wrap_never_joins_a_message_to_its_help() {
    let out = captured(concat!(
        "  \u{d7} refusing to stop run reuse1 because pid 100662 does not own its recorded\n",
        "  \u{2502} run lock\n",
        "  help: check `rhei runs`; the descriptor may be stale or the lock may belong\n",
        "        to another process\n",
    ));

    let text = stderr(&out);
    assert!(
        !text.contains("run lock help"),
        "the message ran into its help block, so an assertion can now match text \
         that was never printed:\n{text}"
    );
}

/// A continuation is rejoined with the single space the wrap removed, never
/// with nothing.
///
/// miette breaks only at spaces, so one space is the exact inverse of the
/// wrap. Joining with the empty string would instead heal a token broken
/// mid-word — the product defect #114 was filed for — and the suite would stop
/// being able to see it. §FS-rhei-errors.2
#[test]
fn undoing_the_wrap_leaves_a_token_broken_mid_word_broken() {
    let out = captured(concat!(
        "  \u{d7} no plan or project at '/tmp/rhei/pl\n",
        "  \u{2502} an.rhei.md'.\n",
    ));

    let text = stderr(&out);
    assert!(
        !text.contains("/tmp/rhei/plan.rhei.md"),
        "a mid-token break was healed into a path the binary never printed, so the \
         suite can no longer catch one:\n{text}"
    );
}

/// Only a wrapped diagnostic is unwrapped. Everything else on stderr — a
/// program's own output, a log line, a Python traceback — is two lines because
/// it was written as two lines, and joining those would invent a sentence.
#[test]
fn stderr_that_is_not_a_wrapped_diagnostic_is_left_alone() {
    const PLAIN: &str = "starting task one\nstarting task two\n";

    let out = captured(PLAIN);
    assert_eq!(stderr(&out), PLAIN, "unrelated stderr lines were rewritten");
}

/// The assertion helper reads the unwrapped text, spacing intact.
///
/// It collapses both sides to their ASCII-graphic characters today, which
/// ignores the wrap by ignoring every space — and so matches a phrase across a
/// block boundary, and heals a mid-token break, for all sixty-one call sites.
/// Tolerating a false failure is a nuisance; producing a false pass is a test
/// suite that has stopped being evidence.
#[test]
fn the_stderr_assertion_refuses_a_phrase_the_binary_never_printed() {
    let run = CliRun {
        status: ExitStatus::default(),
        stdout: String::new(),
        stderr: concat!(
            "  \u{d7} no plan or project at '/tmp/rhei/pl\n",
            "  \u{2502} an.rhei.md'.\n",
        )
        .to_string(),
    };

    let matched = std::panic::catch_unwind(|| {
        assert_stderr_contains(&run, "/tmp/rhei/plan.rhei.md");
    });
    assert!(
        matched.is_err(),
        "the helper matched a path stitched out of two rendered lines; a phrase it \
         accepts must be one the binary actually printed"
    );
}

/// The reading is the harness's, not each test's.
///
/// A safety that has to be opted into is missed by the next assertion someone
/// writes, and this one fails on a single developer's machine and nowhere
/// else, so it is missed for months. Turning captured stderr into text is
/// therefore the harness's job at one seam; a file that does it itself has
/// opted out without meaning to.
#[test]
fn every_end_to_end_file_reads_stderr_through_the_harness() {
    // Spelled in halves so this file is not its own first finding.
    let conversion = concat!("from_", "utf8");
    let e2e = repo_root().join("tests").join("e2e");

    let mut opted_out = Vec::new();
    let mut files: Vec<_> = std::fs::read_dir(&e2e)
        .expect("the end-to-end directory should be readable")
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        // `mod.rs` is the seam: it is the one place allowed to hold the bytes.
        .filter(|path| path.file_name().is_some_and(|name| name != "mod.rs"))
        .collect();
    files.sort();

    for path in files {
        let text = std::fs::read_to_string(&path).expect("an end-to-end source should be readable");
        for (index, _) in text.match_indices(conversion) {
            if !text[index..].chars().take(60).collect::<String>().contains("stderr") {
                continue;
            }
            let line = text[..index].lines().count();
            let name = path.file_name().expect("file name").to_string_lossy().into_owned();
            opted_out.push(format!("{name}:{line}"));
        }
    }

    assert!(
        opted_out.is_empty(),
        "these read captured stderr themselves instead of through `stderr(&out)`, so \
         miette's wrap decides whether their assertions match:\n  {}",
        opted_out.join("\n  ")
    );
}
