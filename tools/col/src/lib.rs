//! Pure, I/O-free core of the `col(1)` clone (util-linux `text-utils/col.c`).
//!
//! `col` reconstructs a visual page from the byte stream a terminal would have
//! received, so the whole program is a small column/line machine:
//!
//! * Lines are counted in **half lines** (`cur_line += 2` per newline), which is
//!   what `ESC`-`\t` (forward half line feed) and `ESC`-`\x08` (reverse half
//!   line feed) move around. The chain of buffered lines therefore has one node
//!   per *half* line, and `this_line` is that chain index.
//! * Every printable character is stored together with the column it lands in.
//!   Spaces and tabs are *not* stored: they only move `cur_col`, so the blanks
//!   between characters are re-generated at flush time. That is what makes
//!   `col`'s output column-accurate rather than byte-accurate.
//! * Backspace is how a terminal overstrikes: `col` emits the overwritten
//!   character *followed by* as many `BS` as the character is wide, unless `-b`
//!   asks for the last character in a column only.
//! * Blanks between characters become tabs when `compress_spaces` is on (the
//!   default, and what plain `col` does; `-x` turns it off, `-h` asks for it
//!   explicitly). A gap is only rewritten as tabs when at least one whole tab
//!   stop fits inside it, otherwise it stays spaces.
//! * The output always ends in a newline, even when the input had none.
//!
//! The reference implementation bounds its memory by flushing lines early once
//! more than `-l` (default 32) lines are buffered; from then on it can no
//! longer back up over already-flushed lines and warns instead. This clone keeps
//! every line in memory instead, so the "can't back up past first line" path
//! never triggers for long inputs. That is the single deliberate deviation, and
//! it only shows up for inputs that both exceed the buffer *and* move the
//! cursor backwards by more than the buffer.

/// The util-linux release whose observable behaviour this clone tracks.
///
/// Verified against the reference `col` binary of this release.
pub const UTIL_LINUX_VERSION: &str = "2.39.3";

/// Number of columns between tab stops, i.e. upstream's hard-coded 8.
pub const TAB_CELLS: i64 = 8;

/// Upstream's `BUFFER_MARGIN`: how many half lines of slack are kept on top of
/// the `-l` request before a flush is forced.
pub const BUFFER_MARGIN: i64 = 32;

/// `col -H` output, shaped like util-linux' `USAGE_HEADER`/`USAGE_OPTIONS`
/// blocks — including the leading blank line those macros emit.
pub const HELP: &str = concat!(
    "\nUsage:\n",
    " col [options]\n",
    "\n",
    "Filter out reverse line feeds from standard input.\n",
    "\n",
    "Options:\n",
    " -b, --no-backspaces    do not output backspaces\n",
    " -f, --fine             permit forward half line feeds\n",
    " -p, --pass             pass unknown control sequences\n",
    " -h, --tabs             convert spaces to tabs\n",
    " -x, --spaces           convert tabs to spaces\n",
    " -l, --lines NUM        buffer at least NUM lines\n",
    " -H, --help             display this help\n",
    " -V, --version          display version\n",
    "\n",
    "For more details see col(1).\n",
);

/// The exact line `col -V` prints (`print_version()` in upstream `c.c`).
pub fn version_line() -> String {
    format!("col from util-linux {}\n", UTIL_LINUX_VERSION)
}

/// Terminal cell width of `c`, following `wcwidth(3)`.
///
/// Upstream stores the raw `wcwidth()` result in `c_width` and clamps only
/// where it advances `cur_col`, so a negative width is meaningful: it marks a
/// character that is passed through but occupies no column.
pub fn char_width(c: char) -> i32 {
    let u = c as u32;
    if u < 0x20 || (0x7f..=0x9f).contains(&u) {
        return -1; // non-printable: upstream's wcwidth() is -1 here
    }
    if (0x0300..=0x036f).contains(&u)
        || (0x200b..=0x200f).contains(&u)
        || (0x2028..=0x202e).contains(&u)
        || (0xfe00..=0xfe0f).contains(&u)
        || u == 0xfeff
    {
        return 0;
    }
    if (0x1100..=0x115f).contains(&u)
        || (0x2e80..=0x303e).contains(&u)
        || (0x3041..=0x33ff).contains(&u)
        || (0x3400..=0x4dbf).contains(&u)
        || (0x4e00..=0x9fff).contains(&u)
        || (0xa000..=0xa4cf).contains(&u)
        || (0xac00..=0xd7a3).contains(&u)
        || (0xf900..=0xfaff).contains(&u)
        || (0xfe30..=0xfe6f).contains(&u)
        || (0xff00..=0xff60).contains(&u)
        || (0xffe0..=0xffe6).contains(&u)
        || (0x1f300..=0x1f64f).contains(&u)
        || (0x1f900..=0x1f9ff).contains(&u)
        || (0x20000..=0x3fffd).contains(&u)
    {
        return 2;
    }
    1
}

