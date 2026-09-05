//! Validate-and-retry loop with a warm prompt cache.
//!
//! Worked example for: "validate the agent's output, and if it isn't correct
//! keep working with the agent — up to a bounded number of retries — without
//! throwing the work away or paying full input tokens on every retry."
//!
//! Shape (a SINGLE self-iterating counted state — the number of states is
//! constant regardless of the retry count):
//!
//!   build (agent, `visits: N`)
//!     • emits + inherits its OWN session snapshot (`from: self`), so each
//!       retry RESUMES the prior conversation — identical prompt prefix =>
//!       prompt-cache hit.
//!     • an `on_leave` validation callback runs after every visit:
//!         - output valid   -> proceed to `completed`
//!         - output invalid -> redirect back to `build` (retry, warm cache),
//!           leaving the findings (the error message) in a file the agent reads
//!     • `visitCount >= visits` -> `human-review` once retries are exhausted.
//!
//! "Don't throw the output away" is automatic: snapshots are session
//! transcripts only — Rhei never reverts the working tree — so the retry edits
//! the same files, and `inherit: from: self` carries the agent's reasoning.
//!
//! The mock agent stands in for `claude-code`. Built-in agents are the only
//! ones with token-accounting extractors, so a mock cannot report genuine
//! token counts. The honest, observable proof that "the tokens would be
//! cached" is therefore the cache PRECONDITION, not a fabricated number:
//!   1. the retry visit is spawned with `--resume <prior-visit session id>`
//!      (logged by the mock agent), replaying an identical prompt prefix, and
//!   2. the orchestrator does NOT warn that the snapshot "may not be
//!      cache-beneficial" — i.e. same agent + provider + model, within TTL,
//!      which is exactly the `cache_beneficial` predicate the engine evaluates
//!      at spawn time (snapshot_runtime_emit::snapshot_cache_benefit_reason).
//!
//! These tests also guard the run-orchestrator fix that made counted
//! self-loops terminate (previously `build` vs `build-2` was mistaken for
//! forward progress and the loop spun forever).

use std::fs;
use std::path::Path;

use super::*;

/// A fake agent that models the retry: a cold first attempt leaves the work
/// `PENDING`; resuming the prior session (`--resume` present) lets it finish
/// the job and write `DONE`. It emits a session transcript into `--session-dir`
/// so the orchestrator can capture the inheritable snapshot, and logs each
/// invocation so the test can see whether the retry resumed.
fn write_retry_agent(dir: &Path) -> PathBuf {
    write_python_agent(
        dir,
        "retry-agent.py",
        r#"session_dir = ''
resume_value = ''
args = sys.argv[1:]
while args:
    flag = args.pop(0)
    if flag == '--session-dir':
        session_dir = args.pop(0) if args else ''
    elif flag == '--resume':
        resume_value = args.pop(0) if args else ''
    elif flag in ('--prompt', '--model'):
        if args:
            args.pop(0)

runtime_root = pathlib.Path(env('RHEI_ROOT', '.')) / 'runtime'
append(
    runtime_root / 'retry-agent.log',
    'task={} state={} target={} resume={}\n'.format(
        env('RHEI_TASK_ID'), env('RHEI_STATE'), env('RHEI_TARGET_SLUG'), resume_value
    ),
)

# Cold run leaves the work incomplete; a resumed (cache-warm) run finishes it.
# Markers must not be substrings of one another.
marker = 'DONE' if resume_value else 'PENDING'
write(runtime_root / 'result.txt', marker + '\n')

# `build` declares an edge to `completed`, so its prompt carries the result path
# and the engine expects one on the way out. §FS-rhei-states.3.3
result('## Result\n\nAttempt in {} left the artifact {}.\n'.format(env('RHEI_STATE'), marker))

# Emit the session transcript so the state can capture an inheritable snapshot.
if session_dir:
    session_id = '{}-{}-{}'.format(
        env('RHEI_TASK_ID'), env('RHEI_STATE'), env('RHEI_TARGET_SLUG', 'target')
    )
    write(
        pathlib.Path(session_dir) / (session_id + '.jsonl'),
        '{{"session":{{"provider":"{}","model":"{}"}}}}\n'
        '{{"role":"assistant","content":"{}"}}\n'.format(
            env('RHEI_MODEL_PROVIDER', 'acme'),
            env('RHEI_MODEL_NAME', 'model-a'),
            env('RHEI_STATE'),
        ),
    )
"#,
    )
}

/// Register the mock agent with a resume-capable session layout so the
/// orchestrator can preload a prior snapshot and pass `--resume`.
fn write_agent_settings(dir: &Path, agent_script: &Path) {
    let settings_dir = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create .agent-grounds/rhei");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "fake": {{
      "command": {},
      "prompt_flag": "--prompt",
      "model_flag": "--model",
      "timeout": "5s",
      "session": {{
        "resume": {{"flag": "--resume"}},
        "session_dir_flag": "--session-dir",
        "layout": {{"kind": "FlatById", "ext": "jsonl"}}
      }}
    }}
  }}
}}"#,
            fixture_command(agent_script)
        ),
    )
    .expect("write settings");
}

const PLAN: &str = r#"# Rhei: Validate Retry Loop

## Tasks

### Task 1: Produce a complete artifact
**State:** build
"#;

