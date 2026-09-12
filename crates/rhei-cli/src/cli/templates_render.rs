    // The restricted instantiation environment and the round trip that keeps
    // text which is not syntax out of the parser's way — a responsibility apart
    // from reading what the user passed, a seam §AR-source-file-size.3 names.

    // The walk both halves share is in `templates_render_scan`, and what a
    // template that will not render is told is in
    // `templates_render_diagnostics`.

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

    /// Hide the two openers MiniJinja must not read: the `\{{` escape, which
    /// stands for a literal `{{`, and `{#`, which is ordinary text.
    ///
    /// Both are hidden inside a `{% raw %}` region as well as outside one, and
    /// put back the same way: the region's text is emitted verbatim either way,
    /// and `\{{` has always lost its backslash inside one. What the walk does
    /// *not* do inside a region is read a `{{` as an opener — it is literal
    /// text there, and pairing it with a `}}` past the end of the region skips
    /// over every `{#` in between and hands the parser the comment the ticket
    /// is about. A real opener outside a region is stepped over whole, so a
    /// `{#` in one of its string literals reaches the parser as written; an
    /// opener whose delimiter never closes holds the rest of the file, which is
    /// therefore copied as written, because the template fails to parse either
    /// way and §FS-rhei-templates.5.3 is what names that opener.
    /// §FS-rhei-templates.5 §FS-rhei-templates.5.2
    fn hide_non_syntax_openers(raw: &str) -> String {
        let mut out = String::with_capacity(raw.len());
        let mut copied = 0;

        for at in TemplateWalk::new(raw) {
            let sentinel = match at.token {
                TemplateToken::EscapedInterpolation => ESCAPED_INTERPOLATION_SENTINEL,
                TemplateToken::CommentOpener => COMMENT_OPENER_SENTINEL,
                TemplateToken::Interpolation | TemplateToken::Tag(_) => continue,
                TemplateToken::Unclosed(_) => break,
            };
            out.push_str(&raw[copied..at.start]);
            out.push(sentinel);
            copied = at.end;
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
    /// MiniJinja's default formatter.
    ///
    /// The strict check on an undefined value is not repeated here, because
    /// `Environment::format` makes it before it dispatches to a formatter at
    /// all — and makes it only for the undefined an expression named without
    /// declaring, which is what §FS-rhei-templates.5 is strict about. An
    /// if-expression with no `else` yields a *silent* undefined the engine
    /// exempts on purpose, and goes on rendering as empty text.
    ///
    /// What this does drop is auto-escaping: `{% autoescape %}` selects a
    /// format from inside the text, and this formatter does not read it. The
    /// tag is not one of the constructs §FS-rhei-templates.5 lists and that
    /// list is closed, so a template using it was never inside the language
    /// rhei renders. §FS-rhei-templates.5 §FS-rhei-templates.5.1
    fn format_escaped_value(
        out: &mut minijinja::Output,
        _state: &minijinja::State,
        value: &minijinja::Value,
    ) -> Result<(), minijinja::Error> {
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
