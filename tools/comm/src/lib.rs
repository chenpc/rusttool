//! `comm(1)`: compare two sorted files line by line.
//!
//! The merge is the obvious one: whichever line sorts first is emitted, and a
//! line that appears in both is emitted in the third column. What the manual
//! does not mention is the order checking, and that is most of the code.
//!
//! From `comm.c`:
//!
//! * By default the input is *only* checked once something turned out to be
//!   unpairable. A file whose lines all matched a line in the other file is
//!   never complained about, however badly ordered it is. `--check-order`
//!   checks from the first pair; `--nocheck-order` never checks.
//! * A disorder is reported at most once per file. `--check-order` makes the
//!   first one fatal; by default it is a warning, and the run ends with a single
//!   `input is not in sorted order` and a non-zero status.
//! * When a file runs out, the pair of lines just before the end is checked
//!   again. That is not redundant: the check while reading could have been
//!   skipped because nothing had been unpairable *yet*, and this pass happens
//!   after it has.
//! * Comparison is `memcmp2`, not `strcmp`: the bytes are compared over the
//!   shorter length and a tie is settled by length, so `"b"` sorts before
//!   `"a b"`. coreutils only reaches for the locale collation order when the
//!   locale is a hard one, which the tests here are not.
//!
//! One empty `--output-delimiter=` writes a single NUL byte rather than
//! nothing, because the length is forced to 1 so that a NUL still counts as a
//! separator in the summary.

use std::cell::Cell;
use std::cmp::Ordering;
use std::rc::Rc;

/// Which of the three columns to print.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    /// Only in FILE1.
    OnlyFirst,
    /// Only in FILE2.
    OnlySecond,
    /// In both.
    Both,
}

/// How hard to look for input that is not in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderCheck {
    /// Only complain once an unpairable line proves the order matters.
    Default,
    /// Complain from the first pair, and stop at the first complaint.
    Enabled,
    /// Never complain.
    Disabled,
}

/// The parsed command line.
#[derive(Clone, Debug)]
pub struct Options {
    pub only_file_1: bool,
    pub only_file_2: bool,
    pub both: bool,
    pub order_check: OrderCheck,
    pub output_delimiter: Vec<u8>,
    pub total: bool,
    /// The byte that ends a line: `\n`, or NUL under `-z`.
    pub terminator: u8,
    pub files: Vec<String>,
}

impl Default for Options {
    /// The three columns, a tab between them and newlines.
    fn default() -> Self {
        Options {
            only_file_1: true,
            only_file_2: true,
            both: true,
            order_check: OrderCheck::Default,
            output_delimiter: b"\t".to_vec(),
            total: false,
            terminator: b'\n',
            files: Vec::new(),
        }
    }
}

impl Options {
    /// Write `line` in `column`, preceded by as many separators as there are
    /// earlier columns that are still being printed.
    pub fn write_line(&self, out: &mut Vec<u8>, line: &[u8], column: Column) {
        match column {
            Column::OnlyFirst => {
                if !self.only_file_1 {
                    return;
                }
            }
            Column::OnlySecond => {
                if !self.only_file_2 {
                    return;
                }
                if self.only_file_1 {
                    out.extend_from_slice(&self.separator());
                }
            }
            Column::Both => {
                if !self.both {
                    return;
                }
                if self.only_file_1 {
                    out.extend_from_slice(&self.separator());
                }
                if self.only_file_2 {
                    out.extend_from_slice(&self.separator());
                }
            }
        }
        out.extend_from_slice(line);
        out.push(self.terminator);
    }

    /// The separator as coreutils sees it.
    ///
    /// An empty `--output-delimiter=` still has length 1, so it writes the
    /// string's own terminator: a NUL.
    fn separator(&self) -> Vec<u8> {
        if self.output_delimiter.is_empty() {
            vec![0]
        } else {
            self.output_delimiter.clone()
        }
    }
}

/// `memcmp2`: compare the shared prefix, then settle a tie by length.
pub fn compare(left: &[u8], right: &[u8]) -> Ordering {
    let shared = left.len().min(right.len());
    match left[..shared].cmp(&right[..shared]) {
        Ordering::Equal => left.len().cmp(&right.len()),
        other => other,
    }
}