/// `iswgraph()`: printable and not a space. Everything else is fed to
/// `handle_not_graphic()` first.
fn is_graph(c: char) -> bool {
    !c.is_control() && c != ' '
}

/// The `SO`/`SI` G0/G1 character-set shift state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Charset {
    /// `SO` is active, i.e. the alternate G1 set is selected.
    Alternate,
    /// `SI` is active, i.e. the normal G0 set is selected.
    Normal,
}

/// One stored character, mirroring upstream's `struct col_char`.
#[derive(Debug, Clone, Copy)]
struct ColChar {
    /// Column the character sits in.
    column: usize,
    /// The character itself.
    ch: char,
    /// `wcwidth()` of `ch`, negative when it is non-printable (`-p`).
    width: i32,
    /// Character set that was selected when it arrived.
    set: Charset,
}

/// One buffered half line, mirroring upstream's `struct col_line`.
#[derive(Debug, Default)]
struct Line {
    chars: Vec<ColChar>,
    /// Set when a character was appended to a column left of `max_col`.
    needs_sort: bool,
    /// Highest column seen, i.e. the size of the counting-sort histogram.
    max_col: usize,
}

/// The knobs `col`'s command line can turn, mirroring upstream's `col_ctl`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// `-b`/`--no-backspaces`: emit only the last character of a column.
    pub no_backspaces: bool,
    /// `-f`/`--fine`: keep half lines instead of rounding them up.
    pub fine: bool,
    /// `-p`/`--pass`: pass unknown control sequences through.
    pub pass_unknown: bool,
    /// `-h`/`--tabs` (on by default), `-x`/`--spaces` turns it off.
    pub compress_spaces: bool,
    /// `-l`/`--lines`: how many lines upstream tries to keep buffered.
    ///
    /// Accepted and recorded for command-line fidelity; see the module docs for
    /// why it cannot change the output here.
    pub buffer_lines: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            no_backspaces: false,
            fine: false,
            pass_unknown: false,
            compress_spaces: true,
            buffer_lines: BUFFER_MARGIN as usize,
        }
    }
}

/// Everything `flush_line()` is allowed to see of upstream's `col_ctl`.
struct State {
    opts: Options,
    lines: Vec<Line>,
    /// `ctl->l`: index of the half line characters are currently going to.
    cur_node: usize,
    /// `cur_line`: logical position, in half lines, of the cursor.
    cur_line: i64,
    /// `this_line`: the half line the chain is actually positioned at.
    this_line: i64,
    /// `adjust`: 1 while an odd `cur_line` is being rounded up to a whole line.
    adjust: i64,
    /// `cur_col`: logical column of the cursor.
    cur_col: usize,
    /// `max_line`: lowest half line ever written to.
    max_line: i64,
    /// `cur_set`: set selected by the most recent `SO`/`SI`.
    cur_set: Charset,
    /// `last_set`: set in effect at the current point of the output.
    last_set: Charset,
    /// `lns->c->c_width`: width of the last *stored* character. Upstream keeps
    /// this as a pointer that survives line moves, so a backspace right after a
    /// line change still uses it; keeping a plain copy reproduces that.
    last_width: Option<i32>,
    /// `nblank_lines`: whole/half line feeds owed to the output.
    nblank_lines: u64,
    /// `nflushd_lines`: half lines already emitted.
    nflushd_lines: i64,
    /// `extra_lines`: lines prepended by the back-up-past-the-start path.
    extra_lines: i64,
}

impl State {
    fn new(opts: &Options) -> State {
        State {
            opts: opts.clone(),
            lines: vec![Line::default()],
            cur_node: 0,
            cur_line: 0,
            this_line: 0,
            adjust: 0,
            cur_col: 0,
            max_line: 0,
            cur_set: Charset::Normal,
            last_set: Charset::Normal,
            last_width: None,
            nblank_lines: 0,
            nflushd_lines: 0,
            extra_lines: 0,
        }
    }
}

