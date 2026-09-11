//! Putting miette's soft wrap back the way it was made, so an assertion reads
//! the sentence the binary printed rather than the line the renderer produced.
//!
//! Captured stderr is never a tty, so every diagnostic arrives laid out to a
//! fixed column. That layout is correct — §FS-rhei-errors.2 asks for exactly
//! it, and for a break at a space rather than inside a token — but it is a
//! property of the renderer, not of the message, and a `contains` whose phrase
//! straddles a break sees nothing. Where the break lands moves with a pid, a
//! temporary path or a run id, so the assertion passes on one machine and
//! fails on the next.
//!
//! This is the inverse of that wrap and nothing more. Two rendered blocks are
//! never merged, and a continuation is rejoined with the single space the wrap
//! removed, so a token the renderer really did break stays broken and stays
//! findable.

/// The column miette lays a report out to when stderr is not a tty.
///
/// Its handler assumes an eighty-column terminal and lays the report two
/// columns in from that, so a rendered line is at most seventy-eight wide,
/// gutter included. Measured against miette 7.6.0 rather than derived: if a
/// release moves it, `diagnostic_wrap_tests` is what says so.
const RENDERED_WIDTH: usize = 78;

/// One rendered block: what opens it, and what the renderer puts in front of a
/// line it had to break.
///
/// A block is the unit a wrap happens inside. The message and the `help:`
/// footer are two of them, and a phrase read across the boundary between them
/// was never printed, so they are never joined.
struct Block {
    /// What the renderer writes in front of the block's first line.
    opening: &'static str,
    /// What it writes in front of every line after a break.
    continuation: &'static str,
}

/// Miette's blocks, in the Unicode theme the suite renders under.
///
/// The vertical bar is an opening as well as a continuation because a blank
/// line inside the message is a paragraph break: the block goes on afterwards,
/// and the line resuming it carries only the continuation.
const BLOCKS: &[Block] = &[
    Block { opening: "  \u{d7} ", continuation: "  \u{2502} " },
    Block { opening: "  \u{26a0} ", continuation: "  \u{2502} " },
    Block { opening: "  \u{261e} ", continuation: "  \u{2502} " },
    Block { opening: "  \u{2502} ", continuation: "  \u{2502} " },
    Block { opening: "  help: ", continuation: "        " },
];

/// The captured text with every soft wrap undone, and everything else
/// untouched.
///
/// stderr that is not a rendered diagnostic comes back as it arrived: two
/// lines a program wrote as two lines stay two lines, because joining those
/// would invent a sentence nobody printed. §FS-rhei-errors.2
pub fn undo_soft_wrap(rendered: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    // The block the previous line belonged to, and how wide that line was
    // after its gutter. A break can only be undone against the line it was
    // made from, so anything outside a block clears both.
    let mut open: Option<&'static Block> = None;
    let mut printed = 0usize;

    for line in rendered.split('\n') {
        // A line carrying the open block's continuation goes on with it; any
        // other line has to open a block of its own to be read as one at all.
        let carried = open.and_then(|block| Some((block, line.strip_prefix(block.continuation)?)));
        let opened =
            BLOCKS.iter().find_map(|block| Some((block, line.strip_prefix(block.opening)?)));
        let Some((block, rest)) = carried.or(opened) else {
            lines.push(line.to_string());
            open = None;
            printed = 0;
            continue;
        };

        match lines.last_mut() {
            Some(previous)
                if carried.is_some() && broke_here(printed, rest, content_width(block)) =>
            {
                previous.push(' ');
                previous.push_str(rest);
            }
            _ => lines.push(line.to_string()),
        }
        open = Some(block);
        printed = width(rest);
    }

    lines.join("\n")
}

/// Whether the renderer, and not the message, put the break before `rest`.
///
/// The wrap is greedy: it breaks only once the next word no longer fits. So a
/// continuation is a line whose first word could not have been added to the
/// line before it, and a predecessor that stopped short of the column stopped
/// because the message itself ended the line there. Joining at one of those
/// would run two of the binary's own lines into one sentence.
fn broke_here(printed: usize, rest: &str, available: usize) -> bool {
    let first_word = rest.split(' ').next().unwrap_or_default();
    !first_word.is_empty() && printed + 1 + width(first_word) > available
}

/// How much of a rendered line the block leaves for the message itself.
fn content_width(block: &Block) -> usize {
    RENDERED_WIDTH - width(block.continuation)
}

/// Columns `text` occupies, near enough for the text a diagnostic is made of.
///
/// The renderer measures display width; every gutter here, and all but a
/// vanishing share of what rhei prints, is one column per character. A
/// double-width character echoed back from a caller's own argument is
/// undercounted, and the error only ever runs one way: a break is left undone,
/// never invented, so the reading stays honest and at worst stays wrapped.
fn width(text: &str) -> usize {
    text.chars().count()
}
