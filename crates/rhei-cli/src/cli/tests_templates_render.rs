// Unit tests for the instantiation renderer's two halves: the substitution
// that hides what is not syntax, and the scan that decides what a malformed
// template is told. §FS-rhei-templates.5 §FS-rhei-templates.5.3

// The end-to-end tests pin what a user sees. These pin the two properties a
// passing end-to-end run would not notice going wrong: that the substitution
// changes no byte offset, and that the scan blames the innermost opener while
// leaving `{% raw %}` regions alone.

mod templates_render_tests {
    use super::super::*;

    /// §FS-rhei-templates.5.3: MiniJinja reports offsets into the text it was
    /// given, and those offsets are only usable against the author's source
    /// while the substitution is length-preserving. A sentinel of the wrong
    /// width reintroduces exactly the drift it exists to remove.
    mod substitution {
        use super::*;

        #[test]
        fn preserves_byte_length() {
            for source in [
                "echo \"${#IR[@]}\"\n",
                "esc: \\{{ literal }}\n",
                "both: \\{{ x }} and ${#A[@]}\n",
                "none of it\n",
                // Multi-byte text either side of a substitution.
                "héllo ${#A[@]} wörld\n",
            ] {
                assert_eq!(
                    hide_non_syntax_openers(source).len(),
                    source.len(),
                    "the substitution shifted an offset in {source:?}"
                );
            }
        }

        #[test]
        fn round_trips_exactly() {
            for source in [
                "echo \"${#IR[@]}\"\n",
                "raw: {% raw %}{# x #}{% endraw %}\n",
                "esc: \\{{ literal }}\n",
                "{{ kept }} and {% if kept %}{% endif %}\n",
                "héllo ${#A[@]} wörld\n",
            ] {
                let hidden = hide_non_syntax_openers(source);
                assert_eq!(
                    restore_hidden_openers(&hidden),
                    // `\{{` restores as the literal `{{` it stands for, which
                    // is the escape's whole contract. §FS-rhei-templates.5.2
                    source.replace(r"\{{", "{{"),
                    "the round trip did not restore {source:?}"
                );
            }
        }

        /// The lexer's own precedence: in `{{#` the `{{` wins, so the `{#` one
        /// byte later is not an opener and must not be hidden.
        #[test]
        fn leaves_an_interpolation_that_starts_with_a_hash() {
            let hidden = hide_non_syntax_openers("{{ '#' }} and {#\n");
            assert!(hidden.starts_with("{{ '#' }}"), "an interpolation was rewritten: {hidden:?}");
            assert!(hidden.ends_with(" and \u{91}\n"), "the text-level `{{#` survived: {hidden:?}");
        }

        /// A real opener is stepped over whole, so a `{#` inside one is part of
        /// an expression rather than text and reaches the parser as written.
        #[test]
        fn leaves_a_hash_brace_inside_a_construct_alone() {
            for source in [
                "{% if marker == \"{#\" %}YES{% endif %}\n",
                "{{ \"pre{#post\" }}\n",
                "{% if a %}{{ \"{#\" }}{% endif %}\n",
            ] {
                assert_eq!(hide_non_syntax_openers(source), source, "a construct was rewritten");
            }
            // ... and a text-level one after a construct is still hidden.
            assert_eq!(hide_non_syntax_openers("{{ a }} and {#\n"), "{{ a }} and \u{91}\n");
        }

        /// Everything after an opener whose delimiter never closes is inside
        /// that opener, so none of it is text and none of it is rewritten.
        #[test]
        fn leaves_everything_after_an_opener_that_never_closes() {
            for source in ["{{ foo and {# more\n", "{% if foo and {# more\n"] {
                assert_eq!(hide_non_syntax_openers(source), source, "text inside an opener moved");
            }
        }

        /// A `{{` inside a `{% raw %}` region is literal text and opens
        /// nothing. Reading it as an opener pairs it with the `}}` of a real
        /// interpolation further down, and every `{#` between the two goes
        /// unhidden — which is the ticket's own failure, for a file that only
        /// documents what `{{` means. §FS-rhei-templates.5.2
        #[test]
        fn leaves_a_raw_regions_literal_braces_unpaired() {
            let source = "{% raw %}\nUse {{ to open.\n{% endraw %}\n\
                          A=(x y); echo \"${#A[@]}\"\ntitle: {{ title }}\n";
            let hidden = hide_non_syntax_openers(source);
            assert!(hidden.contains("Use {{ to open."), "the raw region moved: {hidden:?}");
            assert!(hidden.contains("$\u{91}A[@]}"), "the `{{#` after it stayed: {hidden:?}");
            assert!(hidden.contains("title: {{ title }}"), "the interpolation moved: {hidden:?}");
        }

