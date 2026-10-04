//! Pure, I/O-free core of the `rev(1)` clone (util-linux `text-utils/rev.c`).
//!
//! Upstream semantics this mirrors:
//!
//! * input is a byte stream, not text: it is split into "lines" terminated by a
//!   separator (`\n` by default, NUL with `-0`),
//! * the separator itself is **not** part of the reversed run and is emitted
//!   again unchanged, so `rev` is length- and terminator-preserving,
//! * reversal is by *character*, not by byte: upstream decodes into `wchar_t`
//!   via the locale, so a UTF-8 locale reverses whole code points. We do the
//!   same and additionally treat invalid UTF-8 bytes as single characters,
//!   which is what the C locale does,
//! * a final line without a trailing separator is still reversed and emitted
//!   without one (the 1994 patch noted in the upstream history),
//! * an empty read at EOF produces no output at all.

/// The util-linux release whose observable behaviour this clone tracks.
///
/// Only used for `-V`; it is the version string, not a dependency.
pub const UTIL_LINUX_VERSION: &str = "2.42";

/// `rev -h` output, shaped like util-linux' `USAGE_HEADER`/`USAGE_OPTIONS`
/// blocks — including the leading blank line those macros emit.
pub const HELP: &str = concat!(
    "\nUsage:\n",
    " rev [options] [<file> ...]\n",
    "\n",
    "Reverse lines characterwise.\n",
    "\n",
    "Options:\n",
    " -0, --zero     use the NUL byte as line separator\n",
    " -h, --help     display this help and exit\n",
    " -V, --version  output version information and exit\n",
    "\n",
    "See also:\n",
    "   tac(1)\n",
);

/// The exact line `rev -V` prints (`print_version()` in upstream `c.c`).
pub fn version_line() -> String {
    format!("rev from util-linux {}\n", UTIL_LINUX_VERSION)
}

/// Line separator, i.e. the value `rev` reverses up to (upstream's `sep`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Separator {
    /// Default: reverse up to and including `\n`.
    Newline,
    /// `-0`/`--zero`: reverse up to a NUL byte.
    Zero,
}

impl Separator {
    /// The separator byte to scan for.
    pub fn byte(self) -> u8 {
        match self {
            Separator::Newline => b'\n',
            Separator::Zero => 0,
        }
    }
}

/// Length in bytes of the UTF-8 sequence starting at `start`.
///
/// Returns 1 for anything that is not a well-formed sequence, which is what
/// the C locale does: there every byte is its own character. Overlong forms
/// and surrogates are not rejected, only the shape of the sequence is checked —
/// enough to never split a real code point in half.
fn char_len(bytes: &[u8], start: usize) -> usize {
    let len = match bytes[start] {
        0x00..=0x7f => 1,
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        // A continuation byte or an invalid lead byte: not a sequence start.
        _ => return 1,
    };
    let tail = &bytes[start + 1..];
    if tail.len() >= len - 1 && tail[..len - 1].iter().all(|byte| byte & 0xc0 == 0x80) {
        len
    } else {
        1
    }
}

/// Reverse `bytes` by character (UTF-8 code point), never splitting a
/// multi-byte sequence and never rejecting invalid bytes: each byte that does
/// not belong to a valid sequence is reversed as a single character.
pub fn reverse_chars(bytes: &[u8]) -> Vec<u8> {
    let mut units: Vec<(usize, usize)> = Vec::with_capacity(bytes.len());
    let mut start = 0;
    while start < bytes.len() {
        let end = start + char_len(bytes, start);
        units.push((start, end));
        start = end;
    }
    let mut out = Vec::with_capacity(bytes.len());
    for &(start, end) in units.iter().rev() {
        out.extend_from_slice(&bytes[start..end]);
    }
    out
}