/// Build the state machine. `validate_script` is the `on_leave` validation
/// callback that decides pass (proceed to `completed`) vs. fail (redirect back
/// to `build`).
fn machine_yaml(validate_command: &str) -> String {
    format!(
        r#"name: validate-retry-loop
version: 1
states:
  build:
    initial: true
    description: Implement the task; on a retry resume the prior session and fix only the findings.
    target: fake:acme:model-a
    visits: 4
    snapshot:
      emit: {{ name: loop, on: always }}
      inherit: {{ name: loop, from: self, required: false }}
  human-review:
    description: Retries exhausted; await human inspection.
    gating: true
  completed:
    description: Validation passed.
    final: true
transitions:
  - {{ from: build, to: completed, condition: visitCount < visits, on_leave: 'cli:{validate_command}' }}
  - {{ from: build, to: build, condition: visitCount < visits }}
  - {{ from: build, to: human-review, condition: visitCount >= visits }}
"#
    )
}

#[test]
fn validate_retry_loop_resumes_session_and_stays_cache_beneficial() {
    let dir = unique_temp_dir("validate-retry-cache");
    let agent = write_retry_agent(&dir);
    write_agent_settings(&dir, &agent);

    // Validation callback: pass once the artifact is DONE, otherwise redirect
    // back to `build` and leave the findings (the error message) on disk.
    let validate = write_python_agent(
        &dir,
        "validate.py",
        r#"runtime = pathlib.Path(env('RHEI_PLAN_PATH', '.')).parent / 'runtime'
artifact = runtime / 'result.txt'
done = artifact.is_file() and 'DONE' in artifact.read_text(encoding='utf-8')
if done:
    write(runtime / 'findings.md', 'all checks passed\n')
    print('{"success": true}')
else:
    write(runtime / 'findings.md', 'incomplete: result.txt is not DONE yet - keep working\n')
    print('{"success": true, "nextState": "build"}')
"#,
    );

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine_path =
        write_fixture_file(&dir, "states.yaml", &machine_yaml(&fixture_command_line(&validate)));

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui"]);
    assert_success(&result);

    // The loop terminated at `completed`: the cold attempt failed validation,
    // the resumed attempt passed it. It did NOT escalate to human-review.
    assert_task_state(&plan_path, &machine_path, "1", "completed");

    let agent_log =
        fs::read_to_string(dir.join("runtime/retry-agent.log")).expect("retry agent log");

    // Exactly two build invocations: one cold attempt, then one warm retry.
    let build_invocations = agent_log.lines().filter(|l| l.contains("state=build")).count();
    assert_eq!(
        build_invocations, 2,
        "build should run cold once and then retry once; got:\n{agent_log}"
    );

    // The retry RESUMED the prior visit's session: identical prompt prefix, so
    // the input tokens are served from cache. The first attempt is cold.
    let cold = agent_log.lines().filter(|l| l.trim_end().ends_with("resume=")).count();
    let warm =
        agent_log.lines().filter(|l| l.contains("resume=plan.1-build-fake-acme-model-a")).count();
    assert_eq!(cold, 1, "the first build attempt must be a cold start; got:\n{agent_log}");
    assert_eq!(warm, 1, "the retry must resume the prior build session; got:\n{agent_log}");

    // Cache-beneficial: same agent + provider + model, so the orchestrator does
    // NOT emit the "may not be cache-beneficial" advisory (a provider/model
    // mismatch is the only thing that prints it).
    assert!(
        !result.stderr.contains("may not be cache-beneficial"),
        "inheriting the same agent/provider/model must stay cache-beneficial; stderr:\n{}",
        result.stderr
    );

    // The work was not thrown away between attempts: the retry completed the
    // same result file in place, and the validator left actionable findings.
    let result_txt = fs::read_to_string(dir.join("runtime/result.txt")).expect("result file");
    assert!(result_txt.contains("DONE"), "retry should complete the artifact; got: {result_txt:?}");
    assert!(dir.join("runtime/findings.md").exists(), "validator must write a findings artifact");
}

#[test]
fn validate_retry_loop_escalates_to_human_review_when_retries_exhausted() {
    let dir = unique_temp_dir("validate-retry-exhausted");
    let agent = write_retry_agent(&dir);
    write_agent_settings(&dir, &agent);

    // A validator that never accepts the output: it always redirects back to
    // `build`. The `visits` budget must bound the retries and route to the
    // human-review gate instead of looping forever.
    let validate = write_python_agent(
        &dir,
        "validate.py",
        r#"runtime = pathlib.Path(env('RHEI_PLAN_PATH', '.')).parent / 'runtime'
write(runtime / 'findings.md', 'still failing\n')
print('{"success": true, "nextState": "build"}')
"#,
    );

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine_path =
        write_fixture_file(&dir, "states.yaml", &machine_yaml(&fixture_command_line(&validate)));

    let result = run_cli("run", &plan_path, &machine_path, &["--no-tui"]);
    assert_success(&result);

    // Bounded: the run stops at the gating human-review state rather than
    // spinning forever.
    assert_task_state(&plan_path, &machine_path, "1", "human-review");

    // Retries stayed cache-warm throughout: every attempt after the first
    // resumed the prior session.
    let agent_log =
        fs::read_to_string(dir.join("runtime/retry-agent.log")).expect("retry agent log");
    let warm =
        agent_log.lines().filter(|l| l.contains("resume=plan.1-build-fake-acme-model-a")).count();
    assert!(warm >= 1, "retries should resume the prior session; got:\n{agent_log}");
}