/// What the merge counted and complained about.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// Lines only in FILE1, only in FILE2, and in both.
    pub counts: [u64; 3],
    /// Whether either file was reported as out of order.
    pub disordered: bool,
    /// Which files were reported, in the order the merge ran into them.
    ///
    /// The order is the order of discovery, not the file order: the two files
    /// advance independently, so file 2 is often the first to fall out of order.
    pub reported: Vec<u8>,
}

/// A byte stream that several operands can walk in turn.
///
/// coreutils hands every `-` the same `FILE *`, so `comm - -` reads one stream
/// in sequence and the two columns take turns on it rather than the first
/// column taking all of it.
#[derive(Clone)]
pub struct Stream {
    bytes: Rc<Vec<u8>>,
    position: Rc<Cell<usize>>,
}

impl Stream {
    pub fn new(bytes: Vec<u8>) -> Stream {
        Stream {
            bytes: Rc::new(bytes),
            position: Rc::new(Cell::new(0)),
        }
    }

    /// An operand that reads this stream from wherever the last one stopped.
    pub fn reader(&self, terminator: u8, first: bool) -> Reader {
        Reader {
            stream: self.clone(),
            terminator,
            // current line, the one before it, and the one before that.
            history: [None, None, None],
            taken: 0,
            done: false,
            warned: false,
            first,
        }
    }
}

/// One operand of the merge, with the line history the order check needs.
pub struct Reader {
    stream: Stream,
    terminator: u8,
    history: [Option<Vec<u8>>; 3],
    /// How many lines have been handed over so far.
    taken: usize,
    /// Set once a read has run off the end, so `taken` stops moving.
    done: bool,
    /// Whether this file has already been complained about.
    warned: bool,
    /// Which of the two files this is, so the complaint names it.
    first: bool,
}

impl Reader {
    /// Claim the complaint for the first file, so it is named file 1.
    pub fn set_first(&mut self) {
        self.first = true;
    }

    /// The line the merge is looking at, or None once the file is spent.
    pub fn current(&self) -> Option<&[u8]> {
        self.history[0].as_deref()
    }

    /// Pull the next line, checking the order on the way.
    ///
    /// The read-time check looks at the line just handed over; when there is no
    /// next line the pair before it is looked at instead, which is the pass
    /// that catches a disorder masked while everything still paired up.
    ///
    /// Returns true when the run must stop, which only `--check-order` asks for.
    fn advance(&mut self, run: &mut Run) -> bool {
        let begin = self.stream.position.get();
        let mut scan = begin;
        let bytes = &self.stream.bytes;
        let line = if begin >= bytes.len() {
            None
        } else {
            while scan < bytes.len() && bytes[scan] != self.terminator {
                scan += 1;
            }
            let end = scan;
            if scan < bytes.len() {
                scan += 1;
            }
            self.stream.position.set(scan);
            Some(bytes[begin..end].to_vec())
        };
        self.history[2] = self.history[1].take();
        self.history[1] = self.history[0].take();
        match line {
            Some(line) => {
                if self.taken >= 1 {
                    let previous = self.history[1].clone();
                    if let Some(previous) = previous {
                        if run.check_order(self, &previous, &line) {
                            return true;
                        }
                    }
                }
                self.history[0] = Some(line);
                self.taken += 1;
                false
            }
            None => {
                self.done = true;
                self.history[0] = None;
                if self.taken >= 2 {
                    let (older, previous) = (self.history[2].clone(), self.history[1].clone());
                    if let (Some(older), Some(previous)) = (older, previous) {
                        if run.check_order(self, &older, &previous) {
                            return true;
                        }
                    }
                }
                false
            }
        }
    }
}

/// The state the merge and the order check share.
struct Run {
    order_check: OrderCheck,
    /// Set once any line could not be paired, which is what makes the default
    /// order check worth running at all.
    seen_unpairable: bool,
    summary: Summary,
}

impl Run {
    /// Compare `previous` with `current`, reporting a decrease at most once.
    ///
    /// Returns true when the caller has to stop, which `--check-order` makes it
    /// do on the very first complaint.
    fn check_order(&mut self, side: &mut Reader, previous: &[u8], current: &[u8]) -> bool {
        if self.order_check == OrderCheck::Disabled || side.warned {
            return false;
        }
        if self.order_check != OrderCheck::Enabled && !self.seen_unpairable {
            return false;
        }
        if compare(previous, current) != Ordering::Greater {
            return false;
        }
        self.summary.disordered = true;
        side.warned = true;
        self.summary.reported.push(if side.first { 1 } else { 2 });
        self.order_check == OrderCheck::Enabled
    }
}

