// What the rendered command-line surface promises about an option: that
// `--help` names it, and that what it says there is what the option does.
// §FS-rhei-transition-cmd.2

/// `--supervisor` is a documented option, not a hidden one: rhei's own
/// supervisor prompt instructs an agent to type it
/// (§FS-rhei-supervision.5.1), so a help listing that denies it leaves the
/// agent working out which of the two is wrong before it dares run the
/// command. §FS-rhei-transition-cmd.2
#[test]
fn transition_help_documents_the_supervisor_option() {
    let mut command = cli_command();
    let transition = command.find_subcommand_mut("transition").expect("`transition` subcommand");

    let help = transition.render_help().to_string();
    assert!(
        help.contains("--supervisor"),
        "`rhei transition --help` should list `--supervisor`:\n{help}"
    );

    // clap renders a doc comment's *first paragraph* as the option's short
    // help, so a blank line in it would leave the listing naming the flag
    // without saying what it does — half the answer.
    let description = transition
        .get_arguments()
        .find(|arg| arg.get_id() == "supervisor")
        .expect("`transition` declares `--supervisor`")
        .get_help()
        .expect("`--supervisor` carries help")
        .to_string();
    assert!(
        description.contains("checkpoint"),
        "`--supervisor` should be described by what it does — suppress the \
         checkpoint this move would deliver to the named supervisor — got: {description}"
    );
}
