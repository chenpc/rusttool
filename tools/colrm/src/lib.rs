//! Pure, I/O-free core of the `colrm(1)` clone (util-linux `text-utils/colrm.c`).
//!
//! Upstream semantics this mirrors, straight out of `process_input()`:
//!
//! * `colrm` copies standard input to standard output, dropping the columns in
//!   the inclusive range `first..=last`. With only `first`, everything from
//!   `first` to the end of the line goes.
//! * Columns are counted in *display* positions, not bytes: a tab advances to
//!   the next multiple of 8, a backspace moves back one position, and any other
//!   character advances by its `wcwidth()`.
//! * A removed range is replaced by blanks, so the remaining text keeps its
//!   original horizontal alignment. The blank run is emitted in two pieces: one
//!   covering the part of the first straddling character that lies *before*
//!   `first`, and one covering the columns between `last` and the next
//!   character. Together they reproduce the upstream byte stream exactly (see
//!   [`remove_columns`] and the `colrm.tabs` unit test).
//! * `first == 0` short-circuits the whole filter: upstream tests `!first ||
//!   ct < first`, so with `first == 0` nothing is ever removed and the input is
//!!   copied verbatim. This is why plain `colrm` is a pass-through, and why
//!   `colrm 0 5` is too.
//!
//! **Note on the "no operand" form.** Upstream `colrm` has no mode that reads
//! the start/stop columns from stdin; with no operands `first` and `last` are
//! both `0`, i.e. the pass-through described above. That is the behaviour
//! implemented here, deliberately, in preference to inventing a
//! read-start/stop-from-stdin variant that upstream does not have.

/// The util-linux release whose observable behaviour this clone tracks.
///
/// Verified against the reference `colrm` binary of this release.
pub const UTIL_LINUX_VERSION: &str = "2.39.3";

/// Number of columns between tab stops, i.e. upstream's hard-coded 8.
pub const TAB_CELLS: u64 = 8;

/// `colrm -h` output, shaped like util-linux' `USAGE_HEADER`/`USAGE_OPTIONS`
/// blocks — including the leading blank line those macros emit.
pub const HELP: &str = concat!(
    "\nUsage:\n",
    " colrm [startcol [endcol]]\n",
    "\n",
    "Filter out the specified columns.\n",
    "\n",
    "Options:\n",
    " -h, --help     display this help\n",
    " -V, --version  display version\n",
    "colrm reads from standard input and writes to standard output\n",
    "\n",
    "For more details see colrm(1).\n",
);

/// The exact line `colrm -V` prints (`print_version()` in upstream `c.c`).
pub fn version_line() -> String {
    format!("colrm from util-linux {}\n", UTIL_LINUX_VERSION)
}

/// Terminal cell width of `c`, following `wcwidth(3)`.
///
/// Upstream clamps a negative `wcwidth()` to 0, so control characters (a tab,
/// most importantly) do not advance the column counter of their own accord;
/// `colrm` handles tab and backspace explicitly before falling back here.
pub fn char_width(c: char) -> i32 {
    let u = c as u32;
    // C0 and C1 controls are non-printing.
    if u < 0x20 || (0x7f..=0x9f).contains(&u) {
        return 0;
    }
    // Zero-width: combining marks and format characters.
    if (0x0300..=0x036f).contains(&u)
        || (0x0483..=0x0489).contains(&u)
        || (0x0591..=0x05bd).contains(&u)
        || u == 0x05bf
        || (0x0610..=0x061a).contains(&u)
        || (0x064b..=0x065f).contains(&u)
        || u == 0x0670
        || (0x06d6..=0x06dc).contains(&u)
        || (0x0730..=0x074a).contains(&u)
        || (0x07a6..=0x07b0).contains(&u)
        || (0x0900..=0x0903).contains(&u)
        || u == 0x093a
        || (0x093c..=0x094f).contains(&u)
        || (0x0951..=0x0957).contains(&u)
        || (0x1ab0..=0x1aff).contains(&u)
        || (0x1dc0..=0x1dff).contains(&u)
        || (0x200b..=0x200f).contains(&u)
        || (0x2028..=0x202e).contains(&u)
        || (0x2060..=0x2064).contains(&u)
        || (0x20d0..=0x20f0).contains(&u)
        || (0xfe00..=0xfe0f).contains(&u)
        || (0xfe20..=0xfe2f).contains(&u)
        || u == 0xfeff
        || (0xe0100..=0xe01ef).contains(&u)
    {
        return 0;
    }
    // East Asian Wide / Fullwidth occupy two cells.
    if (0x1100..=0x115f).contains(&u)
        || (0x2e80..=0x303e).contains(&u)
        || (0x3041..=0x33ff).contains(&u)
        || (0x3400..=0x4dbf).contains(&u)
        || (0x4e00..=0x9fff).contains(&u)
        || (0xa000..=0xa4cf).contains(&u)
        || (0xa960..=0xa97f).contains(&u)
        || (0xac00..=0xd7a3).contains(&u)
        || (0xf900..=0xfaff).contains(&u)
        || (0xfe10..=0xfe19).contains(&u)
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

/// The column range to delete, mirroring upstream's `first`/`last` pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Range {
    /// First column to delete (1-based, inclusive).
    pub first: u64,
    /// Last column to delete (1-based, inclusive); `0` means "to end of line".
    pub last: u64,
}

