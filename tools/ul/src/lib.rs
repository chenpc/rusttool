//! Pure core of the `ul(1)` clone (util-linux `text-utils/ul.c`).
//!
//! Upstream semantics mirrored here:
//!
//! * `_` + backspace + `X` (or `X` + backspace + `_`) marks `X` underlined,
//!   `X` + backspace + `X` marks it bold; `\b` moves one column back, `\t`
//!   jumps to the next multiple of 8, `\r` returns to column 0,
//! * `ESC 7` / `ESC 8` / `ESC 9` (full/half reverse/forward) flush the pending
//!   line; any other escape is an error,
//! * `-i`/`--indicated` appends a second line per styled line (`_` for
//!   underline, `!` for bold, ` ` elsewhere, trailing blanks trimmed),
//! * `-t`/`--terminal dumb` (or `TERM=dumb`/unset) emits plain characters;
//!   any other terminal emits ANSI standout sequences.
//!
//! Deviations (documented): no terminfo is linked — the "capable" terminal
//! emits fixed ANSI codes (`\x1b[4m` underline, `\x1b[1m` bold,
//! `\x1b[m` reset) instead of the terminfo `smul`/`bold`/`sgr0` strings, and
//! every character is treated as single-width (no `wcwidth`).

/// The util-linux release whose observable behaviour this clone tracks.
pub const UTIL_LINUX_VERSION: &str = "2.42";

/// `ul -h` output.
pub const HELP: &str = concat!(
    "\nUsage:\n",
    " ul [options] [<file> ...]\n",
    "\n",
    "Do underlining.\n",
    "\n",
    "Options:\n",
    " -t, -T, --terminal TERMINAL  override the TERM environment variable\n",
    " -i, --indicated              underlining is indicated via a separate line\n",
    " -h, --help                   display this help\n",
    " -V, --version                display version\n",
);

/// The exact line `ul -V` prints.
pub fn version_line() -> String {
    format!("ul from util-linux {}\n", UTIL_LINUX_VERSION)
}

const UNDERLINE: u8 = 1;
const BOLD: u8 = 2;

#[derive(Debug, Clone, Copy)]
struct Cell {
    ch: Option<char>,
    mode: u8,
}

