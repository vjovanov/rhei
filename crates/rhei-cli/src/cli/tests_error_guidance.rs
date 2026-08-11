// Unit tests for the shared error-guidance helpers. §FS-rhei-errors
// These decide what a user pastes, so edge cases are tested directly.

mod error_guidance_tests {
    use super::super::*;

    /// Quoting a word so a shell hands it back unchanged. §FS-rhei-errors.2
    mod shell_quote {
        use super::*;

        #[test]
        fn leaves_ordinary_words_bare() {
            for value in ["rhei", "instantiate", "--list-inputs", "a/b/c.md", "1.2.3", "a,b"] {
                assert_eq!(shell_quote(value), value, "{value} should not need quoting");
            }
        }

        #[test]
        fn quotes_the_empty_string_so_it_survives_as_an_argument() {
            assert_eq!(shell_quote(""), "''");
        }

        #[test]
        fn quotes_glob_characters() {
            // The motivating case: unquoted, zsh fails with `no matches found`
            // before rhei is executed at all.
            assert_eq!(
                shell_quote("codex[yolo]:openai:gpt-5.5"),
                "'codex[yolo]:openai:gpt-5.5'"
            );
            assert_eq!(shell_quote("*.md"), "'*.md'");
            assert_eq!(shell_quote("a b"), "'a b'");
            assert_eq!(shell_quote("~/plans"), "'~/plans'");
        }

        #[test]
        fn quotes_a_leading_equals_because_zsh_expands_it() {
            // zsh EQUALS expansion turns a leading `=word` into the path of
            // `word`, even though `=` is harmless elsewhere in a word.
            assert_eq!(shell_quote("=less"), "'=less'");
            assert_eq!(shell_quote("a=b"), "a=b");
        }

        #[test]
        fn escapes_embedded_single_quotes() {
            assert_eq!(shell_quote("it's"), r#"'it'"'"'s'"#);
        }

        #[test]
        fn quotes_a_newline() {
            assert_eq!(shell_quote("one\ntwo"), "'one\ntwo'");
        }
    }

    /// Rendering one argument, keeping a `KEY=` readable. §FS-rhei-errors.2
    mod shell_arg {
        use super::*;

        #[test]
        fn quotes_only_the_value_of_an_assignment() {
            assert_eq!(
                shell_arg("agent=codex[yolo]:openai:gpt-5.5"),
                "agent='codex[yolo]:openai:gpt-5.5'"
            );
            assert_eq!(shell_arg("subject=<value>"), "subject='<value>'");
        }

        #[test]
        fn leaves_an_assignment_that_needs_no_quoting_alone() {
            assert_eq!(shell_arg("subject=docs"), "subject=docs");
        }

        #[test]
        fn quotes_a_flag_whole() {
            assert_eq!(shell_arg("--set"), "--set");
            assert_eq!(shell_arg("--output"), "--output");
        }

        #[test]
        fn treats_a_non_identifier_left_hand_side_as_a_plain_word() {
            // A path that happens to contain `=` is not an assignment.
            assert_eq!(shell_arg("/tmp/a=b/c"), "/tmp/a=b/c");
            assert_eq!(shell_arg("=x"), "'=x'");
        }

        #[test]
        fn round_trips_through_a_shell_unchanged() {
            // What the shell hands back is `key` + the unquoted value, which is
            // the original argument.
            let original = "agent=codex[yolo]:openai:gpt-5.5";
            let rendered = shell_arg(original);
            let unquoted = rendered.replace('\'', "");
            assert_eq!(unquoted, original);
        }
    }

    #[test]
    fn shell_command_joins_quoted_parts() {
        assert_eq!(
            shell_command(["rhei", "instantiate", "guided", "agent=a[b]:c:d"]),
            "rhei instantiate guided agent='a[b]:c:d'"
        );
    }

    /// Suggesting the name the user probably meant. §FS-rhei-errors.1.3
    mod near_misses {
        use super::*;

        fn names(values: &[&str]) -> Vec<String> {
            values.iter().map(|value| value.to_string()).collect()
        }

        #[test]
        fn finds_a_single_character_typo() {
            let known = names(&["subject", "analysis_brief", "plan_title"]);
            assert_eq!(did_you_mean("subjekt", &known).as_deref(), Some("Did you mean 'subject'?"));
        }

        #[test]
        fn ignores_case() {
            let known = names(&["claude-code", "codex"]);
            assert_eq!(
                did_you_mean("Claude-Code", &known).as_deref(),
                Some("Did you mean 'claude-code'?")
            );
        }

        #[test]
        fn lists_the_candidates_when_nothing_is_close() {
            let known = names(&["subject", "brief"]);
            assert_eq!(
                did_you_mean("wildly-different-name", &known).as_deref(),
                Some("Valid values: subject, brief.")
            );
        }

        #[test]
        fn truncates_a_long_candidate_list() {
            let known = (0..20).map(|i| format!("input_number_{i}")).collect::<Vec<_>>();
            let hint = did_you_mean("zzzzzzzzzzzzzzzzzzzzzzz", &known).expect("a hint");
            assert!(hint.starts_with("Valid values include: input_number_0, "), "got: {hint}");
            assert!(hint.ends_with("(and 12 more)."), "got: {hint}");
        }

        #[test]
        fn drops_duplicates_so_a_merged_registry_lists_each_id_once() {
            // Built-ins are seeded into the settings registry, so the same id
            // can reach this twice; it must still be offered once.
            let known = names(&["claude-code", "codex", "claude-code", "codex"]);
            assert_eq!(
                did_you_mean("nothing-alike-at-all", &known).as_deref(),
                Some("Valid values: claude-code, codex.")
            );
        }