/// `process_input()` for one line: returns `true` when another line follows.
///
/// `cursor` is advanced past everything consumed, so the caller can loop until
/// `false`, exactly like upstream's `while (process_input(...));`.
fn process_line(chars: &[char], cursor: &mut usize, range: Range, out: &mut String) -> bool {
    let mut column: u64 = 0;

    // Phase 1: copy characters until one reaches or passes `first`.
    let first_width: i64 = loop {
        let Some(&c) = chars.get(*cursor) else {
            return false;
        };
        *cursor += 1;
        let width: i64 = if c == '\t' {
            (((column + TAB_CELLS) & !(TAB_CELLS - 1)) as i64) - (column as i64)
        } else if c == '\u{8}' {
            (if column > 0 { column - 1 } else { 0 } as i64) - (column as i64)
        } else {
            char_width(c).max(0) as i64
        };
        column = (column as i64 + width) as u64;
        if c == '\n' {
            out.push('\n');
            column = 0;
            continue;
        }
        if range.first == 0 || column < range.first {
            out.push(c);
            continue;
        }
        break width;
    };

    // Upstream: `for (i = ct - w + 1; i < first; i++) putwc(' ')` — the part of
    // the straddling character that lies before `first`.
    let mut column_before = (column as i64 - first_width + 1).max(0);
    while column_before < range.first as i64 {
        out.push(' ');
        column_before += 1;
    }

    // Phase 2: swallow characters until the cursor reaches `last`.
    while range.last == 0 || column < range.last {
        let Some(&c) = chars.get(*cursor) else {
            return false;
        };
        *cursor += 1;
        if c == '\n' {
            out.push('\n');
            return true;
        }
        column = if c == '\t' {
            (column + TAB_CELLS) & !(TAB_CELLS - 1)
        } else if c == '\u{8}' {
            column.saturating_sub(1)
        } else {
            (column as i64 + char_width(c).max(0) as i64) as u64
        };
    }

    // Phase 3: copy the rest of the line, blanking the columns just past
    // `last` the first time a character needs it. Upstream deliberately does
    // not advance `column` here.
    let mut padded = false;
    loop {
        let Some(&c) = chars.get(*cursor) else {
            break;
        };
        *cursor += 1;
        if c == '\n' {
            out.push('\n');
            return true;
        }
        if !padded && range.last < column {
            for _ in range.last..column {
                out.push(' ');
            }
            padded = true;
        }
        out.push(c);
    }
    false
}