        /// The other branch: a raw-fenced `{{` with no `}}` anywhere after it
        /// used to stop the pass dead, so the `\{{` escape past the region
        /// stopped being honoured. §FS-rhei-templates.5.2
        #[test]
        fn honours_what_follows_a_raw_fenced_brace_that_never_pairs() {
            let source = "{% raw %}\nliteral {{ open\n{% endraw %}\nesc: \\{{ notvar }}\n";
            let hidden = hide_non_syntax_openers(source);
            assert!(hidden.contains("\u{e001} notvar }}"), "the escape was left: {hidden:?}");

            let bash = "{% raw %}\nliteral {{ open\n{% endraw %}\necho \"${#A[@]}\"\n";
            assert!(
                hide_non_syntax_openers(bash).contains("$\u{91}A[@]}"),
                "a bundled array length after the region stayed in the parser's way"
            );
        }
    }

    /// §FS-rhei-templates.5.4: the round trip keeps a few code points for
    /// itself, so a template's own text may not carry one — and is told which
    /// file and which code point rather than being quietly rewritten.
    mod reserved_code_points {
        use super::*;

        #[test]
        fn refuses_a_template_carrying_one_by_name() {
            for (raw, named) in [
                ("head\n\u{91} tail\n", "U+0091"),
                ("head\n\u{e001} tail\n", "U+E001"),
                ("head\n\u{e002} tail\n", "U+E002"),
            ] {
                let refusal = reject_reserved_code_points(raw, Path::new("/t/carries.md"))
                    .expect_err("a reserved code point is refused")
                    .to_string();
                assert!(refusal.contains("/t/carries.md"), "the file is named: {refusal}");
                assert!(refusal.contains(named), "the code point is named: {refusal}");
            }
        }

        #[test]
        fn allows_the_text_this_ticket_is_about() {
            let script = "#!/usr/bin/env bash\nIR=(a b)\necho \"${#IR[@]}\"\n";
            assert!(reject_reserved_code_points(script, Path::new("/t/bad.sh")).is_ok());
        }
    }

    /// §FS-rhei-templates.5.1: an input value is resolved into the output as
    /// the value it is. The code points §FS-rhei-templates.5.4 reserves are
    /// reserved against a template's text, not against what is put into it.
    mod value_round_trip {
        use super::*;

        fn render(source: &str, value: &str) -> String {
            let values = BTreeMap::from([("title".to_string(), serde_json::Value::from(value))]);
            render_template_text(source, &values, Path::new("/t/notes.md"), "t")
                .expect("the template renders")
        }

        #[test]
        fn hands_back_a_value_that_carries_a_reserved_code_point() {
            for carried in ["C\u{91}D", "C\u{e001}D", "C\u{e002}D", "C\u{e002}\u{91}D"] {
                assert_eq!(
                    render("note: {{ title }}\n", carried),
                    format!("note: {carried}\n"),
                    "a value was rewritten on its way out"
                );
            }
        }

        /// The other half: what the preprocessor hid is still put back, and a
        /// value beside it is untouched.
        #[test]
        fn still_emits_a_text_level_hash_brace_verbatim() {
            assert_eq!(render("${#A[@]} {{ title }}\n", "x"), "${#A[@]} x\n");
        }

        /// §FS-rhei-templates.5: a `{#` inside an expression is the `{#` the
        /// author wrote, so it compares as one.
        #[test]
        fn compares_a_hash_brace_in_a_string_literal_as_written() {
            let source = "{% if title == \"{#\" %}YES{% else %}NO{% endif %}\n";
            assert_eq!(render(source, "{#"), "YES\n");
            assert_eq!(render(source, "other"), "NO\n");
        }

        /// §FS-rhei-templates.5: a `{% raw %}` region documenting what `{{`
        /// means does not stop a bundled `${#ARR[@]}` beside it from being
        /// emitted verbatim, and the interpolation past both still resolves.
        #[test]
        fn renders_a_bundled_array_length_beside_a_raw_region() {
            let source = "{% raw %}\nUse {{ to open.\n{% endraw %}\n\
                          A=(x y); echo \"${#A[@]}\"\ntitle: {{ title }}\n";
            assert_eq!(
                render(source, "hello"),
                "\nUse {{ to open.\n\nA=(x y); echo \"${#A[@]}\"\ntitle: hello\n"
            );
        }

