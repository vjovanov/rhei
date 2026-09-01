// Unit coverage for the pieces of `rhei summary` that a black-box test can
// only reach through a whole fixture. §FS-rhei-summary

fn summary_machine() -> rhei_validator::StateMachine {
    serde_yaml::from_str(
        r#"name: supervised-ticket-fix
version: 1
states:
  supervising:
    initial: true
  implement: {}
  completed:
    final: true
  cancelled:
    final: true
"#,
    )
    .expect("summary test machine parses")
}

fn summary_plan(states: &[&str]) -> rhei_core::ast::Rhei {
    let mut text = String::from("# Rhei: Tally\n\n## Tasks\n");
    for (index, state) in states.iter().enumerate() {
        text.push_str(&format!(
            "\n### Task {}: Step {}\n**State:** {state}\n",
            index + 1,
            index + 1
        ));
    }
    rhei_core::parse(&text).expect("summary test plan parses")
}

fn summary_record(task_id: &str, visit: u64, started_at: &str, ended_at: &str) -> AccountingInvocationRecord {
    AccountingInvocationRecord {
        started_at: started_at.to_string(),
        ended_at: ended_at.to_string(),
        task_id: task_id.to_string(),
        visit,
        ..accounting_test_record()
    }
}

#[test]
fn task_tally_counts_terminals_in_machine_order_then_the_remainder() {
    // §FS-rhei-summary.2.1: terminal states in declaration order, with the
    // non-terminal remainder appended as `N in progress`.
    let machine = summary_machine();
    let plan = summary_plan(&["cancelled", "completed", "completed", "implement"]);
    assert_eq!(summary_task_tally(&plan, &machine), "2 tasks completed, 1 cancelled, 1 in progress");
}

#[test]
fn task_tally_stays_singular_and_names_an_empty_plan() {
    let machine = summary_machine();
    assert_eq!(summary_task_tally(&summary_plan(&["completed"]), &machine), "1 task completed");
    assert_eq!(summary_task_tally(&summary_plan(&[]), &machine), "no tasks");
}

#[test]
fn a_visit_is_printed_only_where_a_task_has_more_than_one_record() {
    // §FS-rhei-summary.2.2: a repeated supervisor visit is distinguishable and
    // a one-shot step stays clean.
    let inspection = CostInspection {
        summary: None,
        invocations: vec![
            (PathBuf::from("a.json"), summary_record("1", 1, "2026-05-20T10:00:00Z", "2026-05-20T10:02:32Z")),
            (PathBuf::from("b.json"), summary_record("2", 1, "2026-05-20T10:03:00Z", "2026-05-20T10:21:04Z")),
            (PathBuf::from("c.json"), summary_record("1", 2, "2026-05-20T10:22:00Z", "2026-05-20T10:22:45Z")),
        ],
        errors: Vec::new(),
    };
    let steps = summary_steps(&inspection);
    assert!(steps.contains("1. `1` work (visit 1) —"), "got:\n{steps}");
    assert!(steps.contains("2. `2` work — "), "got:\n{steps}");
    assert!(steps.contains("3. `1` work (visit 2) —"), "got:\n{steps}");
}

#[test]
fn a_step_duration_is_omitted_when_a_timestamp_will_not_parse() {
    // §FS-rhei-summary.2.2: an unparseable timestamp yields no duration rather
    // than a guessed one.
    let good = summary_record("1", 1, "2026-05-20T10:00:00Z", "2026-05-20T10:02:32Z");
    assert_eq!(summary_step_duration(&good).as_deref(), Some("2m32s"));
    let bad = summary_record("1", 1, "not-a-timestamp", "2026-05-20T10:02:32Z");
    assert_eq!(summary_step_duration(&bad), None);
    let backwards = summary_record("1", 1, "2026-05-20T10:02:32Z", "2026-05-20T10:00:00Z");
    assert_eq!(summary_step_duration(&backwards), None);
}

#[test]
fn an_unmeasured_record_contributes_no_token_clause() {
    // §FS-rhei-summary.4: an unmeasured record contributes no token line.
    let record = summary_record("1", 1, "2026-05-20T10:00:00Z", "2026-05-20T10:02:32Z");
    assert_eq!(summary_step_tokens(&record), None);

    let mut measured = record;
    measured.tokens.input.total = AccountingTokenDimension::measured(41_200);
    measured.tokens.output.total = AccountingTokenDimension::measured(3_800);
    assert_eq!(summary_step_tokens(&measured).as_deref(), Some("41.2k in / 3.8k out"));
}

#[test]
fn an_empty_accounting_store_still_renders_a_lead_line_and_the_unmeasured_line() {
    // §FS-rhei-summary.5: a freshly instantiated workspace is summarizable.
    let inspection = CostInspection { summary: None, invocations: Vec::new(), errors: Vec::new() };
    let rendered =
        render_summary(&summary_plan(&["implement"]), &summary_machine(), &inspection, false);
    assert_eq!(
        rendered,
        "`supervised-ticket-fix` workflow: 0 agent invocations across 0 models; \
         1 task in progress.\n\nToken accounting was not measured for this run.\n"
    );
}