/// Copy `input`, deleting columns `first..=last` (`last == 0` means "to the end
/// of each line").
pub fn remove_columns(input: &str, range: Range) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut cursor = 0usize;
    let mut out = String::with_capacity(input.len());
    while process_line(&chars, &mut cursor, range, &mut out) {}
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rm(input: &str, first: u64, last: u64) -> String {
        remove_columns(input, Range { first, last })
    }

    #[test]
    fn no_arguments_is_a_verbatim_copy() {
        // `!first` short-circuits every branch, so colrm degenerates to cat(1).
        for sample in ["", "abc\n", "abcdef\n", "a\tb\tc\n", "\t\t\n", "no newline"] {
            assert_eq!(rm(sample, 0, 0), sample, "pass-through changed {:?}", sample);
        }
    }

    #[test]
    fn first_zero_also_disables_removal_even_with_a_last() {
        // Upstream tests `!first || ct < first`, so `first == 0` wins.
        assert_eq!(rm("abcdefgh\n", 0, 5), "abcdefgh\n");
    }

    #[test]
    fn start_only_removes_to_the_end_of_the_line() {
        assert_eq!(rm("abcdefgh\n", 3, 0), "ab\n");
        assert_eq!(rm("abcdef\n", 7, 0), "abcdef\n");
    }

    #[test]
    fn single_column_range() {
        assert_eq!(rm("abcdefgh\n", 4, 4), "abcefgh\n");
    }

    #[test]
    fn range_at_the_start_of_the_line() {
        assert_eq!(rm("abcdefgh\n", 1, 3), "defgh\n");
    }

    #[test]
    fn range_in_the_middle_is_replaced_by_blanks() {
        // Columns 3..=5 of "abcdefgh" go; 'f' starts at column 6 so no blank is
        // needed between the two surviving pieces.
        assert_eq!(rm("abcdefgh\n", 3, 5), "abfgh\n");
        assert_eq!(rm("abcdefgh\n", 2, 4), "aefgh\n");
    }

    #[test]
    fn removed_range_beyond_the_line_end() {
        // `last` past the end of the line leaves the newline in place;
        // verified against the 2.39.3 oracle (`abcdefgh` with 6 9 -> `abcde`).
        assert_eq!(rm("abcdefgh\n", 6, 9), "abcde\n");
    }

    #[test]
    fn start_beyond_the_line_end_keeps_the_line() {
        assert_eq!(rm("ab\n", 5, 0), "ab\n");
        assert_eq!(rm("ab\n", 3, 5), "ab\n");
    }

    #[test]
    fn tabs_advance_to_the_next_tab_stop() {
        // 'a' is column 1, the tab covers columns 2..=8, so columns 3..=5 are
        // blanked with one space (the tab's column 2) plus three spaces (the
        // gap between `last` = 5 and the column of 'b', which is 9).
        assert_eq!(rm("a\tb\tc\n", 3, 5), "a    b\tc\n");
        // Starting inside the tab: column 2 survives, nothing is blanked before
        // `last` because the cursor is already past it.
        assert_eq!(rm("ab\tcdef\n", 2, 4), "a    cdef\n");
    }

    #[test]
    fn tab_stop_is_every_eight_columns() {
        // 'a'=1, the first tab covers 2..=8, 'b'=9, the second tab covers
        // 10..=16, 'c'=17.
        assert_eq!(rm("a\tb\tc\n", 10, 11), "a\tb     c\n");
        assert_eq!(rm("a\tb\tc\n", 9, 9), "a\t\tc\n");
    }

    #[test]
    fn backspace_moves_the_column_counter_back() {
        // 'a'=1, 'b'=2, BS -> back to 1, 'c'=1, 'd'=2. Removing column 3 never
        // triggers, so the line is unchanged apart from the backspace itself.
        assert_eq!(rm("ab\u{8}cd\n", 3, 5), "ab\u{8}c\n");
    }

    #[test]
    fn backspace_at_column_zero_does_not_underflow() {
        // The backspace cannot move before column 1, so the column counter
        // stays at 0 and 'a' lands in column 1; removing column 2 drops 'b'.
        // Verified against the 2.39.3 oracle.
        assert_eq!(rm("\u{8}abc\n", 2, 2), "\u{8}ac\n");
    }

    #[test]
    fn wide_characters_occupy_two_columns() {
        // U+4F60 and U+597D are East Asian Wide, so they cover columns 1..=2
        // and 3..=4 and 'x' starts at column 5. Verified against the oracle:
        // removing 1..=2 drops the first wide char, removing column 3
        // straddles the second one.
        assert_eq!(rm("\u{4f60}\u{597d}x\n", 1, 2), "\u{597d}x\n");
        assert_eq!(rm("\u{4f60}\u{597d}x\n", 3, 3), "\u{4f60} x\n");
    }

    #[test]
    fn control_characters_do_not_advance_the_counter() {
        // The bell occupies no column, so 'b' is at column 2 and 'x' at
        // column 3. Verified against the 2.39.3 oracle.
        assert_eq!(rm("a\u{7}bx\n", 3, 3), "a\u{7}b\n");
        assert_eq!(rm("a\u{7}bx\n", 4, 4), "a\u{7}bx\n");
    }

    #[test]
    fn several_lines_are_processed_independently() {
        assert_eq!(rm("abcdef\nxy\n", 3, 0), "ab\nxy\n");
        assert_eq!(rm("abc\ndef\n", 2, 2), "ac\ndf\n");
    }

    #[test]
    fn a_missing_final_newline_is_preserved() {
        assert_eq!(rm("abcdef", 3, 4), "abef");
        assert_eq!(rm("abcdef", 3, 0), "ab");
    }

    #[test]
    fn empty_input_produces_empty_output() {
        assert_eq!(rm("", 1, 3), "");
    }

    #[test]
    fn removing_from_column_one_empties_every_line() {
        // `last == 0` swallows the rest of each line, newline included, and
        // `process_line()` reports the line as handled — so only the newlines
        // survive.
        let sample = "the quick brown fox\njumps over\n";
        assert_eq!(rm(sample, 1, 0), "\n\n");
        // A range past the end of every line changes nothing.
        assert_eq!(rm(sample, 100, 200), sample);
    }

    #[test]
    fn version_line_matches_upstream_format() {
        assert_eq!(version_line(), "colrm from util-linux 2.39.3\n");
    }

    #[test]
    fn help_mentions_every_supported_option() {
        for needle in ["colrm [startcol [endcol]]", "-h, --help", "-V, --version"] {
            assert!(HELP.contains(needle), "help is missing {:?}", needle);
        }
    }
}