/// Filter `input` into terminal output.
///
/// `dumb` selects plain output; `indicated` appends the `_`/`!` marker line.
/// Returns the bytes to emit, or an error string for unknown escapes.
pub fn ul_filter(input: &str, dumb: bool, indicated: bool) -> Result<String, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut buf: Vec<Cell> = Vec::new();
    let mut column: usize = 0usize;
    let mut max_column: usize = 0;
    let mut i = 0;

    let ensure = |buf: &mut Vec<Cell>, column: usize, max_column: &mut usize| {
        if buf.len() <= column {
            buf.resize(column + 1, Cell { ch: None, mode: 0 });
        }
        if *max_column < column + 1 {
            *max_column = column + 1;
        }
    };

    // Render and reset the pending line.
    let mut flush =
        |buf: &mut Vec<Cell>, max_column: &mut usize, out: &mut String, column: &mut usize| {
            if *max_column == 0 {
                // An empty pending line still emits its newline when it came
                // from an explicit '\n' (handled by the caller passing
                // through); nothing to do here for EOF.
                return;
            }
            let mut current: u8 = 0;
            for k in 0..*max_column {
                let cell = buf.get(k).copied().unwrap_or(Cell { ch: None, mode: 0 });
                let ch = cell.ch.unwrap_or(' ');
                if dumb {
                    out.push(ch);
                } else {
                    if cell.mode != current {
                        if current != 0 {
                            out.push_str("\x1b[m");
                        }
                        if cell.mode & UNDERLINE != 0 {
                            out.push_str("\x1b[4m");
                        }
                        if cell.mode & BOLD != 0 {
                            out.push_str("\x1b[1m");
                        }
                        current = cell.mode;
                    }
                    out.push(ch);
                }
            }
            if !dumb && current != 0 {
                out.push_str("\x1b[m");
            }
            out.push('\n');
            if indicated {
                let mut marks = String::new();
                let mut had = false;
                for k in 0..*max_column {
                    let mode = buf.get(k).map(|c| c.mode).unwrap_or(0);
                    if mode != 0 {
                        had = true;
                    }
                    marks.push(if mode & UNDERLINE != 0 && mode & BOLD != 0 {
                        'X'
                    } else if mode & UNDERLINE != 0 {
                        '_'
                    } else if mode & BOLD != 0 {
                        '!'
                    } else {
                        ' '
                    });
                }
                if had {
                    while marks.ends_with(' ') {
                        marks.pop();
                    }
                    out.push_str(&marks);
                    out.push('\n');
                }
            }
            buf.clear();
            *max_column = 0;
            *column = 0;
        };

    while i < chars.len() {
        let c = chars[i];
        i += 1;
        match c {
            '\x08' => {
                column = column.saturating_sub(1);
            }
            '\t' => {
                column = (column + 8) & !7;
            }
            '\r' => {
                column = 0;
            }
            '\x0e' | '\x0f' => {
                // SO/SI alternate charset: tracked by upstream for mode bits
                // but invisible in dumb/indicated output; ignore.
            }
            '\x1b' => {
                if i >= chars.len() {
                    return Err("unknown escape sequence in input".to_string());
                }
                let e = chars[i];
                i += 1;
                match e {
                    // Full/half reverse/forward: flush the pending line.
                    '7' | '8' | '9' => {
                        flush(&mut buf, &mut max_column, &mut out, &mut column);
                    }
                    _ => {
                        return Err("unknown escape sequence in input".to_string());
                    }
                }
            }
            '_' => {
                ensure(&mut buf, column, &mut max_column);
                if buf[column].ch.is_some() {
                    buf[column].mode |= UNDERLINE;
                    column += 1;
                } else {
                    buf[column] = Cell { ch: Some('_'), mode: 0 };
                    column += 1;
                }
            }
            ' ' => {
                ensure(&mut buf, column, &mut max_column);
                column += 1;
            }
            '\n' => {
                if max_column == 0 {
                    // Empty line: flush() would be a no-op, emit the newline.
                    out.push('\n');
                } else {
                    flush(&mut buf, &mut max_column, &mut out, &mut column);
                }
            }
            '\x0c' => {
                flush(&mut buf, &mut max_column, &mut out, &mut column);
                out.push('\x0c');
            }
            _ => {
                if c.is_control() {
                    continue;
                }
                ensure(&mut buf, column, &mut max_column);
                match buf[column].ch {
                    None => {
                        buf[column] = Cell { ch: Some(c), mode: 0 };
                    }
                    Some('_') => {
                        buf[column] = Cell { ch: Some(c), mode: UNDERLINE };
                    }
                    Some(prev) if prev == c => {
                        buf[column].mode |= BOLD;
                    }
                    Some(_) => {
                        // Overstruck differing characters: upstream keeps the
                        // old glyph and resets the mode; do the same.
                        buf[column].mode = 0;
                    }
                }
                column += 1;
            }
        }
    }
    flush(&mut buf, &mut max_column, &mut out, &mut column);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dumb(s: &str) -> String {
        ul_filter(s, true, false).unwrap()
    }

    #[test]
    fn underline_sequences_collapse_to_plain_in_dumb() {
        // _\bX and X\b_ both underline X; X\bX bolds X; dumb drops styling.
        assert_eq!(dumb("_\x08a b\x08b\n"), "a b\n");
        assert_eq!(dumb("a\x08_\n"), "a\n");
    }

    #[test]
    fn indicated_adds_marker_line() {
        let out = ul_filter("_\x08a\n", true, true).unwrap();
        assert_eq!(out, "a\n_\n");
        let out = ul_filter("b\x08b\n", true, true).unwrap();
        assert_eq!(out, "b\n!\n");
        // Plain lines get no marker line.
        assert_eq!(ul_filter("abc\n", true, true).unwrap(), "abc\n");
    }

    #[test]
    fn ansi_terminal_emits_standout_sequences() {
        let out = ul_filter("_\x08a\n", false, false).unwrap();
        assert_eq!(out, "\x1b[4ma\x1b[m\n");
        let out = ul_filter("b\x08b\n", false, false).unwrap();
        assert_eq!(out, "\x1b[1mb\x1b[m\n");
        assert_eq!(ul_filter("abc\n", false, false).unwrap(), "abc\n");
    }

    #[test]
    fn tab_cr_and_backspace_move_the_column() {
        assert_eq!(dumb("a\tb\n"), "a       b\n");
        // Overstriking a *different* character keeps the old glyph upstream
        // (only same-char overstrike bolds), so \r + X keeps "abc".
        assert_eq!(dumb("abc\rX\n"), "abc\n");
        // ...while \r + the same character bolds it (still "a" in dumb mode).
        assert_eq!(dumb("a\ra\n"), "a\n");
        // Same-character overstrike via backspace also bolds ("ab" stays "ab"
        // in dumb mode); a *different* character keeps the old glyph.
        assert_eq!(dumb("ab\x08b\n"), "ab\n");
        assert_eq!(dumb("ab\x08X\n"), "ab\n");
    }

    #[test]
    fn half_escapes_flush_the_line() {
        assert_eq!(dumb("ab\x1b7cd\n"), "ab\ncd\n");
    }

    #[test]
    fn unknown_escape_is_an_error() {
        assert!(ul_filter("a\x1bX", true, false).is_err());
    }

    #[test]
    fn version_line_matches_upstream_format() {
        assert_eq!(version_line(), "ul from util-linux 2.42\n");
    }
}