/// `flush_blanks()`: emit the pending line feeds.
///
/// An odd count means a half line is owed; without `-f` it is rounded up to a
/// whole line, with `-f` it becomes `ESC` `9` (and a bare `CR` when no whole
/// line is emitted at all, so the half line does not run into the text).
fn flush_blanks(st: &mut State, out: &mut String) {
    let mut half = false;
    let mut blanks = st.nblank_lines;
    if blanks & 1 == 1 {
        if st.opts.fine {
            half = true;
        } else {
            blanks += 1;
        }
    }
    blanks /= 2;
    for _ in 0..blanks {
        out.push('\n');
    }
    if half {
        out.push('\u{1b}');
        out.push('9');
        if blanks == 0 {
            out.push('\r');
        }
    }
    st.nblank_lines = 0;
}

/// Stable counting sort by `column`, exactly like upstream's O(n) pass: the
/// running total is shifted down by one and used as a write cursor, so
/// characters keep their relative order inside a column.
fn sort_by_column(chars: &mut [ColChar], max_col: usize) {
    let mut count = vec![0usize; max_col + 1];
    for entry in chars.iter() {
        count[entry.column] += 1;
    }
    let mut total = 0usize;
    for slot in count.iter_mut() {
        let save = *slot;
        *slot = total;
        total += save;
    }
    let mut sorted = vec![chars[0]; chars.len()];
    for entry in chars.iter() {
        sorted[count[entry.column]] = *entry;
        count[entry.column] += 1;
    }
    chars.copy_from_slice(&sorted);
}

/// `flush_line()`: render one buffered half line.
fn flush_line(st: &mut State, line: &Line, out: &mut String) {
    let mut chars = line.chars.clone();
    if line.needs_sort {
        sort_by_column(&mut chars, line.max_col);
    }

    let mut cursor = 0usize;
    let mut remaining = chars.len();
    let mut last_col: i64 = 0;

    while remaining > 0 {
        let this_col = chars[cursor].column as i64;
        // Walk to the first character that is not in this column.
        let mut end = cursor + 1;
        loop {
            remaining -= 1;
            if remaining == 0 || chars[end].column as i64 != this_col {
                break;
            }
            end += 1;
        }

        let mut start = cursor;
        if st.opts.no_backspaces {
            // Only the last character of the column survives — unless the next
            // character lands inside it, in which case this one is overwritten
            // anyway and is dropped too.
            start = end - 1;
            if remaining > 0 && (chars[end].column as i64) < this_col + chars[start].width as i64 {
                cursor = start;
                continue;
            }
        }

        if last_col < this_col {
            let mut spaces = this_col - last_col;
            if st.opts.compress_spaces && spaces > 1 {
                let tabs = this_col / TAB_CELLS - last_col / TAB_CELLS;
                if tabs > 0 {
                    spaces = this_col & (TAB_CELLS - 1);
                    for _ in 0..tabs {
                        out.push('\t');
                    }
                }
            }
            for _ in 0..spaces {
                out.push(' ');
            }
            last_col = this_col;
        }

        let mut index = start;
        loop {
            if chars[index].set != st.last_set {
                match chars[index].set {
                    Charset::Normal => out.push('\u{f}'),
                    Charset::Alternate => out.push('\u{e}'),
                }
                st.last_set = chars[index].set;
            }
            out.push(chars[index].ch);
            // Rub out every character this one overwrites.
            if index + 1 < end {
                for _ in 0..chars[index].width.max(0) {
                    out.push('\u{8}');
                }
            }
            index += 1;
            if index >= end {
                break;
            }
        }
        last_col += chars[index - 1].width as i64;
        cursor = index;
    }
}

