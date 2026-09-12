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
