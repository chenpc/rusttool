//! Pure core of the `colcrt(1)` clone (util-linux `text-utils/colcrt.c`).
//!
//! Upstream semantics mirrored here:
//!
//! * input is nroff/CRT output; `_` marks an underlined column (a `-` is put
//!   on the continuation line), `ESC 8` rubs out one column, `ESC 7` two,
//!   `\t` advances to the next multiple of 8 (a no-op when already on one),
//!   other control characters are dropped,
//! * `-` / `--no-underlining` suppresses the underline line,
//! * `-2` / `--half-lines` prints a leading blank line plus a blank line per
//!   input line (half-line previewing),
//! * lines longer than 132 columns are flushed and the rest of the input line
//!   is discarded up to the next `\n` (upstream's `OUTPUT_COLS` rule),
//! * a final line without `\n` is still emitted (at EOF `print_nl` is forced
//!   off, so no extra newline is added).
//!
//! Deviation: input is decoded as UTF-8 lossily and printability follows
//! Rust's `char::is_control` (upstream uses the locale's `iswprint`).

/// The util-linux release whose observable behaviour this clone tracks.
pub const UTIL_LINUX_VERSION: &str = "2.42";

/// `colcrt -h` output.
pub const HELP: &str = concat!(
    "\nUsage:\n",
    " colcrt [options] [<file>...]\n",
    "\n",
    "Filter nroff output for CRT previewing.\n",
    "\n",
    "Options:\n",
    " -,  --no-underlining    suppress all underlining\n",
    " -2, --half-lines        print all half-lines\n",
    "\n",
    " -h, --help              display this help\n",
    " -V, --version           display version\n",
);

/// The exact line `colcrt -V` prints.
pub fn version_line() -> String {
    format!("colcrt from util-linux {}\n", UTIL_LINUX_VERSION)
}

/// Width of upstream's `line`/`line_under` buffers (`OUTPUT_COLS` = 132).
pub const OUTPUT_COLS: usize = 132;

