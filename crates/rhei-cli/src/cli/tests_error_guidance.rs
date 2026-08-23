// Unit tests for the shared error-guidance helpers. §FS-rhei-errors
// These decide what a user pastes, so edge cases are tested directly.

mod error_guidance_tests {
    use super::super::*;

    /// Quoting a word so a shell hands it back unchanged.
    ///
    /// Which quote a platform uses is `rhei_core::platform`'s question and is
    /// pinned there, both halves, on every platform. What is asked here is the
    /// decision this CLI depends on: bare when the shell would not touch it,
    /// quoted when it would.

    // §FS-rhei-errors.2
    mod shell_quote {
        use super::*;

        #[test]
        fn leaves_ordinary_words_bare() {
            for value in ["rhei", "instantiate", "--list-inputs", "a/b/c.md", "1.2.3", "a,b"] {
                assert_eq!(shell_quote(value), value, "{value} should not need quoting");
            }
        }

        #[test]
        fn quotes_what_a_shell_would_otherwise_read() {
            // The motivating case is the selector: unquoted, zsh fails with
            // `no matches found` before rhei is executed at all.
            for value in
                ["", "codex[yolo]:openai:gpt-5.5", "*.md", "a b", "~/plans", "it's", "one\ntwo"]
            {
                let quoted = shell_quote(value);
                assert_ne!(quoted, value, "{value:?} must not be printed bare");
                assert!(quoted.len() > value.len(), "{value:?} came back as {quoted:?}");
            }
        }

        #[test]
        fn quotes_a_leading_equals_where_the_shell_expands_it() {
            // zsh EQUALS expansion turns a leading `=word` into the path of
            // `word`, even though `=` is harmless elsewhere in a word. `cmd`
            // has no such expansion, so there the word stands as written.
            assert_eq!(shell_quote("=less"), if cfg!(windows) { "=less" } else { "'=less'" });
            assert_eq!(shell_quote("a=b"), "a=b");
        }
    }

    /// Rendering one argument, keeping a `KEY=` readable. §FS-rhei-errors.2
    mod shell_arg {
        use super::*;

        #[test]
        fn quotes_only_the_value_of_an_assignment() {
            // Built with the quoting the product would use, because what this
            // function decides is *where* the quotes go, not which they are.
            assert_eq!(
                shell_arg("agent=codex[yolo]:openai:gpt-5.5"),
                format!("agent={}", shell_quote("codex[yolo]:openai:gpt-5.5"))
            );
            assert_eq!(shell_arg("subject=<value>"), format!("subject={}", shell_quote("<value>")));
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
            assert_eq!(shell_arg("=x"), shell_quote("=x"));
        }

        #[test]
        fn round_trips_through_a_shell_unchanged() {
            // What the shell hands back is `key` + the unquoted value, which is
            // the original argument.
            let original = "agent=codex[yolo]:openai:gpt-5.5";
            let rendered = shell_arg(original);
            let quote = if cfg!(windows) { '"' } else { '\'' };
            let unquoted = rendered.replace(quote, "");
            assert_eq!(unquoted, original);
        }
    }

    #[test]
    fn shell_command_joins_quoted_parts() {
        assert_eq!(
            shell_command(["rhei", "instantiate", "guided", "agent=a[b]:c:d"]),
            format!("rhei instantiate guided agent={}", shell_quote("a[b]:c:d"))
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

    /// Every diagnostic carries a next action, guarded here rather than left
    /// to review because coverage is a property of the whole crate rather than
    /// of any one call site. §FS-rhei-errors.6
    #[test]
    fn every_miette_site_carries_help() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut uncovered = Vec::new();
        let mut total = 0usize;
        let mut pending = vec![src];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(&dir).expect("read source dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                // Test fixtures construct reports to assert on, not to show.
                if !name.ends_with(".rs") || name.starts_with("tests_") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).expect("read source file");
                let lines: Vec<&str> = text.lines().collect();
                for (index, line) in lines.iter().enumerate() {
                    for _ in line.match_indices("miette!(") {
                        total += 1;
                        let window = lines[index..lines.len().min(index + 3)].join("\n");
                        if !window.contains("help =") {
                            uncovered.push(format!("{}:{}", name, index + 1));
                        }
                    }
                }
            }
        }
        assert!(total > 300, "the scan found only {total} sites; has the pattern changed?");
        assert!(
            uncovered.is_empty(),
            "{} of {total} miette! sites carry no help:\n  {}",
            uncovered.len(),
            uncovered.join("\n  ")
        );
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
            assert!(help.contains(&shell_quote("/tmp/a b/plan.md")), "got: {help}");
        }
    }
}
