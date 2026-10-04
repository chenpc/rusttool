//! Pure core of the `line(1)` clone (util-linux `text-utils/line.c`).
//!
//! Upstream reads one line from stdin with `getwchar()` and echoes it: every
//! character up to (but not including) the first `\n` is printed, then a
//! final `\n` is always printed. Exit status is success only when the loop
//! ends at a newline; EOF (even after partial input, even on empty input)
//! exits failure.
//!
//! Deviation: input is processed as bytes; a `\n` byte ends the line. For
//! UTF-8 input this matches the wide-character behaviour on line boundaries.

/// The util-linux release whose observable behaviour this clone tracks.
pub const UTIL_LINUX_VERSION: &str = "2.42";

/// `line -h` output.
pub const HELP: &str = concat!(
    "\nUsage:\n",
    " line [options]\n",
    "\n",
    "Read one line.\n",
    "\n",
    "Options:\n",
    " -h, --help     display this help and exit\n",
    " -V, --version  output version information and exit\n",
);

/// The exact line `line -V` prints.
pub fn version_line() -> String {
    format!("line from util-linux {}\n", UTIL_LINUX_VERSION)
}

/// Copy exactly one line from `input` to `out`.
///
/// Returns `true` when the line ended at a newline (exit 0), `false` on EOF
/// (exit 1). A trailing `\n` is always emitted, even for empty/EOF input.
pub fn run_line(input: &[u8]) -> (Vec<u8>, bool) {
    match input.iter().position(|&b| b == b'\n') {
        Some(pos) => {
            let mut out = Vec::with_capacity(pos + 1);
            out.extend_from_slice(&input[..pos]);
            out.push(b'\n');
            (out, true)
        }
        None => {
            let mut out = Vec::with_capacity(input.len() + 1);
            out.extend_from_slice(input);
            out.push(b'\n');
            (out, false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_at_first_newline() {
        assert_eq!(run_line(b"hello\nrest\n"), (b"hello\n".to_vec(), true));
    }

    #[test]
    fn eof_without_newline_still_prints_and_fails() {
        assert_eq!(run_line(b"partial"), (b"partial\n".to_vec(), false));
    }

    #[test]
    fn empty_input_prints_newline_and_fails() {
        assert_eq!(run_line(b""), (b"\n".to_vec(), false));
    }

    #[test]
    fn empty_first_line_succeeds() {
        assert_eq!(run_line(b"\nrest"), (b"\n".to_vec(), true));
    }

    #[test]
    fn version_line_matches_upstream_format() {
        assert_eq!(version_line(), "line from util-linux 2.42\n");
    }
}