/// Split `bytes` into lines, dropping the terminator.
pub fn split_lines(bytes: &[u8], terminator: u8) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for index in 0..bytes.len() {
        if bytes[index] == terminator {
            out.push(&bytes[start..index]);
            start = index + 1;
        }
    }
    if start < bytes.len() {
        out.push(&bytes[start..]);
    }
    out
}

/// Merge two sorted files and write the selected columns to `out`.
///
/// `left` and `right` are read in turn, so pointing both at one [`Stream`]
/// reproduces `comm - -`, where the two columns share standard input.
///
/// Returns the tally, and whether `--check-order` cut the run short.
pub fn merge(
    left: &mut Reader,
    right: &mut Reader,
    options: &Options,
    out: &mut Vec<u8>,
) -> (Summary, bool) {
    let mut run = Run {
        order_check: options.order_check,
        seen_unpairable: false,
        summary: Summary::default(),
    };
    // comm.c reads the first line of each file before the loop starts and does
    // not order check it.
    left.advance(&mut run);
    right.advance(&mut run);

    let mut stopped = false;
    while left.current().is_some() || right.current().is_some() {
        let order = match (left.current(), right.current()) {
            (Some(a), Some(b)) => compare(a, b),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => break,
        };
        let column = match order {
            Ordering::Equal => Column::Both,
            Ordering::Less => Column::OnlyFirst,
            Ordering::Greater => Column::OnlySecond,
        };
        let line = match column {
            Column::Both => right.current().unwrap(),
            Column::OnlyFirst => left.current().unwrap(),
            Column::OnlySecond => right.current().unwrap(),
        };
        run.summary.counts[column_index(column)] += 1;
        options.write_line(out, line, column);
        if order != Ordering::Equal {
            run.seen_unpairable = true;
        }

        // comm.c walks the two files in index order, which matters when both
        // operands share one stream: the first column is the one that gets the
        // earlier line.
        if order != Ordering::Greater && left.advance(&mut run) {
            stopped = true;
        }
        if stopped {
            break;
        }
        if order != Ordering::Less && right.advance(&mut run) {
            stopped = true;
        }
        if stopped {
            break;
        }
    }

    if !stopped && options.total {
        write_summary(out, &run.summary.counts, options);
    }
    (run.summary, stopped)
}

fn column_index(column: Column) -> usize {
    match column {
        Column::OnlyFirst => 0,
        Column::OnlySecond => 1,
        Column::Both => 2,
    }
}

/// The `--total` line, with the same delimiters as the columns.
pub fn write_summary(out: &mut Vec<u8>, counts: &[u64; 3], options: &Options) {
    let separator = options.separator();
    for count in counts {
        out.extend_from_slice(count.to_string().as_bytes());
        out.extend_from_slice(&separator);
    }
    out.extend_from_slice(b"total");
    out.push(options.terminator);
}

/// The diagnostics coreutils prints.
pub fn unrecognized_option_message(name: &str) -> String {
    format!("comm: unrecognized option '--{}'", name)
}

pub fn invalid_option_message(letter: char) -> String {
    format!("comm: invalid option -- '{}'", letter)
}

pub fn requires_argument_message(name: &str) -> String {
    format!("comm: option '--{}' requires an argument", name)
}

pub fn missing_operand_message() -> String {
    "comm: missing operand".to_string()
}

/// coreutils quotes an operand with `quote()`, which always wraps the value in
/// typographic quotes -- unlike a file name, which goes through `quotef`.
pub fn quoted_operand(name: &str) -> String {
    format!("\u{2018}{}\u{2019}", name)
}

pub fn missing_operand_after_message(name: &str) -> String {
    format!("comm: missing operand after {}", quoted_operand(name))
}

pub fn extra_operand_message(name: &str) -> String {
    format!("comm: extra operand {}", quoted_operand(name))
}

pub fn multiple_delimiters_message() -> String {
    "comm: multiple output delimiters specified".to_string()
}

