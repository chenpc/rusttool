//! Counting logic of `wc(1)`, independent of the file plumbing.

/// Which counts to produce; with none selected `wc` prints all three.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    pub lines: bool,
    pub words: bool,
    pub bytes: bool,
    pub chars: bool,
    /// Print the total line even when a single file was given.
    pub total: bool,
}

impl Options {
    /// True when the user asked for nothing in particular.
    pub fn is_default(&self) -> bool {
        !self.lines && !self.words && !self.bytes && !self.chars
    }

    /// The columns to print, in GNU's order: lines, words, bytes.
    pub fn columns(&self) -> Vec<Kind> {
        if self.is_default() {
            vec![Kind::Lines, Kind::Words, Kind::Bytes]
        } else {
            let mut out = Vec::new();
            if self.lines {
                out.push(Kind::Lines);
            }
            if self.words {
                out.push(Kind::Words);
            }
            if self.bytes || self.chars {
                out.push(Kind::Bytes);
            }
            out
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Lines,
    Words,
    Bytes,
}

/// The counts for one input: lines end in a newline, words are separated by
/// blanks, and bytes are simply the length.
pub fn count(data: &[u8]) -> (u64, u64, u64) {
    let mut lines = 0u64;
    let mut words = 0u64;
    let mut in_word = false;
    for &byte in data {
        if byte == b'\n' {
            lines += 1;
        }
        let blank = matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r');
        if !blank && !in_word {
            in_word = true;
            words += 1;
        } else if blank {
            in_word = false;
        }
    }
    (lines, words, data.len() as u64)
}

/// Format one count with GNU's width rule: right-aligned in 7 columns, but
/// wider when the number needs it.
pub fn column_widths(counts: &[u64]) -> Vec<usize> {
    let widest = counts.iter().copied().max().unwrap_or(0);
    vec![widest.to_string().len().max(1) + 1]
}

/// One output line: the counts, then the label when there is one.
///
/// One width for every column, exactly like coreutils: the first column is
/// printed without a leading blank, the rest get one, and the file name is
/// appended after a space. `wc a b` with two regular files totalling 10 bytes
/// prints ` 1  2  4 a`.
pub fn format_line(counts: &[u64], width: usize, label: Option<&str>) -> String {
    let mut out = String::new();
    for (index, value) in counts.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{:>width$}", value, width = width.max(1)));
    }
    if let Some(name) = label {
        out.push(' ');
        out.push_str(name);
    }
    out
}

/// The size of one input, or `None` when it is not a regular file (a pipe or
/// standard input).
pub type InputSize = Option<u64>;

/// coreutils' `compute_number_width`: the width comes from the total *size* of
/// the regular inputs, and any non-regular input forces at least seven
/// columns. `stats_known` is false in the cases where coreutils skips the
/// stat entirely (no operands, or a single input with a single count), which
/// leaves the width at one.
pub fn number_width(sizes: &[InputSize], stats_known: bool) -> usize {
    let mut width = 1usize;
    if sizes.is_empty() || !stats_known {
        return width;
    }
    let mut minimum = 1usize;
    let mut regular_total: u64 = 0;
    for size in sizes {
        match size {
            Some(bytes) => regular_total = regular_total.saturating_add(*bytes),
            None => minimum = 7,
        }
    }
    let mut total = regular_total;
    while total >= 10 {
        width += 1;
        total /= 10;
    }
    width.max(minimum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_lines_words_bytes() {
        let (l, w, b) = count(b"one two\nthree\n");
        assert_eq!((l, w, b), (2, 3, 14));
    }

    #[test]
    fn runs_of_blanks_are_one_separator() {
        let (_, w, _) = count(b"a  \t b\n");
        assert_eq!(w, 2);
    }

    #[test]
    fn unterminated_last_line_is_not_counted() {
        assert_eq!(count(b"a\nb").0, 1);
    }

    #[test]
    fn default_columns() {
        assert_eq!(Options::default().columns(), vec![Kind::Lines, Kind::Words, Kind::Bytes]);
        assert_eq!(
            Options { words: true, ..Options::default() }.columns(),
            vec![Kind::Words]
        );
    }

    #[test]
    fn single_named_file_uses_width_one() {
        // 6 bytes in one regular file: below ten, so no padding at all.
        assert_eq!(number_width(&[Some(6)], true), 1);
        assert_eq!(format_line(&[3, 3, 6], 1, Some("file")), "3 3 6 file");
    }

    #[test]
    fn width_follows_the_total_size() {
        // Two files totalling ten bytes need two columns.
        assert_eq!(number_width(&[Some(4), Some(6)], true), 2);
        assert_eq!(format_line(&[1, 2, 4], 2, Some("/tmp/w1")), " 1  2  4 /tmp/w1");
        assert_eq!(format_line(&[4, 5, 10], 2, Some("total")), " 4  5 10 total");
    }

    #[test]
    fn non_regular_input_forces_seven_columns() {
        assert_eq!(number_width(&[None], true), 7);
        assert_eq!(
            format_line(&[2, 2, 4], 7, None),
            "      2       2       4"
        );
    }

    #[test]
    fn single_file_single_count_skips_the_stat() {
        // coreutils does not stat in this case, so the width stays one.
        assert_eq!(number_width(&[Some(20000)], false), 1);
        assert_eq!(format_line(&[20000], 1, Some("file")), "20000 file");
    }}