        #[test]
        fn declines_when_there_are_no_candidates() {
            assert_eq!(did_you_mean("anything", &[]), None);
        }

        #[test]
        fn declines_on_empty_input() {
            let known = names(&["subject"]);
            assert_eq!(nearest_match("", &known), None);
            assert_eq!(nearest_match("   ", &known), None);
        }

        #[test]
        fn breaks_ties_by_name_so_suggestions_are_stable() {
            let known = names(&["beta", "alpha"]);
            // Both are distance 5 from a 5-character input; the lower name wins.
            assert_eq!(nearest_match("gamma", &known).as_deref(), None);
            let known = names(&["bx", "ax"]);
            assert_eq!(nearest_match("cx", &known).as_deref(), Some("ax"));
        }
    }

    #[test]
    fn levenshtein_distance_counts_single_edits() {
        assert_eq!(levenshtein_distance("", ""), 0);
        assert_eq!(levenshtein_distance("abc", "abc"), 0);
        assert_eq!(levenshtein_distance("", "abc"), 3);
        assert_eq!(levenshtein_distance("abc", ""), 3);
        assert_eq!(levenshtein_distance("abc", "abd"), 1);
        assert_eq!(levenshtein_distance("abc", "ab"), 1);
        assert_eq!(levenshtein_distance("ab", "abc"), 1);
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
    }

    #[test]
    fn levenshtein_distance_counts_characters_not_bytes() {
        // A multi-byte character is one edit, not one per byte.
        assert_eq!(levenshtein_distance("café", "cafe"), 1);
        assert_eq!(levenshtein_distance("naïve", ""), 5);
    }

    /// A `--agent` value that is really a selector. §FS-rhei-errors.1.2
    mod agent_flag_selector {
        use super::*;

        #[test]
        fn declines_for_a_bare_id() {
            assert_eq!(agent_flag_selector_help("claude-code", &[]), None);
        }

        #[test]
        fn rewrites_a_selector_into_the_flags_that_carry_it() {
            let help = agent_flag_selector_help("my-agent[yolo]:some-model", &[])
                .expect("a selector should be recognized");
            assert!(
                help.contains("--agent my-agent --agent-mode yolo --model some-model"),
                "got: {help}"
            );
        }

        #[test]
        fn handles_a_selector_without_a_mode() {
            let help =
                agent_flag_selector_help("my-agent:some-model", &[]).expect("a selector");
            assert!(help.contains("--agent my-agent --model some-model"), "got: {help}");
            assert!(!help.contains("--agent-mode"), "got: {help}");
        }

        #[test]
        fn also_names_a_typo_inside_the_selector() {
            let known = vec!["claude-code".to_string()];
            let help = agent_flag_selector_help("claude-codee:m", &known).expect("a selector");
            assert!(help.contains("--agent claude-codee --model m"), "got: {help}");
            assert!(help.contains("Did you mean 'claude-code'?"), "got: {help}");
        }

        #[test]
        fn stays_silent_about_an_id_that_is_defined() {
            let known = vec!["my-agent".to_string()];
            let help = agent_flag_selector_help("my-agent:m", &known).expect("a selector");
            assert!(!help.contains("Did you mean"), "got: {help}");
        }

        #[test]
        fn declines_when_the_value_is_not_a_parseable_selector() {
            assert_eq!(agent_flag_selector_help("a:b:c:d:e", &[]), None);
        }
    }

    #[test]
    fn unknown_agent_help_does_not_repeat_a_merged_registry() {
        let known = vec!["claude-code".to_string(), "codex".to_string(), "codex".to_string()];
        let help = unknown_agent_help("totally-unrelated", &known);
        assert_eq!(help.matches("codex").count(), 1, "got: {help}");
        assert!(help.contains("agents.<id>"), "got: {help}");
    }

    /// Filesystem remedies derived from the error kind. §FS-rhei-errors.6
    mod io_help {
        use super::*;
        use std::io::ErrorKind;

        #[test]
        fn a_missing_file_in_an_existing_directory_is_a_spelling_check() {
            // Tests run with the crate root as the working directory, so `src`
            // is a directory that really exists.
            let help = io_error_help(&PathBuf::from("src/absent.md"), ErrorKind::NotFound);
            assert!(help.contains("Check the spelling"), "got: {help}");
            assert!(help.contains("ls src"), "got: {help}");
        }

        #[test]
        fn a_missing_directory_is_an_mkdir() {
            let help =
                io_error_help(&PathBuf::from("no/such/dir/absent.md"), ErrorKind::NotFound);
            assert!(help.contains("mkdir -p"), "got: {help}");
        }

        #[test]
        fn a_bare_filename_has_no_directory_to_suggest() {
            let help = io_error_help(&PathBuf::from("absent.md"), ErrorKind::NotFound);
            assert!(help.contains("Check the spelling"), "got: {help}");
            assert!(!help.contains("ls "), "got: {help}");
        }

        #[test]
        fn permission_denied_points_at_the_path_itself() {
            let help = io_error_help(&PathBuf::from("/etc/shadow"), ErrorKind::PermissionDenied);
            assert!(help.contains("ls -ld /etc/shadow"), "got: {help}");
        }

        #[test]
        fn a_path_needing_quotes_is_quoted() {
            let help =
                io_error_help(&PathBuf::from("/tmp/a b/plan.md"), ErrorKind::PermissionDenied);
            assert!(help.contains("'/tmp/a b/plan.md'"), "got: {help}");
        }
    }
}
