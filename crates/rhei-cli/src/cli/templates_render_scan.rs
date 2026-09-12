    // One left-to-right walk over a template's text, and the two questions
    // answered from it — a responsibility apart from hiding and restoring what
    // the walk finds, and a seam §AR-source-file-size.3 names.

    /// Tags that open a block the template has to close with `{% end... %}`.
    /// Only `if`, `for`, and `raw` are reachable in the restricted environment;
    /// the rest cost nothing and keep the scan honest if it ever widens.
    const BLOCK_OPENING_TAGS: [&str; 8] =
        ["if", "for", "filter", "with", "block", "macro", "call", "autoescape"];

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

    /// What the walk stops at. Ordinary text is never handed out.
    enum TemplateToken<'a> {
        /// A `\{{` standing for a literal `{{`. §FS-rhei-templates.5.2
        EscapedInterpolation,
        /// A `{{ ... }}` whose `}}` came.
        Interpolation,
        /// A `{% ... %}` whose `%}` came.
        Tag(TemplateTag<'a>),
        /// A `{#`, which is text rather than syntax. §FS-rhei-templates.5
        CommentOpener,
        /// A `{{` or `{%` whose delimiter never comes, so the rest of the file
        /// is inside it and the walk ends there.
        Unclosed(&'static str),
    }

    /// One token, and what the walk knew when it reached it.
    struct TokenAt<'a> {
        token: TemplateToken<'a>,
        /// Byte index of its first byte.
        start: usize,
        /// Byte index just past it.
        end: usize,
        /// 1-based line `start` sits on.
        line: usize,
        /// Whether it sits inside a `{% raw %}` region, where it is literal
        /// text rather than the construct it resembles.
        in_raw: bool,
    }

    /// One left-to-right walk over a template's text.
    ///
    /// The substitution that hides what is not syntax and the scan that decides
    /// what a malformed template is told both need to know where each construct
    /// begins and ends, and which of them sit inside a `{% raw %}` region —
    /// literal text, whatever they resemble (§FS-rhei-templates.5.2). They
    /// share this walk because two walks are two answers, and a substitution
    /// that reads a region's literal `{{` as an opener pairs it with a `}}`
    /// past the region and leaves every `{#` between them in the parser's way.
    ///
    /// The lexer's own precedence is preserved: in `{{#`, the `{{` wins and the
    /// `{#` one byte later is not an opener at all. A construct is handed out
    /// whole, so a `{#` in one of its string literals is part of an expression
    /// rather than text and still compares as the `{#` the author wrote.
    /// §FS-rhei-templates.5
    struct TemplateWalk<'a> {
        raw: &'a str,
        /// Byte index the next token is looked for at.
        i: usize,
        line: usize,
        in_raw: bool,
        /// Set by an unclosed opener: everything after one is inside it.
        stopped: bool,
    }

    impl<'a> TemplateWalk<'a> {
        fn new(raw: &'a str) -> Self {
            Self { raw, i: 0, line: 1, in_raw: false, stopped: false }
        }

        /// Hand out the token spanning `start..end` and resume after it.
        fn at(&mut self, token: TemplateToken<'a>, start: usize, end: usize) -> TokenAt<'a> {
            self.i = end;
            TokenAt { token, start, end, line: self.line, in_raw: self.in_raw }
        }
    }

    impl<'a> Iterator for TemplateWalk<'a> {
        type Item = TokenAt<'a>;

        fn next(&mut self) -> Option<TokenAt<'a>> {
            let raw = self.raw;
            let bytes = raw.as_bytes();

            while !self.stopped && self.i < bytes.len() {
                let start = self.i;
                if bytes[start] == b'\n' {
                    self.line += 1;
                    self.i += 1;
                    continue;
                }
                if bytes[start] == b'\\' && bytes[start + 1..].starts_with(b"{{") {
                    return Some(self.at(TemplateToken::EscapedInterpolation, start, start + 3));
                }
                if bytes[start] != b'{' {
                    self.i += 1;
                    continue;
                }

                match bytes.get(start + 1) {
                    Some(b'#') => {
                        return Some(self.at(TemplateToken::CommentOpener, start, start + 2));
                    }
                    Some(b'%') => {
                        let Some(tag) = read_template_tag(raw, start) else {
                            // Inside a region an unreadable tag is text, not a
                            // failure: only `{% endraw %}` means anything there.
                            if self.in_raw {
                                self.i = start + 2;
                                continue;
                            }
                            self.stopped = true;
                            return Some(self.at(
                                TemplateToken::Unclosed("{%"),
                                start,
                                bytes.len(),
                            ));
                        };
                        let (end, newlines) = (tag.end, tag.newlines);
                        let entering = !self.in_raw && tag.name == "raw";
                        let leaving = self.in_raw && tag.name == "endraw";
                        let at = self.at(TemplateToken::Tag(tag), start, end);
                        self.line += newlines;
                        self.in_raw = (self.in_raw || entering) && !leaving;
                        return Some(at);
                    }
                    Some(b'{') => {
                        // Inside a region it is literal text: it opens nothing,
                        // and pairing it with a `}}` outside would swallow both.
                        if self.in_raw {
                            self.i = start + 2;
                            continue;
                        }
                        let Some((end, newlines)) = read_interpolation(raw, start) else {
                            self.stopped = true;
                            return Some(self.at(
                                TemplateToken::Unclosed("{{"),
                                start,
                                bytes.len(),
                            ));
                        };
                        let at = self.at(TemplateToken::Interpolation, start, end);
                        self.line += newlines;
                        return Some(at);
                    }
                    _ => self.i += 1,
                }
            }

            None
        }
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

    /// Find the opener a malformed template never closed, and the first `{#`
    /// whose meaning has changed — one pass, because both questions are asked
    /// of the same tokens.
    ///
    /// A `{% raw %}` region is literal text: nothing inside one is blamed for
    /// being unclosed and nothing inside one is warned about
    /// (§FS-rhei-templates.5.3). Scanning stops at an opener whose delimiter
    /// never closes, because everything after it is inside that opener.
    fn scan_template_text(raw: &str) -> TemplateTextScan {
        let mut scan = TemplateTextScan::default();
        let mut open_blocks: Vec<UnclosedOpener> = Vec::new();

        for TokenAt { token, line, in_raw, .. } in TemplateWalk::new(raw) {
            match token {
                TemplateToken::Unclosed(opener) => {
                    let closing = if opener == "{{" { "}}" } else { "%}" };
                    scan.unclosed =
                        Some(UnclosedOpener { opener, line, closing: closing.to_string() });
                    return scan;
                }
                TemplateToken::CommentOpener => {
                    if !in_raw {
                        scan.first_comment_opener.get_or_insert(line);
                    }
                }
                TemplateToken::Tag(tag) if in_raw => {
                    if tag.name == "endraw" {
                        open_blocks.pop();
                    }
                }
                TemplateToken::Tag(tag) => {
                    if tag.name == "raw" {
                        open_blocks.push(UnclosedOpener {
                            opener: "{%",
                            line,
                            closing: "{% endraw %}".to_string(),
                        });
                    } else if BLOCK_OPENING_TAGS.contains(&tag.name) {
                        open_blocks.push(UnclosedOpener {
                            opener: "{%",
                            line,
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
                TemplateToken::EscapedInterpolation | TemplateToken::Interpolation => {}
            }
        }

        if scan.unclosed.is_none() {
            // The innermost one, which is the one the author has to close first.
            scan.unclosed = open_blocks.pop();
        }
        scan
    }
