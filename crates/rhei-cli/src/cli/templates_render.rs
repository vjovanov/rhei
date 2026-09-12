    // The restricted instantiation environment and everything rhei says about a
    // template's text — a responsibility apart from reading what the user
    // passed, and a seam §AR-source-file-size.3 names.

    /// Stands in for a text-level `{#` for as long as MiniJinja is looking at
    /// the text. `{# ... #}` is not instantiation syntax
    /// (§FS-rhei-templates.5), and MiniJinja's default lexer has no way of
    /// being told so: the opener is hidden from it and put back verbatim once
    /// rendering is done.
    ///
    /// Both sentinels are single scalars of exactly the byte length of the
    /// text they replace, so every offset MiniJinja reports still indexes the
    /// source the author wrote and can be pointed at
    /// (§FS-rhei-templates.5.3). They are a C1 control and a private-use code
    /// point that no plan, script, or YAML file is authored with; a template
    /// carrying one anyway is refused by name rather than quietly mangled.
    const COMMENT_OPENER_SENTINEL: char = '\u{91}';

    /// Stands in for the `\{{` escape (§FS-rhei-templates.5.2) — three bytes
    /// for three, for the same reason.
    const ESCAPED_INTERPOLATION_SENTINEL: char = '\u{e001}';

    /// Marks a code point an **input value** carried rather than one the
    /// preprocessor put there, so the restore hands the value back its own
    /// bytes instead of rewriting them into `{#` or `{{`
    /// (§FS-rhei-templates.5.1). It is written into the output and taken out
    /// again, never into a template's source, so nothing constrains its width.
    const VALUE_SENTINEL_ESCAPE: char = '\u{e002}';

    /// The code points the round trip keeps for itself, which is why a
    /// template's own text may not carry one. §FS-rhei-templates.5.4
    const RESERVED_CODE_POINTS: [char; 3] =
        [COMMENT_OPENER_SENTINEL, ESCAPED_INTERPOLATION_SENTINEL, VALUE_SENTINEL_ESCAPE];

    // The whole point of both sentinels is that they change no offset.
    const _: () = assert!(COMMENT_OPENER_SENTINEL.len_utf8() == "{#".len());
    const _: () = assert!(ESCAPED_INTERPOLATION_SENTINEL.len_utf8() == r"\{{".len());

    /// Tags that open a block the template has to close with `{% end... %}`.
    /// Only `if`, `for`, and `raw` are reachable in the restricted environment;
    /// the rest cost nothing and keep the scan honest if it ever widens.
    const BLOCK_OPENING_TAGS: [&str; 8] =
        ["if", "for", "filter", "with", "block", "macro", "call", "autoescape"];

    /// Hide the two openers MiniJinja must not read: the `\{{` escape, which
    /// stands for a literal `{{`, and `{#`, which is ordinary text.
    ///
    /// One left-to-right pass, so the lexer's own precedence is preserved — in
    /// `{{#`, the `{{` wins and the `{#` one byte later is not an opener at
    /// all. A real opener is stepped over *whole*: what is inside it is an
    /// expression rather than text, so a `{#` in one of its string literals is
    /// still the `{#` the author wrote and still compares as one. An opener
    /// whose delimiter never closes holds the rest of the file, which is
    /// therefore copied as written — the template fails to parse either way,
    /// and §FS-rhei-templates.5.3 is what names that opener.
    /// §FS-rhei-templates.5 §FS-rhei-templates.5.2
    fn hide_non_syntax_openers(raw: &str) -> String {
        let bytes = raw.as_bytes();
        let mut out = String::with_capacity(raw.len());
        let mut copied = 0;
        let mut i = 0;

        while i < bytes.len() {
            if bytes[i] == b'\\' && bytes[i + 1..].starts_with(b"{{") {
                out.push_str(&raw[copied..i]);
                out.push(ESCAPED_INTERPOLATION_SENTINEL);
                i += 3;
                copied = i;
                continue;
            }
            if bytes[i] == b'{' {
                match bytes.get(i + 1) {
                    // A real opener: step over it so its body is not rewritten
                    // as if it were text.
                    Some(b'{') => match read_interpolation(raw, i) {
                        Some((end, _)) => {
                            i = end;
                            continue;
                        }
                        None => break,
                    },
                    Some(b'%') => match read_template_tag(raw, i) {
                        Some(tag) => {
                            i = tag.end;
                            continue;
                        }
                        None => break,
                    },
                    Some(b'#') => {
                        out.push_str(&raw[copied..i]);
                        out.push(COMMENT_OPENER_SENTINEL);
                        i += 2;
                        copied = i;
                        continue;
                    }
                    _ => {}
                }
            }
            i += 1;
        }

        out.push_str(&raw[copied..]);
        out
    }

    /// Put back what [`hide_non_syntax_openers`] hid, and hand back untouched
    /// what [`format_escaped_value`] marked as an input value's own — the
    /// preprocessor's sentinels are the only ones that stand for anything.
    /// §FS-rhei-templates.5 §FS-rhei-templates.5.1
    fn restore_hidden_openers(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars();

        while let Some(next) = chars.next() {
            match next {
                VALUE_SENTINEL_ESCAPE => {
                    if let Some(carried) = chars.next() {
                        out.push(carried);
                    }
                }
                COMMENT_OPENER_SENTINEL => out.push_str("{#"),
                ESCAPED_INTERPOLATION_SENTINEL => out.push_str("{{"),
                other => out.push(other),
            }
        }

        out
    }

    /// Mark every reserved code point an input value carries, so the restore
    /// gives the value its own bytes back rather than reading them as
    /// something the preprocessor hid. §FS-rhei-templates.5.1
    fn escape_value_sentinels(text: &str) -> String {
        let mut out = String::with_capacity(text.len());

        for next in text.chars() {
            if RESERVED_CODE_POINTS.contains(&next) {
                out.push(VALUE_SENTINEL_ESCAPE);
            }
            out.push(next);
        }

        out
    }

    /// Write one interpolated value, marking what it carries, in place of
    /// MiniJinja's default formatter — which is also where the strict check on
    /// an undefined value lives, so that check is repeated here rather than
    /// lost. Auto-escaping never applies: the environment loads no named
    /// template, so nothing selects a format for it.
    /// §FS-rhei-templates.5 §FS-rhei-templates.5.1
    fn format_escaped_value(
        out: &mut minijinja::Output,
        state: &minijinja::State,
        value: &minijinja::Value,
    ) -> Result<(), minijinja::Error> {
        if value.is_undefined()
            && matches!(state.undefined_behavior(), minijinja::UndefinedBehavior::Strict)
        {
            return Err(minijinja::Error::from(minijinja::ErrorKind::UndefinedError));
        }
        out.write_str(&escape_value_sentinels(&value.to_string())).map_err(minijinja::Error::from)
    }

    /// Refuse a template that already contains a reserved code point, rather
    /// than letting the round trip rewrite text the author meant.
    /// §FS-rhei-templates.5.4
    fn reject_reserved_code_points(raw: &str, path: &Path) -> MietteResult<()> {
        let Some(found) = RESERVED_CODE_POINTS.into_iter().find(|point| raw.contains(*point))
        else {
            return Ok(());
        };
        Err(miette!(
            help = "rhei stands these code points in for the text it hides from the parser \
                    while a template is rendered, so a template cannot contain one itself. \
                    Remove it from the file, then re-run.",
            "template '{}' contains the reserved code point U+{:04X}",
            path.display(),
            found as u32
        ))
    }

    /// A `{{` or `{%` the text opens and never closes. §FS-rhei-templates.5.3
    struct UnclosedOpener {
        /// The opener as written: `{{` or `{%`.
        opener: &'static str,
        /// 1-based line the opener itself sits on — never the later line at
        /// which the parser ran out of input.
        line: usize,
        /// What closes it: `}}`, `%}`, or an end tag such as `{% endif %}`.
        closing: String,
    }

    /// What one pass over a template's text can say about it without parsing.
    #[derive(Default)]
    struct TemplateTextScan {
        /// The innermost opener still open at end of input.
        unclosed: Option<UnclosedOpener>,
        /// 1-based line of the first `{#` that sits in text — the ones inside
        /// a `{% raw %}` region or inside an expression never were comments.
        first_comment_opener: Option<usize>,
    }

    /// The head of one `{% ... %}` tag.
    struct TemplateTag<'a> {
        /// The first word inside it: `if`, `endfor`, `raw`, ...
        name: &'a str,
        /// Byte index just past the closing `%}`.
        end: usize,
        /// Newlines between the opener and that `%}`.
        newlines: usize,
    }

    /// Read the tag that opens at `open`, or `None` when its `%}` never comes.
    fn read_template_tag(raw: &str, open: usize) -> Option<TemplateTag<'_>> {
        let bytes = raw.as_bytes();
        let mut i = open + 2;
        let mut newlines = 0;

        while i + 1 < bytes.len() {
            if bytes[i] == b'\n' {
                newlines += 1;
            }
            if bytes[i] == b'%' && bytes[i + 1] == b'}' {
                let body = raw[open + 2..i].trim().trim_matches(|c| c == '-' || c == '+').trim();
                return Some(TemplateTag {
                    name: body.split_whitespace().next().unwrap_or_default(),
                    end: i + 2,
                    newlines,
                });
            }
            i += 1;
        }

        None
    }

    /// Byte index just past the `}}` that closes the interpolation opening at
    /// `open`, with the newlines crossed on the way, or `None` when it never
    /// closes.
    fn read_interpolation(raw: &str, open: usize) -> Option<(usize, usize)> {
        let bytes = raw.as_bytes();
        let mut i = open + 2;
        let mut newlines = 0;

        while i + 1 < bytes.len() {
            if bytes[i] == b'\n' {
                newlines += 1;
            }
            if bytes[i] == b'}' && bytes[i + 1] == b'}' {
                return Some((i + 2, newlines));
            }
            i += 1;
        }

        None
    }

    /// Find the opener a malformed template never closed, and the first `{#`
    /// whose meaning has changed — one pass, because both questions are asked
    /// of the same tokens.
    ///
    /// A `{% raw %}` region is literal text: nothing inside one is blamed for
    /// being unclosed and nothing inside one is warned about
    /// (§FS-rhei-templates.5.3). Scanning stops at an opener whose delimiter
    /// never closes, because everything after it is inside that opener.
    fn scan_template_text(raw: &str) -> TemplateTextScan {
        let bytes = raw.as_bytes();
        let mut scan = TemplateTextScan::default();
        let mut open_blocks: Vec<UnclosedOpener> = Vec::new();
        let mut in_raw = false;
        let mut line = 1;
        let mut i = 0;

        while i < bytes.len() {
            if bytes[i] == b'\n' {
                line += 1;
                i += 1;
                continue;
            }
            if !in_raw && bytes[i] == b'\\' && bytes[i + 1..].starts_with(b"{{") {
                // A literal `{{`, not an opener. §FS-rhei-templates.5.2
                i += 3;
                continue;
            }
            if bytes[i] != b'{' {
                i += 1;
                continue;
            }

            match bytes.get(i + 1) {
                Some(b'%') => {
                    let Some(tag) = read_template_tag(raw, i) else {
                        if !in_raw {
                            scan.unclosed = Some(UnclosedOpener {
                                opener: "{%",
                                line,
                                closing: "%}".to_string(),
                            });
                            return scan;
                        }
                        i += 2;
                        continue;
                    };
                    let opener_line = line;
                    line += tag.newlines;
                    i = tag.end;

                    if in_raw {
                        if tag.name == "endraw" {
                            in_raw = false;
                            open_blocks.pop();
                        }
                    } else if tag.name == "raw" {
                        in_raw = true;
                        open_blocks.push(UnclosedOpener {
                            opener: "{%",
                            line: opener_line,
                            closing: "{% endraw %}".to_string(),
                        });
                    } else if BLOCK_OPENING_TAGS.contains(&tag.name) {
                        open_blocks.push(UnclosedOpener {
                            opener: "{%",
                            line: opener_line,
                            closing: format!("{{% end{} %}}", tag.name),
                        });
                    } else if let Some(ended) = tag.name.strip_prefix("end") {
                        // A block opener's `closing` is exactly its end tag, so
                        // this closes the innermost block only when the two
                        // names match. A mismatched pair leaves both open and
                        // the inner one — the one to close first — is named.
                        let wanted = format!("{{% end{ended} %}}");
                        if open_blocks.last().is_some_and(|open| open.closing == wanted) {
                            open_blocks.pop();
                        }
                    }
                }
                Some(b'{') => {
                    if in_raw {
                        i += 2;
                        continue;
                    }
                    let Some((end, newlines)) = read_interpolation(raw, i) else {
                        scan.unclosed =
                            Some(UnclosedOpener { opener: "{{", line, closing: "}}".to_string() });
                        return scan;
                    };
                    line += newlines;
                    i = end;
                }
                Some(b'#') => {
                    if !in_raw {
                        scan.first_comment_opener.get_or_insert(line);
                    }
                    i += 2;
                }
                _ => i += 1,
            }
        }

        if scan.unclosed.is_none() {
            // The innermost one, which is the one the author has to close first.
            scan.unclosed = open_blocks.pop();
        }
        scan
    }

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

    /// Resolve one text file's instantiation variables.
    ///
    /// The restricted environment is built per file so nothing a template does
    /// leaks into the next one. §FS-rhei-templates.5
    fn render_template_text(
        raw: &str,
        values: &BTreeMap<String, serde_json::Value>,
        path: &Path,
        template_ref: &str,
    ) -> MietteResult<String> {
        reject_reserved_code_points(raw, path)?;
        let preprocessed = hide_non_syntax_openers(raw);

        let mut env = MiniJinjaEnvironment::new();
        env.set_undefined_behavior(UndefinedBehavior::Strict);
        // MiniJinja strips a single trailing newline by default, which drops the
        // final newline from every instantiated file (states.yaml, settings.json,
        // task files, ...). Preserve it so rendered files keep the POSIX trailing
        // newline of their template source.
        env.set_keep_trailing_newline(true);
        // An input value is the author's, not the preprocessor's. §FS-rhei-templates.5.1
        env.set_formatter(format_escaped_value);
        env.add_filter("slug", |value: String| slugify_target_value(&value));

        let template = env
            .template_from_str(&preprocessed)
            .map_err(|err| template_parse_report(path, raw, &err))?;
        let rendered = template
            .render(values)
            .map_err(|err| template_render_report(path, &preprocessed, template_ref, &err))?;

        warn_about_hidden_comment_opener(raw, path);
        Ok(restore_hidden_openers(&rendered))
    }