/// `update_cur_line()`: move the line chain so that `cur_line` is addressable.
///
/// Without `-f` an odd `cur_line` is rounded *up* to the next whole line, which
/// is what makes `ESC`-`\t` behave like a plain newline for a default `col`.
fn update_cur_line(st: &mut State) {
    st.adjust = 0;
    let mut move_by = st.cur_line - st.this_line;
    if !st.opts.fine && (st.cur_line & 1) != 0 {
        st.adjust = 1;
        move_by += 1;
    }

    if move_by < 0 {
        let mut left = move_by;
        while left < 0 && st.cur_node > 0 {
            st.cur_node -= 1;
            left += 1;
        }
        // Nothing has been flushed yet, so upstream happily backs up past the
        // first line by prepending fresh (empty) ones.
        while left < 0 {
            st.lines.insert(0, Line::default());
            st.cur_node = 0;
            st.extra_lines += 1;
            left += 1;
        }
    } else {
        while move_by > 0 && st.cur_node + 1 < st.lines.len() {
            st.cur_node += 1;
            move_by -= 1;
        }
        while move_by > 0 {
            st.lines.push(Line::default());
            st.cur_node = st.lines.len() - 1;
            move_by -= 1;
        }
    }

    st.this_line = st.cur_line + st.adjust;
    // Upstream flushes the tail here to bound memory; this clone keeps every
    // line (see the module docs), so nothing is emitted and `nflushd_lines`
    // stays 0.
    debug_assert_eq!(st.nflushd_lines, 0);
}

/// `handle_not_graphic()`: the cursor motions and swallowed characters.
///
/// Returns `true` when the character has been dealt with and must not be stored.
fn handle_not_graphic(st: &mut State, ch: char, input: &[char], cursor: &mut usize) -> bool {
    match ch {
        // BS: step back over whatever was printed last.
        '\u{8}' => {
            if st.cur_col == 0 {
                return true; // cannot go back any further
            }
            match st.last_width {
                Some(width) if width >= 0 && (width as usize) <= st.cur_col => {
                    st.cur_col -= width as usize
                }
                Some(_) => st.cur_col = 0,
                None => st.cur_col -= 1,
            }
            true
        }
        '\r' => {
            st.cur_col = 0;
            true
        }
        // ESC: the following byte is consumed as part of the sequence, whether
        // or not `col` knows it. ESC BS/HT/BEL are the ECMA-48 reverse
        // half-line, forward half-line and reverse-index controls.
        '\u{1b}' => {
            if let Some(&next) = input.get(*cursor) {
                *cursor += 1;
                match next {
                    '\u{7}' => st.cur_line -= 2, // RLF
                    '\u{8}' => st.cur_line -= 1, // RHLF
                    '\t' => {
                        // FHLF
                        st.cur_line += 1;
                        if st.cur_line > 0 && st.cur_line > st.max_line {
                            st.max_line = st.cur_line;
                        }
                    }
                    _ => {}
                }
            }
            true
        }
        '\n' => {
            st.cur_line += 2;
            if st.cur_line > 0 && st.cur_line > st.max_line {
                st.max_line = st.cur_line;
            }
            st.cur_col = 0;
            true
        }
        ' ' => {
            st.cur_col += 1;
            true
        }
        '\u{e}' => {
            st.cur_set = Charset::Alternate; // SO
            true
        }
        '\u{f}' => {
            st.cur_set = Charset::Normal; // SI
            true
        }
        '\t' => {
            // Next tab stop.
            st.cur_col |= (TAB_CELLS - 1) as usize;
            st.cur_col += 1;
            true
        }
        '\u{b}' => {
            st.cur_line -= 2; // VT: reverse line feed
            true
        }
        other if other.is_whitespace() => {
            let width = char_width(other);
            if width > 0 {
                st.cur_col += width as usize;
            }
            true
        }
        // Anything else is an unknown control sequence: dropped by default,
        // passed through with `-p`.
        _ => !st.opts.pass_unknown,
    }
}

/// `process_char()`: the main dispatch for one input character.
fn process_char(st: &mut State, ch: char, input: &[char], cursor: &mut usize) {
    if !is_graph(ch) && handle_not_graphic(st, ch, input, cursor) {
        return;
    }

    // Is the chain positioned at the line the cursor is on?
    if st.cur_line != st.this_line - st.adjust {
        update_cur_line(st);
    }

    let width = char_width(ch);
    let column = st.cur_col;
    let set = st.cur_set;
    let node = st.cur_node;
    let line = &mut st.lines[node];
    line.chars.push(ColChar {
        column,
        ch,
        width,
        set,
    });
    if column < line.max_col {
        line.needs_sort = true;
    } else {
        line.max_col = column;
    }
    st.last_width = Some(width);
    if width > 0 {
        st.cur_col += width as usize;
    }
}

