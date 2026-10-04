//! Pure formatting core of `cat(1)`: how a byte stream is written out under
//! the option set, independent of where the bytes come from.

/// The formatting options `cat` accepts, in the order they are documented.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    /// -n: number every output line.
    pub number: bool,
    /// -b: number only non-empty lines.
    pub number_nonblank: bool,
    /// -s: squeeze repeated blank lines into one.
    pub squeeze_blank: bool,
    /// -t/-T/-v/-e/-E: render non-printing characters.
    pub show_tabs: bool,
    pub show_tabs_line_end: bool,
    pub show_nonprinting: bool,
    pub show_line_end: bool,
    /// -A is the usual shorthand for -vET.
    pub show_all: bool,
    /// -u: unbuffer (ignored for byte streams, kept for flag compatibility).
    pub unbuffered: bool,
}

impl Options {
    /// Apply the shorthands after parsing: -A implies -vET, -e implies -vE and
    /// -t, -E is -v, -T is -t.
    pub fn normalize(&mut self) {
        if self.show_all {
            self.show_nonprinting = true;
            self.show_tabs = true;
            self.show_line_end = true;
        }
        if self.show_line_end {
            self.show_nonprinting = true;
        }
        if self.show_tabs {
            self.show_nonprinting = true;
        }
    }

    fn numbering(&self) -> bool {
        self.number || self.number_nonblank
    }

    fn renders(&self) -> bool {
        self.show_nonprinting || self.show_tabs || self.show_line_end
    }
}

/// util-linux `cat` prints "      1\t" for the first line.
fn line_prefix(number: usize) -> String {
    format!("{:>6}\t", number)
}

/// Render one byte for the non-printing modes, following coreutils:
///
/// * printable ASCII (space included) passes through;
/// * a tab becomes `^I` whenever -v (or -t/-A) is in effect;
/// * a newline becomes `$` **followed by the newline** with -e/-E;
/// * other controls become `^X`, with `M-^X` for the high half.
fn render(byte: u8, tabs: bool, line_end: bool) -> String {
    match byte {
        b'\t' => {
            if tabs {
                "^I".to_string()
            } else {
                "\t".to_string()
            }
        }
        b'\n' => {
            if line_end {
                "$\n".to_string()
            } else {
                "\n".to_string()
            }
        }
        0x20..=0x7e => (byte as char).to_string(),
        0x00..=0x1f | 0x7f => format!("^{}", (byte ^ 0x40) as char),
        _ => {
            let masked = byte & 0x7f;
            if matches!(masked, 0x00..=0x1f | 0x7f) {
                format!("M-^{}", (masked ^ 0x40) as char)
            } else {
                format!("M-{}", masked as char)
            }
        }
    }
}

/// Write `input` to `out` under `options`, returning the number of lines
/// written.
pub fn run(input: &[u8], options: &Options, out: &mut Vec<u8>) -> usize {
    let mut lines = 0usize;
    let mut at_line_start = true;
    let mut last_was_blank = false;
    let mut number = 0usize;

    for &byte in input {
        let is_newline = byte == b'\n';
        let blank_line = at_line_start && is_newline;

        // A run of blank lines is squeezed down to one.
        if options.squeeze_blank && blank_line && last_was_blank {
            at_line_start = true;
            continue;
        }

        if at_line_start {
            if options.numbering() && !(options.number_nonblank && blank_line) {
                number += 1;
                out.extend_from_slice(line_prefix(number).as_bytes());
            }
            lines += 1;
            last_was_blank = blank_line;
        }

        if options.renders() {
            out.extend_from_slice(
                render(byte, options.show_tabs, options.show_line_end).as_bytes(),
            );
        } else {
            out.push(byte);
        }
        at_line_start = is_newline;
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat(input: &str, options: Options) -> String {
        let mut options = options;
        options.normalize();
        let mut out = Vec::new();
        run(input.as_bytes(), &options, &mut out);
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn plain_passthrough() {
        assert_eq!(cat("a\nb\n", Options::default()), "a\nb\n");
    }

    #[test]
    fn numbers_every_line() {
        assert_eq!(
            cat("a\nb\n", Options { number: true, ..Options::default() }),
            "     1\ta\n     2\tb\n"
        );
    }

    #[test]
    fn number_nonblank_skips_empty_lines() {
        let out = cat(
            "a\n\nb\n",
            Options { number_nonblank: true, ..Options::default() },
        );
        assert_eq!(out, "     1\ta\n\n     2\tb\n");
    }

    #[test]
    fn squeeze_collapses_repeats_only() {
        assert_eq!(
            cat("a\n\n\n\nb\n", Options { squeeze_blank: true, ..Options::default() }),
            "a\n\nb\n"
        );
    }

    #[test]
    fn show_tabs_and_line_ends() {
        // -e keeps the newline after the dollar sign, like coreutils.
        let out = cat("a\tb\n", Options { show_tabs: true, show_line_end: true, ..Options::default() });
        assert_eq!(out, "a^Ib$\n");
    }

    #[test]
    fn show_all_is_vet() {
        let out = cat("a\tb\n\x01\n", Options { show_all: true, ..Options::default() });
        assert_eq!(out, "a^Ib$\n^A$\n");
    }

    #[test]
    fn high_bytes_use_meta_notation() {
        // Measured against coreutils: 0x80 -> M-^@, 0xa0 -> M- , 0xe1 -> M-a,
        // 0xff -> M-^?, 0x7f -> ^?
        let options = Options { show_nonprinting: true, ..Options::default() };
        let mut out = Vec::new();
        run(&[0x80, 0xa0, 0xe1, 0xff, 0x7f, 0x01], &options, &mut out);
        assert_eq!(String::from_utf8(out).unwrap(), "M-^@M- M-aM-^?^?^A");
    }

    #[test]
    fn space_is_printable() {
        let out = cat("a b", Options { show_nonprinting: true, ..Options::default() });
        assert_eq!(out, "a b");
    }

    #[test]
    fn counts_unterminated_last_line() {
        let mut out = Vec::new();
        // "a\nb" is two lines: the trailing partial line counts as one.
        assert_eq!(run(b"a\nb", &Options::default(), &mut out), 2);
    }
}
