// The argument group for `rhei new`: one flag per field the plan language lets
// an author write, kept beside the command that consumes them rather than in
// the shared declaration file.

// §FS-rhei-new.1

/// Every field `rhei new` can author, in one flattened argument group.
///
/// The set is deliberately exhaustive: each metadata field the plan language
/// lets an author write on a new ticket has a flag here, so creating a ticket
/// never has to be finished by hand in an editor.
// §FS-rhei-new.1
#[derive(Args, Debug)]
struct NewOptions {
    /// Title of the rhei or ticket
    #[arg(value_name = "TITLE")]
    title: String,
    /// Owning rhei id (`auth`, `basin`) for a top-level ticket, or ticket id
    /// (`auth.3`) for a subtask. Omitted, a rhei is created under Panta
    #[arg(long, value_name = "PARENT", add = ArgValueCompleter::new(complete_new_parent))]
    under: Option<String>,
    /// Project or plan to write into; omitted, the enclosing project,
    /// workspace, or lone plan is used
    #[arg(
        long,
        value_name = "RHEI_PLAN",
        add = ArgValueCompleter::new(complete_rhei_plan_path)
    )]
    project: Option<PathBuf>,
    /// Explicit id, replacing the one derived from the title
    #[arg(long, value_name = "ID")]
    id: Option<String>,
    /// Body content: the ticket's description, or the rhei's lead paragraph
    #[arg(long, value_name = "TEXT")]
    description: Option<String>,
    /// Read the description from a file; `-` reads standard input
    #[arg(
        long,
        value_name = "PATH",
        conflicts_with = "description",
        add = ArgValueCompleter::new(complete_any_path)
    )]
    description_file: Option<PathBuf>,
    /// Create a Directory Workspace rhei instead of a single file
    #[arg(long)]
    dir: bool,
    /// Bind the new rhei to a state machine by name
    #[arg(long, value_name = "NAME")]
    states: Option<String>,
    /// Write `structure.maxLevels` for the new rhei
    #[arg(long, value_name = "N")]
    max_levels: Option<u8>,
    /// Write `structure.nodeKinds` for the new rhei (repeatable;
    /// comma-separated list also accepted)
    #[arg(long, value_name = "KIND", value_delimiter = ',')]
    node_kinds: Vec<String>,
    /// Heading keyword for the new ticket, checked against structure.nodeKinds
    #[arg(long, value_name = "KIND", add = ArgValueCompleter::new(complete_new_node_kind))]
    kind: Option<String>,
    /// Starting state; defaults to the owning rhei machine's initial state
    #[arg(long, value_name = "STATE", add = ArgValueCompleter::new(complete_state_name))]
    state: Option<String>,
    /// Prior dependency (repeatable; comma-separated list also accepted)
    #[arg(
        long,
        value_name = "ID",
        value_delimiter = ',',
        add = ArgValueCompleter::new(complete_task_id)
    )]
    prior: Vec<String>,
    /// Export this ticket publishes (repeatable; comma-separated also accepted)
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    provides: Vec<String>,
    /// Export this ticket reads, as `<task-id>:<name>` (repeatable;
    /// comma-separated list also accepted)
    #[arg(long, value_name = "ID:NAME", value_delimiter = ',')]
    consumes: Vec<String>,
    /// Assignee for the new ticket
    #[arg(long, value_name = "WHO")]
    assignee: Option<String>,
    /// Per-ticket model override
    #[arg(long, value_name = "MODEL")]
    model: Option<String>,
    /// Per-ticket execution-identity override
    #[arg(long, value_name = "TARGET", conflicts_with = "model")]
    target: Option<String>,
    /// Print the target path and the markdown that would be written
    #[arg(long)]
    dry_run: bool,
    /// Emit the created id, kind, path, and state as JSON
    #[arg(long)]
    json: bool,
    /// Keep the write when validation fails, instead of rolling it back
    #[arg(long)]
    keep_on_error: bool,
}