/// Reverse every line of `input`, leaving separators where they were.
///
/// Equivalent to calling [`reverse_chars`] on each line, so this is the
/// reference the streaming implementation in `main.rs` must agree with.
pub fn rev(input: &[u8], sep: Separator) -> Vec<u8> {
    let sep = sep.byte();
    let mut out = Vec::with_capacity(input.len());
    let mut start = 0;
    while start < input.len() {
        match input[start..].iter().position(|&byte| byte == sep) {
            Some(offset) => {
                out.extend_from_slice(&reverse_chars(&input[start..start + offset]));
                out.push(sep);
                start += offset + 1;
            }
            None => {
                out.extend_from_slice(&reverse_chars(&input[start..]));
                start = input.len();
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(input: &str) -> String {
        String::from_utf8(rev(input.as_bytes(), Separator::Newline)).expect("valid UTF-8")
    }

    #[test]
    fn reverse_chars_is_an_involution() {
        // Reversing twice returns the original.
        for sample in ["", "a", "ab", "abc", "你好", "aéb", "\u{1f600}"] {
            let once = reverse_chars(sample.as_bytes());
            let twice = reverse_chars(&once);
            assert_eq!(
                String::from_utf8(twice).unwrap(),
                sample,
                "double reverse of {:?}",
                sample
            );
        }
    }

    #[test]
    fn reverse_chars_keeps_utf8_sequences_whole() {
        // "\u{4f60}\u{597d} world" -> "dlrow \u{597d}\u{4f60}"
        assert_eq!(reverse_chars("你好 world".as_bytes()), "dlrow 好你".as_bytes());
        // 4-byte code point
        assert_eq!(reverse_chars("a\u{1f600}b".as_bytes()), "b\u{1f600}a".as_bytes());
    }

    #[test]
    fn reverse_chars_treats_invalid_bytes_singly() {
        // Lone continuation bytes are not part of a sequence: moved as-is.
        assert_eq!(reverse_chars(&[b'a', 0x80, 0x80, b'b']), vec![b'b', 0x80, 0x80, b'a']);
    }

    #[test]
    fn rev_reverses_each_line() {
        assert_eq!(r("hello world\nabc\n"), "dlrow olleh\ncba\n");
    }

    #[test]
    fn rev_of_empty_input_is_empty() {
        // Upstream: the first read_line() returns 0 and is skipped.
        assert_eq!(rev(b"", Separator::Newline), b"");
    }

    #[test]
    fn rev_keeps_blank_lines_blank() {
        assert_eq!(r("\n"), "\n");
        assert_eq!(r("a\n\nb\n"), "a\n\nb\n");
        assert_eq!(r("\n\n\n"), "\n\n\n");
        // A line of one character is its own reverse, but must still be emitted.
        assert_eq!(r("x\ny\n"), "x\ny\n");
    }

    #[test]
    fn rev_preserves_a_missing_trailing_newline() {
        assert_eq!(r("abc"), "cba");
        assert_eq!(r("abc\nno newline"), "cba\nenilwen on");
        assert_eq!(r("abc\n\nxyz"), "cba\n\nzyx");
    }

    #[test]
    fn rev_preserves_carriage_returns_as_data() {
        // CR is an ordinary character for rev: only \n is a separator, so
        // "abc\r" reverses to "\rcba".
        assert_eq!(r("abc\r\n"), "\rcba\n");
    }

    #[test]
    fn rev_is_length_preserving() {
        for sample in ["", "\n", "abc", "a\nbb\nccc", "\u{4f60}\u{597d}\n"] {
            assert_eq!(
                rev(sample.as_bytes(), Separator::Newline).len(),
                sample.len(),
                "length changed for {:?}",
                sample
            );
        }
    }

    #[test]
    fn rev_with_nul_separator() {
        let zero = |s: &str| String::from_utf8(rev(s.as_bytes(), Separator::Zero)).unwrap();
        assert_eq!(zero("a\0b\0"), "a\0b\0");
        assert_eq!(zero("ab\0cd"), "ba\0dc");
        // With -0 a newline is just data.
        assert_eq!(zero("ab\ncd\0"), "dc\nba\0");
    }

    #[test]
    fn rev_handles_embedded_nul_bytes_in_binary() {
        // Without -0 a NUL is ordinary data, so it is reversed like any byte.
        assert_eq!(rev(b"a\0b", Separator::Newline), vec![b'b', 0, b'a']);
    }

    #[test]
    fn version_line_matches_upstream_format() {
        assert_eq!(version_line(), "rev from util-linux 2.42\n");
        assert!(version_line().starts_with("rev from util-linux "));
    }

    #[test]
    fn help_mentions_every_supported_option() {
        for needle in ["-0, --zero", "-h, --help", "-V, --version", "rev [options]"] {
            assert!(HELP.contains(needle), "help is missing {:?}", needle);
        }
    }
}