        /// MiniJinja makes the strict check before it reaches a formatter, and
        /// makes it only for the undefined an expression named without
        /// declaring. An if-expression with no `else` yields the *silent*
        /// undefined the engine exempts, and renders as empty text.
        /// §FS-rhei-templates.5
        #[test]
        fn renders_an_if_expression_with_no_else_as_empty_text() {
            let values = BTreeMap::from([("flag".to_string(), serde_json::Value::from(false))]);
            let rendered =
                render_template_text("x: [{{ 1 if flag }}]\n", &values, Path::new("/t/n.md"), "t")
                    .expect("an if-expression with no false branch renders");
            assert_eq!(rendered, "x: []\n");
        }

        /// The other side of the same line: replacing the formatter does not
        /// lose the strict check, so an undeclared input is still an error
        /// naming itself. §FS-rhei-templates.5.3
        #[test]
        fn still_fails_on_an_input_the_manifest_does_not_declare() {
            let values = BTreeMap::new();
            let source = "run: {{ github.sha }}\n";
            let failure = render_template_text(source, &values, Path::new("/t/ci.yml"), "t")
                .expect_err("an undeclared input is a failure")
                .to_string();
            assert!(failure.contains("github"), "the expression is named: {failure}");
        }
    }

    /// §FS-rhei-templates.5.3: which opener a parse failure is anchored on.
    mod unclosed_opener {
        use super::*;

        fn unclosed(source: &str) -> Option<(&'static str, usize, String)> {
            scan_template_text(source)
                .unclosed
                .map(|open| (open.opener, open.line, open.closing))
        }

        #[test]
        fn finds_the_innermost_of_nested_openers() {
            let source = "{% for row in rows %}\n{% if row %}\n{% endfor %}\n";
            let (opener, line, closing) = unclosed(source).expect("a block is left open");
            assert_eq!((opener, line), ("{%", 2), "the `if` on line 2 is the innermost");
            assert_eq!(closing, "{% endif %}");
        }

        #[test]
        fn names_an_interpolation_on_its_own_line() {
            let source = "line one\nline two {{ foo\nline three\n";
            assert_eq!(unclosed(source), Some(("{{", 2, "}}".to_string())));
        }

        #[test]
        fn names_a_tag_whose_delimiter_never_closes() {
            let source = "line one\n{% if foo\nline three\n";
            assert_eq!(unclosed(source), Some(("{%", 2, "%}".to_string())));
        }

        /// A `{% raw %}` region is literal text, so an opener inside one is
        /// not a failure and is never blamed for being one.
        #[test]
        fn skips_raw_regions() {
            let source = "{% raw %}{{ unclosed and {% if x %}{# c #}{% endraw %}\n{{ ok }}\n";
            assert!(unclosed(source).is_none(), "a raw region was blamed: {source:?}");
        }

        #[test]
        fn blames_a_raw_block_that_never_ends() {
            let source = "text\n{% raw %}\n{{ literal }}\n";
            assert_eq!(unclosed(source), Some(("{%", 2, "{% endraw %}".to_string())));
        }

        /// A well-formed template leaves nothing open, so a parse failure in
        /// one takes the fallback path and quotes the parser's own position.
        #[test]
        fn finds_nothing_in_a_malformed_expression_that_is_closed() {
            assert!(unclosed("{{ foo bar }}\n").is_none());
            assert!(unclosed("{% if x %}{{ y }}{% endif %}\n").is_none());
            assert!(unclosed("plain text with a }} and a %}\n").is_none());
        }

        /// The escape stands for a literal `{{`, so it opens nothing.
        #[test]
        fn ignores_the_backslash_escape() {
            assert!(unclosed("esc: \\{{ not_an_opener\n").is_none());
        }
    }

    /// §FS-rhei-templates.5: which `{#` used to cut text, and so is warned
    /// about. The ones inside a raw region or inside an expression never were
    /// comments, so nothing about them has changed.
    mod comment_opener {
        use super::*;

        fn first_line(source: &str) -> Option<usize> {
            scan_template_text(source).first_comment_opener
        }

        #[test]
        fn names_the_line_of_the_first_text_level_occurrence() {
            let source = "#!/usr/bin/env bash\nA=(x y)\necho \"${#A[@]}\"\necho \"${#A[@]}\"\n";
            assert_eq!(first_line(source), Some(3));
        }

        #[test]
        fn ignores_one_inside_a_raw_region() {
            assert_eq!(first_line("raw: {% raw %}{# x #}{% endraw %}\n"), None);
        }

        #[test]
        fn ignores_one_inside_an_interpolation() {
            assert_eq!(first_line("{{ '{#' }}\n"), None);
        }

        #[test]
        fn finds_nothing_in_a_file_without_one() {
            assert_eq!(first_line("{{ title }}\n{% if x %}{% endif %}\n"), None);
        }
    }
}