pub fn cannot_open_message(name: &[u8], reason: &str) -> String {
    format!(
        "comm: {}: {}",
        String::from_utf8_lossy(&quotef(name)),
        reason
    )
}

pub fn unsorted_file_message(which: u8) -> String {
    format!("comm: file {} is not in sorted order", which)
}

pub fn unsorted_input_message() -> String {
    "comm: input is not in sorted order".to_string()
}

pub fn try_help_message() -> String {
    "Try 'comm --help' for more information.".to_string()
}

/// Whether a byte can go into a diagnostic unquoted.
fn is_plain(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"#%+,-./@]_{}~".contains(&byte)
}

/// Whether a byte can be printed as itself inside a quoted run.
fn is_printable(byte: u8) -> bool {
    (0x20..0x7f).contains(&byte)
}

/// A byte as it is written inside a `$'...'` chunk.
fn dollar_escape(byte: u8) -> String {
    match byte {
        0x07 => "\\a".to_string(),
        0x08 => "\\b".to_string(),
        b'\t' => "\\t".to_string(),
        b'\n' => "\\n".to_string(),
        0x0b => "\\v".to_string(),
        0x0c => "\\f".to_string(),
        b'\r' => "\\r".to_string(),
        other => format!("\\{:03o}", other),
    }
}