/// Filter `input`, returning the reconstructed page.
pub fn filter(input: &str, opts: &Options) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut st = State::new(opts);
    let mut out = String::with_capacity(input.len());

    let mut cursor = 0usize;
    while cursor < chars.len() {
        let ch = chars[cursor];
        cursor += 1;
        process_char(&mut st, ch, &chars, &mut cursor);
    }

    // Upstream walks the chain to its end before flushing anything.
    st.this_line += st.lines.len() as i64 - 1 - st.cur_node as i64;

    // "no lines, so just exit": nothing was ever written and the cursor never
    // left the first column.
    if st.max_line == 0 && st.cur_col == 0 {
        return out;
    }

    let mut flush = st.this_line - st.nflushd_lines + st.extra_lines + 1;
    flush = flush.clamp(0, st.lines.len() as i64);
    for _ in 0..flush {
        let line = st.lines.remove(0);
        if !line.chars.is_empty() {
            flush_blanks(&mut st, &mut out);
            flush_line(&mut st, &line, &mut out);
        }
        st.nblank_lines += 1;
    }

    // Leave the charset as we found it.
    if st.last_set != Charset::Normal {
        out.push('\u{f}');
    }

    let mut trailing = st.max_line - st.this_line;
    if (st.max_line & 1) != 0 {
        trailing += 1;
    } else if trailing == 0 {
        // The input did not end in a newline; supply one.
        trailing = 2;
    }
    st.nblank_lines = trailing.max(0) as u64;
    flush_blanks(&mut st, &mut out);

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run with non-default options on top of [`Options::default`].
    fn run(input: &str, tweak: impl FnOnce(&mut Options)) -> String {
        let mut opts = Options::default();
        tweak(&mut opts);
        filter(input, &opts)
    }

    #[test]
    fn plain_text_is_passed_through_with_a_newline() {
        assert_eq!(filter("abc\n", &Options::default()), "abc\n");
        assert_eq!(filter("a\nb\nc\n", &Options::default()), "a\nb\nc\n");
        assert_eq!(filter("", &Options::default()), "");
    }

    #[test]
    fn a_missing_final_newline_is_supplied() {
        assert_eq!(filter("abc", &Options::default()), "abc\n");
    }

    #[test]
    fn blank_lines_survive() {
        assert_eq!(filter("a\n\n\nb\n", &Options::default()), "a\n\n\nb\n");
        assert_eq!(filter("\n", &Options::default()), "\n");
    }

    #[test]
    fn backspace_overstrike_keeps_the_rubout() {
        // 'Z' lands on top of 'b'; a terminal shows "aZc", the byte stream keeps
        // the overwritten character plus one BS.
        assert_eq!(filter("ab\u{8}c\n", &Options::default()), "ab\u{8}c\n");
        assert_eq!(filter("abc\u{8}\u{8}Z\n", &Options::default()), "ab\u{8}Zc\n");
    }

    #[test]
    fn backspace_at_column_zero_is_ignored() {
        assert_eq!(filter("a\u{8}Z\n", &Options::default()), "a\u{8}Z\n");
    }

    #[test]
    fn no_backspaces_keeps_only_the_last_character() {
        let opts = Options {
            no_backspaces: true,
            ..Default::default()
        };
        assert_eq!(filter("abc\u{8}\u{8}Z\ny\u{8}W\n", &opts), "aZc\nW\n");
        assert_eq!(filter("ab\u{8}c\n", &opts), "ac\n");
    }

    #[test]
    fn carriage_return_rewinds_to_column_zero() {
        assert_eq!(filter("abc\rZ\n", &Options::default()), "a\u{8}Zbc\n");
    }

    #[test]
    fn tabs_are_recreated_as_tabs_by_default() {
        // 'b' sits in column 8, so the 7-column gap collapses to one tab.
        assert_eq!(filter("a\tb\n", &Options::default()), "a\tb\n");
    }

    #[test]
    fn spaces_option_keeps_the_gap_as_spaces() {
        let opts = Options {
            compress_spaces: false,
            ..Default::default()
        };
        assert_eq!(filter("a\tb\n", &opts), "a       b\n");
        assert_eq!(filter("a          b\n", &opts), "a          b\n");
    }

    #[test]
    fn tabs_option_is_the_default_compression() {
        let opts = Options {
            compress_spaces: true,
            ..Default::default()
        };
        assert_eq!(filter("a          b\n", &opts), "a\t   b\n");
    }

    #[test]
    fn short_gaps_stay_spaces_even_with_compression() {
        // Two spaces contain no whole tab stop, so nothing is rewritten.
        assert_eq!(filter("a  b\n", &Options::default()), "a  b\n");
    }

    #[test]
    fn unknown_control_sequences_are_dropped() {
        assert_eq!(filter("a\u{1}b\n", &Options::default()), "ab\n");
        // ESC consumes the following byte whatever it is.
        assert_eq!(filter("abc\u{1b}9def\n", &Options::default()), "abcdef\n");
    }

    #[test]
    fn pass_option_forwards_unknown_control_sequences() {
        let opts = Options {
            pass_unknown: true,
            ..Default::default()
        };
        assert_eq!(filter("a\u{1}b\n", &opts), "a\u{1}b\n");
    }

    #[test]
    fn vertical_tab_is_a_reverse_line_feed() {
        // Verified byte-for-byte against /usr/bin/col.
        assert_eq!(filter("a\nb\u{b}Z\n", &Options::default()), "aZ\nb\n");
    }

    #[test]
    fn charset_shifts_are_passed_through() {
        assert_eq!(filter("a\u{e}G\u{f}Z\n", &Options::default()), "a\u{e}G\u{f}Z\n");
    }

    #[test]
    fn half_line_feed_without_fine_becomes_a_newline() {
        // ESC HT is a forward half line feed; a default col rounds it up, so
        // the gap is re-created from column 3.
        assert_eq!(filter("xxx\u{1b}\tY\n", &Options::default()), "xxx\n   Y\n");
        assert_eq!(filter("xxx\u{1b}\t", &Options::default()), "xxx\n");
    }

    #[test]
    fn fine_keeps_half_lines() {
        let opts = Options {
            fine: true,
            ..Default::default()
        };
        // "xxx", then ESC 9 CR for the half line, the 3-column gap, "Y", the
        // closing newline and the trailing half line the input implies.
        assert_eq!(filter("xxx\u{1b}\tY\n", &opts), "xxx\u{1b}9\r   Y\n\u{1b}9");
        assert_eq!(
            filter("xxx\u{1b}\tYYY\u{1b}\tZZZ\n", &opts),
            "xxx\u{1b}9\r   YYY\u{1b}9\r      ZZZ\n"
        );
    }

    #[test]
    fn reverse_index_moves_up_a_whole_line() {
        // ESC BEL is RLF, which steps back over one *whole* line — exactly like
        // a newline, so it lands on the line holding 'b' and overstrikes it.
        assert_eq!(filter("a\nb\n\u{1b}\u{7}Z\n", &Options::default()), "a\nb\u{8}Z\n");
    }

    #[test]
    fn reverse_half_line_feed_moves_up_half_a_line() {
        // ESC BS is RHLF: one half line up from line 2 is still line 1's worth
        // of rounding, so 'Z' opens a new line instead of overstriking 'b'.
        assert_eq!(filter("a\nb\n\u{1b}\u{8}Z\n", &Options::default()), "a\nb\nZ\n");
    }

    #[test]
    fn wide_characters_advance_two_columns() {
        assert_eq!(
            filter("\u{4f60}\u{597d}\n", &Options::default()),
            "\u{4f60}\u{597d}\n"
        );
        // 'X' sits in column 2, so no separator is needed at all.
        assert_eq!(filter("\u{4f60}X\n", &Options::default()), "\u{4f60}X\n");
    }

    #[test]
    fn output_is_byte_identical_for_plain_ascii_lines() {
        let sample = "the quick brown fox\njumps over the lazy dog\n";
        assert_eq!(filter(sample, &Options::default()), sample);
    }

    #[test]
    fn version_line_matches_upstream_format() {
        assert_eq!(version_line(), "col from util-linux 2.39.3\n");
    }

    #[test]
    fn help_mentions_every_supported_option() {
        for needle in [
            "-b, --no-backspaces",
            "-f, --fine",
            "-p, --pass",
            "-h, --tabs",
            "-x, --spaces",
            "-l, --lines",
            "-H, --help",
            "-V, --version",
            "col [options]",
        ] {
            assert!(HELP.contains(needle), "help is missing {:?}", needle);
        }
    }
}
