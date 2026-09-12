    // What a template that will not render is told — a responsibility apart
    // from rendering one that will, and a seam §AR-source-file-size.3 names.

    // Every message here is anchored on what the file contains rather than on
    // where the parser gave up. §FS-rhei-templates.5.3

    /// What the parser itself said, without the `(in <string>:N)` suffix its
    /// `Display` appends: that position is where parsing stopped, which is
    /// exactly what §FS-rhei-templates.5.3 says not to report as the mistake.
    fn minijinja_words(err: &minijinja::Error) -> String {
        match err.detail() {
            Some(detail) => format!("{}: {detail}", err.kind()),
            None => err.kind().to_string(),
        }
    }

    /// A parse failure, anchored on the opener that was never closed rather
    /// than on the token the parser choked on. §FS-rhei-templates.5.3
    fn template_parse_report(path: &Path, raw: &str, err: &minijinja::Error) -> Report {
        match scan_template_text(raw).unclosed {
            Some(open) => miette!(
                help = format!(
                    "close it with `{}`, or, if the braces are meant to appear in the output, \
                     wrap them in {{% raw %}} ... {{% endraw %}}.",
                    open.closing
                ),
                "failed to parse template '{}:{}': this `{}` is never closed by `{}` ({})",
                path.display(),
                open.line,
                open.opener,
                open.closing,
                minijinja_words(err)
            ),
            // Nothing is left open, so the parser's own position is the most
            // specific thing there is, and its words stand unedited.
            None => miette!(
                help = "correct the instantiation expression the parser stopped at, or, if the \
                        braces are meant to appear in the output, wrap them in {% raw %} ... \
                        {% endraw %}.",
                "failed to parse template '{}:{}': {}",
                path.display(),
                err.line().unwrap_or(1),
                minijinja_words(err)
            ),
        }
    }

    /// The text of the expression that failed to render, restored to the way
    /// the author wrote it. MiniJinja's span indexes the preprocessed text,
    /// which is byte-for-byte as long as the source, so it indexes the source
    /// too. §FS-rhei-templates.5.3
    fn failing_expression(preprocessed: &str, err: &minijinja::Error, line: usize) -> String {
        if let Some(text) = err.range().and_then(|range| preprocessed.get(range)) {
            let text = restore_hidden_openers(text.trim());
            if !text.is_empty() {
                return text;
            }
        }
        // No span: the line is the most specific thing left.
        restore_hidden_openers(
            preprocessed.lines().nth(line.saturating_sub(1)).unwrap_or_default().trim(),
        )
    }

    /// The input an expression names first, which is the one the manifest has
    /// to declare. §FS-rhei-templates.5.3
    fn undeclared_input_name(expression: &str) -> Option<String> {
        let name: String =
            expression.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        (!name.is_empty()).then_some(name)
    }

    /// A render failure, naming the expression and its line rather than the
    /// manifest. §FS-rhei-templates.5.3
    fn template_render_report(
        path: &Path,
        preprocessed: &str,
        template_ref: &str,
        err: &minijinja::Error,
    ) -> Report {
        let line = err.line().unwrap_or(1);
        let expression = failing_expression(preprocessed, err, line);
        let undeclared = (err.kind() == minijinja::ErrorKind::UndefinedError)
            .then(|| undeclared_input_name(&expression))
            .flatten();
        let remedy = "To emit the braces literally instead, wrap them in {% raw %} ... \
                      {% endraw %}. List the inputs this template declares with: rhei \
                      instantiate";
        let help = match undeclared {
            Some(name) => format!(
                "`{name}` is not an input this template declares. {remedy} {template_ref} \
                 --list-inputs"
            ),
            None => format!("Correct the expression. {remedy} {template_ref} --list-inputs"),
        };
        miette!(
            help = help,
            "failed to render template '{}:{}': {} in `{}`",
            path.display(),
            line,
            minijinja_words(err),
            expression
        )
    }

    /// Say once, per file, that text earlier versions of rhei cut is now
    /// emitted. Not a failure: instantiation proceeds and the exit code is
    /// unchanged, so `--dry-run` is how a template author asks which of their
    /// files moved. §FS-rhei-templates.5.3
    fn warn_about_hidden_comment_opener(raw: &str, path: &Path) {
        if !raw.contains("{#") {
            return;
        }
        let Some(line) = scan_template_text(raw).first_comment_opener else {
            return;
        };
        eprintln!(
            "warning: {}:{} contains `{{#`. Earlier versions of rhei read it as an opener and \
             either cut the text through to the next `#}}` or refused the file outright; it is \
             now emitted verbatim.",
            path.display(),
            line
        );
    }
