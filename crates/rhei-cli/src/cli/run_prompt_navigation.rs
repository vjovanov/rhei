// The two sub-sections `## Rhei Commands` gains: the map that makes every
// finished task in the project reachable from any other, and the note that says
// what a trail left behind is worth.
//
// Its own part because these render paths and fixed prose rather than memory:
// nothing here reads a result, a ledger, or the plan tree.

// §AR-source-file-size.3 §FS-rhei-memory.3.4

/// Where the artifacts are under any execution root, and which commands are
/// safe to run while looking for them.
// §FS-rhei-memory.3.4
const READING_THE_RHEI_TAIL: &str = "\
- Under each execution root: `runtime/results/<task-id>.md` (results),
  `runtime/exports/<task-id>/<name>.md` (exports), `runtime/supervise/<task-id>[/<state>].md` (briefs),
  `runtime/state-transitions.log` (order of events), `runtime/logs/` (agent transcripts)
- Read-only commands, always safe: `rhei list [--rhei <id>] [--terminal] [--has-prior <id>] [--parent <id>]`,
  `rhei render <plan> --format json --pretty`
";

/// What the next agent and the human will see, and what this one may edit.
///
/// It describes artifacts and permitted edits and says nothing about when to
/// stop: completion stays with the completion condition.
// §FS-rhei-memory.3.4 §FS-rhei-agents.3.1
const LEAVING_A_TRAIL: &str = "\
### Leaving a trail

What you write is what the next agent and the human see.
- `runtime/results/<task-id>.md`: the first line is the one-line summary every later Plan History shows; detail below it.
- You may append progress paragraphs to your own task body \u{2014} files touched, commands run, decisions made \u{2014} and append child tasks under your own task. Do not edit `**State:**` lines or any other task's body.
";

/// `### Reading the rhei` — the map that makes §FS-rhei-memory.1.1 true across
/// rheis: every execution root in the project, named, so the results of a rhei
/// the prompt does not list are one path away.
// §FS-rhei-memory.1.1 §FS-rhei-memory.3.4
fn render_reading_the_rhei(render_context: &RuntimeTemplateContext<'_>) -> String {
    let Some(memory) = render_context.memory else { return String::new() };
    let task_id = render_context.task.id.to_string();
    let rhei_id = owning_rhei_id(render_context);
    let root = rhei_id
        .as_deref()
        .and_then(|id| memory.rhei_roots.get(id))
        .map(PathBuf::as_path)
        .unwrap_or(render_context.workspace_root);
    let plan = rhei_id
        .as_deref()
        .and_then(|id| memory.rhei_plans.get(id))
        .map(PathBuf::as_path)
        .unwrap_or(render_context.plan_path);
    let task_file = memory
        .task_sources
        .get(&task_id)
        .map(PathBuf::as_path)
        .unwrap_or(plan);
    let mut out = format!(
        "\n### Reading the rhei\n\n\
         - This rhei: `{}` \u{2014} plan `{}`, this task's file `{}`\n\
         - Every rhei in this project and its execution root:\n",
        memory_path(render_context, root),
        memory_path(render_context, plan),
        memory_path(render_context, task_file),
    );
    for id in &memory.rhei_ids {
        let Some(other) = memory.rhei_roots.get(id) else { continue };
        out.push_str(&format!("  - `{id}` \u{2014} `{}`\n", memory_path(render_context, other)));
    }
    out.push_str(READING_THE_RHEI_TAIL);
    out
}

/// Both sub-sections, in the order §FS-rhei-memory.3.4 states, appended after
/// the existing authority text and transition list.
// §FS-rhei-memory.3.4
fn render_rhei_navigation(render_context: &RuntimeTemplateContext<'_>) -> String {
    let reading = render_reading_the_rhei(render_context);
    if reading.is_empty() {
        return String::new();
    }
    format!("{reading}\n{LEAVING_A_TRAIL}")
}