/// gnulib's `quotef`, the quoting coreutils uses for a file name.
///
/// A name whose bytes are all plain goes out untouched. Once any byte is not
/// plain the whole name is treated as quoted, so the string is cut into runs of
/// printable bytes and runs of bytes that are not. Each printable run is wrapped
/// in single quotes, or double quotes when it holds an apostrophe, and each run
/// of unprintable bytes becomes one `$'...'` chunk with octal escapes.
pub fn quotef(name: &[u8]) -> Vec<u8> {
    if name.is_empty() {
        return b"''".to_vec();
    }
    if name.iter().all(|byte| is_plain(*byte)) {
        return name.to_vec();
    }
    let mut out: Vec<u8> = Vec::with_capacity(name.len() + 8);
    let mut index = 0usize;
    while index < name.len() {
        let printable = is_printable(name[index]);
        let start = index;
        while index < name.len() && is_printable(name[index]) == printable {
            index += 1;
        }
        let run = &name[start..index];
        if printable {
            if run.contains(&b'\'') {
                out.push(b'"');
                out.extend_from_slice(run);
                out.push(b'"');
            } else {
                out.push(b'\'');
                out.extend_from_slice(run);
                out.push(b'\'');
            }
            continue;
        }
        if out.is_empty() {
            out.extend_from_slice(b"''");
        }
        out.extend_from_slice(b"$'");
        for byte in run {
            out.extend_from_slice(dollar_escape(*byte).as_bytes());
        }
        out.push(b'\'');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(left: &str, right: &str, options: &Options) -> (String, Summary) {
        let mut out = Vec::new();
        let mut first = Stream::new(left.as_bytes().to_vec()).reader(options.terminator, true);
        let mut second = Stream::new(right.as_bytes().to_vec()).reader(options.terminator, false);
        let (summary, _) = merge(&mut first, &mut second, options, &mut out);
        (String::from_utf8(out).unwrap(), summary)
    }

    #[test]
    fn defaults_print_three_columns() {
        let options = Options::default();
        assert!(options.only_file_1 && options.only_file_2 && options.both);
        assert_eq!(options.output_delimiter, b"\t");
        assert_eq!(options.terminator, b'\n');
        assert_eq!(options.order_check, OrderCheck::Default);
        assert!(!options.total);
    }

    #[test]
    fn compare_settles_a_prefix_by_length() {
        // memcmp2, so a prefix sorts first: this is what makes "b" come before
        // "a b" and what makes an empty line sort before everything.
        assert_eq!(compare(b"b", b"a b"), Ordering::Greater);
        assert_eq!(compare(b"a", b"a b"), Ordering::Less);
        assert_eq!(compare(b"a", b"a"), Ordering::Equal);
        assert_eq!(compare(b"", b"a"), Ordering::Less);
        assert_eq!(compare(b"a", b""), Ordering::Greater);
        assert_eq!(compare(b"", b""), Ordering::Equal);
    }

    #[test]
    fn the_three_columns_are_placed_by_which_file_had_the_line() {
        // A line in both files carries a separator for each earlier column that
        // is still being printed, so the third column gets two tabs.
        let (out, summary) = run("a\nb\nc\n", "b\nc\nd\n", &Options::default());
        assert_eq!(out, "a\n\t\tb\n\t\tc\n\td\n");
        assert_eq!(summary.counts, [1, 1, 2]);
        assert!(!summary.disordered);
    }

    #[test]
    fn identical_files_put_everything_in_the_third_column() {
        let (out, summary) = run("a\nb\nc\n", "a\nb\nc\n", &Options::default());
        assert_eq!(out, "\t\ta\n\t\tb\n\t\tc\n");
        assert_eq!(summary.counts, [0, 0, 3]);
    }

    #[test]
    fn repeated_lines_pair_one_for_one() {
        // "a" appears twice on the left and once on the right, so one pair and
        // one leftover; the leftover sorts before the pair.
        assert_eq!(run("a\na\nb\n", "a\n", &Options::default()).0, "\t\ta\na\nb\n");
    }

    #[test]
    fn a_suppressed_column_takes_its_separator_with_it() {
        let mut options = Options::default();
        options.only_file_1 = false;
        // The third column keeps one separator for the column that is gone.
        assert_eq!(run("a\nb\nc\n", "b\nc\nd\n", &options).0, "\tb\n\tc\nd\n");
        options = Options::default();
        options.only_file_2 = false;
        assert_eq!(run("a\nb\nc\n", "b\nc\nd\n", &options).0, "a\n\tb\n\tc\n");
        options = Options::default();
        options.both = false;
        assert_eq!(run("a\nb\nc\n", "b\nc\nd\n", &options).0, "a\n\td\n");
    }

    #[test]
    fn all_columns_off_prints_nothing() {
        let mut options = Options::default();
        options.only_file_1 = false;
        options.only_file_2 = false;
        options.both = false;
        assert_eq!(run("a\nb\nc\n", "b\nc\nd\n", &options).0, "");
    }

    #[test]
    fn the_delimiter_can_be_anything() {
        let mut options = Options::default();
        options.output_delimiter = b" | ".to_vec();
        assert_eq!(
            run("a\nb\nc\n", "b\nc\nd\n", &options).0,
            "a\n |  | b\n |  | c\n | d\n"
        );
    }

    #[test]
    fn an_empty_delimiter_writes_one_nul() {
        // The length is forced to 1 so that a NUL still separates, and the byte
        // written is the empty string's own terminator.
        let mut options = Options::default();
        options.output_delimiter = Vec::new();
        let out = run("a\nb\nc\n", "b\nc\nd\n", &options).0;
        assert_eq!(out.as_bytes(), b"a\n\0\0b\n\0\0c\n\0d\n");
    }

    #[test]
    fn the_summary_counts_every_column() {
        let mut options = Options::default();
        options.total = true;
        assert_eq!(
            run("a\nb\nc\n", "b\nc\nd\n", &options).0,
            "a\n\t\tb\n\t\tc\n\td\n1\t1\t2\ttotal\n"
        );
    }

    #[test]
    fn the_summary_ignores_the_columns_that_were_suppressed() {
        let mut options = Options::default();
        options.total = true;
        options.only_file_1 = false;
        assert_eq!(
            run("a\nb\nc\n", "b\nc\nd\n", &options).0,
            "\tb\n\tc\nd\n1\t1\t2\ttotal\n"
        );
    }

    #[test]
    fn a_nul_terminator_moves_the_line_boundary() {
        let mut options = Options::default();
        options.terminator = 0;
        assert_eq!(
            run("a\0b\0", "b\0c\0", &options).0,
            "a\0\t\tb\0\tc\0".to_string()
        );
        options.total = true;
        let out = run("a\0b\0", "b\0c\0", &options).0;
        assert_eq!(out.as_bytes(), b"a\0\t\tb\0\tc\0\x31\t\x31\t\x31\ttotal\0");
    }

    #[test]
    fn a_prefix_sorts_before_the_longer_line() {
        assert_eq!(run("a b\nb\n", "a\n", &Options::default()).0, "\ta\na b\nb\n");
    }

    #[test]
    fn an_empty_file_pads_the_other_one() {
        assert_eq!(run("a\nb\nc\n", "", &Options::default()).0, "a\nb\nc\n");
        assert_eq!(run("", "a\nb\nc\n", &Options::default()).0, "\ta\n\tb\n\tc\n");
        assert_eq!(run("", "", &Options::default()).0, "");
    }

    #[test]
    fn a_disorder_masked_while_everything_paired_up_is_caught_on_the_way_out() {
        // "b" after "a" is out of order, but when "a" was read the only pair
        // had matched, so nothing was unpairable yet and the check stayed quiet.
        // It is the pass at end of file, once "b" has been reported, that finds
        // it -- which is why this cannot be simplified away.
        let (out, summary) = run("c\nb\n", "c\n", &Options::default());
        assert_eq!(out, "\t\tc\nb\n");
        assert!(summary.disordered);
    }

    #[test]
    fn a_disorder_found_while_reading_is_reported() {
        let (out, summary) = run("b\na\n", "a\nb\nc\n", &Options::default());
        assert_eq!(out, "\ta\n\t\tb\na\n\tc\n");
        assert!(summary.disordered);
        assert_eq!(summary.counts, [1, 2, 1]);
    }

    #[test]
    fn nocheck_order_hides_a_disorder() {
        let mut options = Options::default();
        options.order_check = OrderCheck::Disabled;
        let (out, summary) = run("b\na\n", "a\nb\nc\n", &options);
        assert!(!summary.disordered);
        assert_eq!(out, "\ta\n\t\tb\na\n\tc\n");
    }

    #[test]
    fn check_order_stops_at_the_first_complaint_and_prints_no_summary() {
        let mut options = Options::default();
        options.order_check = OrderCheck::Enabled;
        options.total = true;
        let mut out = Vec::new();
        let mut first = Stream::new(b"b\na\n".to_vec()).reader(b'\n', true);
        let mut second = Stream::new(b"a\nb\nc\n".to_vec()).reader(b'\n', false);
        let (summary, stopped) = merge(&mut first, &mut second, &options, &mut out);
        assert!(stopped);
        assert!(summary.disordered);
        // "a" was only in the second file and the "b" pair matched; the run
        // stops before "a" on the left is ever read.
        assert_eq!(summary.counts, [0, 1, 1]);
        assert_eq!(String::from_utf8(out).unwrap(), "\ta\n\t\tb\n");
    }

    #[test]
    fn two_operands_can_walk_one_stream_in_turn() {
        // What `comm - -` does: coreutils passes the same FILE * twice, so the
        // columns alternate over one stream instead of the first taking all of
        // it.
        let stream = Stream::new(b"a\nb\nc\nd\ne\n".to_vec());
        let mut first = stream.reader(b'\n', true);
        let mut second = stream.reader(b'\n', false);
        let mut out = Vec::new();
        merge(&mut first, &mut second, &Options::default(), &mut out);
        assert_eq!(String::from_utf8(out).unwrap(), "a\n\tb\nc\n\td\ne\n");
    }

    #[test]
    fn which_files_were_reported_is_recorded_in_the_order_they_were_found() {
        let mut options = Options::default();
        // File 2 falls out of order first here: its second line "a-b" sorts
        // before the "aa" it follows, and that is noticed while file 1 is still
        // on its first line. So the report is 2 then 1, not 1 then 2.
        let mut out = Vec::new();
        let mut first = Stream::new(b"b\nd\naa\n".to_vec()).reader(b'\n', true);
        let mut second = Stream::new(b"aa\na-b\n".to_vec()).reader(b'\n', false);
        let (summary, _) = merge(&mut first, &mut second, &options, &mut out);
        assert_eq!(summary.reported, vec![2, 1]);

        // A file that never runs past its partner is never reported.
        let mut out = Vec::new();
        let mut first = Stream::new(b"a\nb\nc\n".to_vec()).reader(b'\n', true);
        let mut second = Stream::new(b"b\nc\nd\n".to_vec()).reader(b'\n', false);
        let (summary, _) = merge(&mut first, &mut second, &options, &mut out);
        assert!(summary.reported.is_empty());
    }

    #[test]
    fn only_the_second_file_can_be_the_disorder_one() {
        let mut options = Options::default();
        let mut out = Vec::new();
        let mut first = Stream::new(b"a\n".to_vec()).reader(b'\n', true);
        let mut second = Stream::new(b"b\na\n".to_vec()).reader(b'\n', false);
        let (summary, _) = merge(&mut first, &mut second, &options, &mut out);
        assert_eq!(summary.reported, vec![2]);
    }

    #[test]
    fn only_the_first_file_can_be_the_disorder_one() {
        let mut options = Options::default();
        let mut out = Vec::new();
        let mut first = Stream::new(b"b\na\n".to_vec()).reader(b'\n', true);
        let mut second = Stream::new(b"a\nb\nc\n".to_vec()).reader(b'\n', false);
        let (summary, _) = merge(&mut first, &mut second, &options, &mut out);
        assert_eq!(summary.reported, vec![1]);
    }

    #[test]
    fn a_default_run_still_prints_the_summary_when_disordered() {
        let mut options = Options::default();
        options.total = true;
        assert_eq!(
            run("b\na\n", "a\nb\nc\n", &options).0,
            "\ta\n\t\tb\na\n\tc\n1\t2\t1\ttotal\n"
        );
        assert_eq!(
            run("c\nb\n", "c\n", &options).0,
            "\t\tc\nb\n1\t0\t1\ttotal\n"
        );
    }

    #[test]
    fn an_empty_line_sorts_before_everything() {
        // "a" then "" is a decrease, because memcmp2 breaks the tie by length,
        // and that is what makes this input report a disorder.
        let (out, summary) = run("a\n\nb\n", "\na\n", &Options::default());
        assert_eq!(out, "\t\n\t\ta\n\nb\n");
        assert!(summary.disordered);
    }

    #[test]
    fn sorted_input_is_never_reported() {
        for (left, right) in [
            ("a\nb\nc\n", "b\nc\nd\n"),
            ("", "a\n"),
            ("a\n", ""),
            ("a\na\nb\n", "a\n"),
            ("a\nb\nc\n", "a\nb\nc\n"),
            ("a b\nb\n", "a\n"),
            ("a\n", "a b\nb\n"),
        ] {
            let (_, summary) = run(left, right, &Options::default());
            assert!(!summary.disordered, "{:?} vs {:?}", left, right);
        }
    }

    #[test]
    fn quotef_leaves_a_plain_name_alone() {
        assert_eq!(quotef(b"nosuchfile"), b"nosuchfile".to_vec());
        assert_eq!(quotef(b"/a/b-c_d.txt"), b"/a/b-c_d.txt".to_vec());
        assert_eq!(quotef(b""), b"''".to_vec());
        assert_eq!(quotef(b"no pe"), b"'no pe'".to_vec());
        assert_eq!(quotef(b"no'pe"), b"\"no'pe\"".to_vec());
        assert_eq!(quotef(b"a\tb"), b"'a'$'\\t''b'".to_vec());
        assert_eq!(quotef(b"\ta"), b"''$'\\t''a'".to_vec());
    }

    #[test]
    fn messages_match_coreutils() {
        assert_eq!(
            unrecognized_option_message("nonsense"),
            "comm: unrecognized option '--nonsense'"
        );
        assert_eq!(invalid_option_message('Q'), "comm: invalid option -- 'Q'");
        assert_eq!(
            requires_argument_message("output-delimiter"),
            "comm: option '--output-delimiter' requires an argument"
        );
        assert_eq!(missing_operand_message(), "comm: missing operand");
        assert_eq!(
            missing_operand_after_message("f"),
            "comm: missing operand after \u{2018}f\u{2019}"
        );
        assert_eq!(
            extra_operand_message("g"),
            "comm: extra operand \u{2018}g\u{2019}"
        );
        assert_eq!(
            multiple_delimiters_message(),
            "comm: multiple output delimiters specified"
        );
        assert_eq!(
            cannot_open_message(b"nope", "No such file or directory"),
            "comm: nope: No such file or directory"
        );
        assert_eq!(
            cannot_open_message(b"no pe", "Is a directory"),
            "comm: 'no pe': Is a directory"
        );
        assert_eq!(
            unsorted_file_message(1),
            "comm: file 1 is not in sorted order"
        );
        assert_eq!(
            unsorted_file_message(2),
            "comm: file 2 is not in sorted order"
        );
        assert_eq!(unsorted_input_message(), "comm: input is not in sorted order");
        assert_eq!(try_help_message(), "Try 'comm --help' for more information.");
    }
}