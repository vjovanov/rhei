// §FS-rhei-memory.3.1 §FS-rhei-memory.4.2 driven end to end: a real
// stdin-delivered supervisor handoff stays compact because repository-scale
// standing context remains reachable instead of being pasted into the prompt.

use std::fs;

use super::*;

const MAX_SUPERVISOR_PROMPT_BYTES: usize = 131_072;
const EVIDENCE_MARKERS: [&str; 4] = [
    "TICKET-EVIDENCE-BEGIN",
    "VALIDATION-EVIDENCE-BEGIN",
    "SUPERVISION-PLAN-BEGIN",
    "RELATED-ARTIFACTS-BEGIN",
];

fn repository_evidence(label: &str) -> String {
    let mut evidence = format!("{label}-BEGIN\n");
    for number in 1..=450 {
        evidence.push_str(&format!(
            "{label} line {number:04}: durable repository context {}\n",
            "x".repeat(585)
        ));
    }
    evidence.push_str(&format!("{label}-END"));
    evidence
}

/// vjovanov/rhei#209: an `execute_on` state receives one compact supervisory
/// handoff through `rhei run`, even when both authored context scopes contain
/// large but individually under-cap sections. The prompt keeps its operative
/// task and navigation sections while omitting all four evidence bodies.
#[test]
fn spawned_supervisor_prompt_omits_repository_context_within_bound() {
    let dir = unique_temp_dir("memory-supervisor-context");
    let project = dir.join("project");
    fs::create_dir_all(&project).expect("create project");

    let manifest = format!(
        "# Panta: Repository-scale ticket\n\n## Ticket Evidence\n\n{}\n\n\
         ## Validation Evidence\n\n{}\n",
        repository_evidence("TICKET-EVIDENCE"),
        repository_evidence("VALIDATION-EVIDENCE")
    );
    fs::write(project.join("index.panta.md"), manifest).expect("write Panta manifest");

    let rhei = format!(
        "# Rhei: Supervision\n\n## Supervision Plan\n\n{}\n\n\
         ## Related Artifacts\n\n{}\n\n## Tasks\n\n\
         ### Task 1: Brief the selected step\n**State:** guiding\n\n\
         The ticket is already validated. Preserve the ruling and brief one selected step.\n\n\
         #### Task 1.1: Selected step\n**State:** completed\n",
        repository_evidence("SUPERVISION-PLAN"),
        repository_evidence("RELATED-ARTIFACTS")
    );
    fs::write(project.join("rhei.rhei.md"), rhei).expect("write rhei");

    let machine_path = write_fixture_file(
        &project,
        "states.yaml",
        r#"name: compact-supervisor-prompt
version: 1
states:
  guiding:
    initial: true
    description: Brief the selected step
    execute_on: descendant-terminal
    agent: capture
    agent_timeout: 30s
    visits: 5
    instructions: Preserve the ruling and brief exactly one selected step.
  completed:
    final: true
    description: Finished
  cancelled:
    final: true
    description: Cancelled
transitions:
  - from: guiding
    to: completed
    description: The selected subtree is closed
    condition: openDescendants < 1
  - from: guiding
    to: guiding
    description: Release the selected subtree
  - from: '*'
    to: cancelled
    description: Cancelled
"#,
    );

    let agent = write_python_agent(
        &project,
        "capture-agent.py",
        r#"prompt = agent_prompt()
root = pathlib.Path(env('RHEI_ROOT'))
write(root / 'runtime' / 'supervisor-prompt.md', prompt)
write(root / 'runtime' / 'supervise' / 'rhei.1.1.md', 'Brief exactly the selected step.\n')
result('## Result\n\nPreserved the ruling and briefed one selected step.\n')
"#,
    );
    let settings_dir = project.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "capture", "agent_timeout": "30s" }},
  "agents": {{ "capture": {{ "command": {}, "stdin_prompt": true, "timeout": "30s" }} }}
}}"#,
            fixture_command(&agent)
        ),
    )
    .expect("write settings");
    fs::create_dir_all(project.join("runtime/results")).expect("create result directory");
    write_fixture_file(
        &project,
        "runtime/results/rhei.1.1.md",
        "## Result\n\nThe selected step was already completed.\n",
    );

    let result = run_cli("run", &project, &machine_path, &["--no-callbacks", "--no-tui"]);
    assert_success(&result);

    let prompt = fs::read_to_string(project.join("runtime/supervisor-prompt.md"))
        .expect("capture the supervisor prompt");
    assert!(prompt.contains("Preserve the ruling and brief one selected step."), "got:\n{prompt}");
    assert!(prompt.contains("## Child Tasks"), "got:\n{prompt}");
    assert!(prompt.contains("### Reading the rhei"), "got:\n{prompt}");
    assert!(prompt.contains("rhei render <plan> --format json --pretty"), "got:\n{prompt}");

    let prompt_bytes = prompt.len();
    let embedded: Vec<&str> =
        EVIDENCE_MARKERS.into_iter().filter(|marker| prompt.contains(marker)).collect();
    assert!(
        prompt_bytes <= MAX_SUPERVISOR_PROMPT_BYTES && embedded.is_empty(),
        "supervisor handoff was {prompt_bytes} bytes and embedded {}/4 repository-scale evidence markers {:?}; expected at most {MAX_SUPERVISOR_PROMPT_BYTES} bytes and no markers",
        embedded.len(),
        embedded
    );
}