/// Filter `input` for CRT previewing.
///
/// `input` is the decoded text of one or more files concatenated (callers
/// join files with the same state machine upstream uses per file; for the
/// stateless parts this is identical). Returns the exact bytes to emit.
pub fn colcrt_filter(input: &str, no_underlining: bool, half_lines: bool) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    // Buffers: '\0' = empty slot, ' ' = blank underline slot.
    let mut line = vec!['\0'; OUTPUT_COLS];
    let mut under = vec![' '; OUTPUT_COLS];
    let mut col: isize = 0;
    let mut print_nl = true;
    let mut need_under = false;
    if half_lines {
        out.push('\n');
    }
    let mut i = 0;
    // Emit the pending line like upstream output_lines().
    let mut flush = |col: isize,
                     line: &mut Vec<char>,
                     under: &mut Vec<char>,
                     print_nl: &mut bool,
                     need_under: &mut bool,
                     out: &mut String| {
        let end = col.max(0) as usize;
        // Trim trailing spaces/NULs of the text line.
        let mut last = 0usize;
        for k in 0..end.min(OUTPUT_COLS) {
            if line[k] != '\0' && line[k] != ' ' {
                last = k + 1;
            }
        }
        for k in 0..last {
            let c = line[k];
            out.push(if c == '\0' { ' ' } else { c });
        }
        if *print_nl {
            out.push('\n');
        }
        if !half_lines && !no_underlining {
            *print_nl = false;
        }
        for k in line.iter_mut() {
            *k = '\0';
        }
        if *need_under {
            *need_under = false;
            let mut ulast = 0usize;
            for k in 0..end.min(OUTPUT_COLS) {
                if under[k] != ' ' && under[k] != '\0' {
                    ulast = k + 1;
                }
            }
            for k in 0..ulast {
                out.push(under[k]);
            }
            out.push('\n');
            for k in under.iter_mut() {
                *k = ' ';
            }
        } else if half_lines && col > 0 {
            out.push('\n');
        }
    };

    while i < chars.len() {
        if col > OUTPUT_COLS as isize - 1 {
            flush(col, &mut line, &mut under, &mut print_nl, &mut need_under, &mut out);
            // Discard up to and including the next newline.
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            col = -1;
            // Fall through so the newline (if any) is processed normally.
            if i >= chars.len() {
                break;
            }
        }
        let c = chars[i];
        i += 1;
        if c == '\x1b' {
            // ESC followed by '8' rubs one column, '7' two; anything else is
            // dropped together with the ESC.
            if i < chars.len() {
                let d = chars[i];
                i += 1;
                if d == '8' {
                    let mut n = 1;
                    while n > 0 && col > 0 {
                        line[col as usize] = '\0';
                        under[col as usize] = ' ';
                        n -= 1;
                        col -= 1;
                    }
                } else if d == '7' {
                    let mut n = 2;
                    while n > 0 && col > 0 {
                        line[col as usize] = '\0';
                        under[col as usize] = ' ';
                        n -= 1;
                        col -= 1;
                    }
                }
            }
            continue;
        }
        match c {
            '\n' => {
                flush(col, &mut line, &mut under, &mut print_nl, &mut need_under, &mut out);
                col = -1;
            }
            '\t' => {
                while col % 8 != 0 && col < OUTPUT_COLS as isize {
                    if (col as usize) < OUTPUT_COLS {
                        line[col as usize] = ' ';
                    }
                    col += 1;
                }
                col -= 1;
            }
            '_' => {
                if (col as usize) < OUTPUT_COLS && col >= 0 {
                    line[col as usize] = ' ';
                    if !no_underlining {
                        need_under = true;
                        under[col as usize] = '-';
                    }
                }
            }
            _ => {
                if c.is_control() {
                    // Upstream drops non-printables (col-- cancels the col++).
                    continue;
                }
                print_nl = true;
                if (col as usize) < OUTPUT_COLS && col >= 0 {
                    line[col as usize] = c;
                }
            }
        }
        col += 1;
    }
    print_nl = false;
    flush(col, &mut line, &mut under, &mut print_nl, &mut need_under, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(s: &str) -> String {
        colcrt_filter(s, false, false)
    }

    #[test]
    fn plain_text_passes_through() {
        assert_eq!(f("hello\nworld\n"), "hello\nworld\n");
    }

    #[test]
    fn underscore_produces_underline_line() {
        // '_' at column 1: text line "A " trims to "A", underline " -" trims to " -".
        assert_eq!(f("A_\n"), "A\n -\n");
    }

    #[test]
    fn no_underlining_suppresses_it() {
        assert_eq!(colcrt_filter("A_\n", true, false), "A\n");
    }

    #[test]
    fn tab_advances_to_multiple_of_8() {
        // 'a' at col 0, tab moves to col 8, 'b' lands at 8.
        let got = f("a\tb\n");
        assert_eq!(got, "a       b\n");
        // A tab already on a multiple of 8 is a no-op upstream.
        assert_eq!(f("\tb\n"), "b\n");
    }

    #[test]
    fn esc_8_rubs_one_column() {
        assert_eq!(f("ab\x1b8c\n"), "ac\n");
    }

    #[test]
    fn esc_7_rubs_two_columns() {
        assert_eq!(f("abc\x1b7d\n"), "ad\n");
    }

    #[test]
    fn half_lines_adds_blanks() {
        assert_eq!(colcrt_filter("test\n", false, true), "\ntest\n\n");
    }

    #[test]
    fn missing_trailing_newline_still_emits() {
        assert_eq!(f("abc"), "abc");
        assert_eq!(f("abc\ndef"), "abc\ndef");
    }

    #[test]
    fn control_chars_are_dropped() {
        // Backspace is not printable for colcrt: "a\ba" becomes "aa".
        assert_eq!(f("a\x08a\n"), "aa\n");
    }

    #[test]
    fn version_line_matches_upstream_format() {
        assert_eq!(version_line(), "colcrt from util-linux 2.42\n");
    }
}
