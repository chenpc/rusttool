//! `paste(1)`: merge lines of files.
//!
//! Two shapes, both from coreutils' `paste.c`:
//!
//! * Parallel (the default) joins line *n* of every file into one row. A file
//!   that runs out early still contributes its slot, so a short file leaves a
//!   hole rather than shifting the rest left.
//! * Serial (`-s`) joins all the lines of one file into one row, then moves to
//!   the next file.
//!
//! Three details the manual leaves out:
//!
//! * The delimiter list is used **cyclically and independently per row**: the
//!   cursor restarts at the front for every output row, and in parallel mode
//!   every file but the last consumes one. So `-d ab` over two files always
//!   uses `a`, and over three files the rows read `a b`, `a b`, `a b` again.
//! * `-d` understands backslash escapes (`\t`, `\n`, `\\`, `\0`, ...), and a
//!   `\0` means "this position gets no delimiter at all". An empty `-d ''` is
//!   rewritten to `\0` before the escapes are collapsed, so it also produces no
//!   delimiter rather than an error.
//! * Parallel mode gives up at once on a file it cannot open, while serial mode
//!   only complains and carries on with the next one.
//!
//! In serial mode an empty file still emits one line terminator, so the row
//! count follows the file count rather than the content. A file that could not
//! be opened contributes nothing at all, even in serial mode.

/// The delimiter byte that means "put nothing here".
///
/// It is a NUL because `-d '\0'` has to survive inside the list, and the list
/// is a plain byte string with no way to spell an escape twice.
pub const EMPTY_DELIM: u8 = 0;

use std::cell::RefCell;
use std::rc::Rc;

/// Which way the lines are merged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// `-s` absent: line *n* of every file becomes row *n*.
    Parallel,
    /// `-s`: every line of one file becomes one row.
    Serial,
}

/// The parsed command line.
#[derive(Clone, Debug)]
pub struct Options {
    /// The delimiter list after escapes are collapsed.
    pub delimiters: Vec<u8>,
    pub mode: Mode,
    /// The byte that ends a line: `\n`, or NUL under `-z`.
    pub terminator: u8,
    pub files: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            delimiters: vec![b'\t'],
            mode: Mode::Parallel,
            terminator: b'\n',
            files: Vec::new(),
        }
    }
}

impl Options {
    /// The delimiter for the `index`th gap of a row, wrapping around.
    ///
    /// An empty list can only happen if the caller built `Options` by hand;
    /// `from_argument` never produces one, because `-d ''` becomes `\0`.
    pub fn delimiter(&self, index: usize) -> u8 {
        if self.delimiters.is_empty() {
            return EMPTY_DELIM;
        }
        self.delimiters[index % self.delimiters.len()]
    }

    /// The `-d` value after coreutils' backslash collapsing.
    ///
    /// The empty string is rewritten to `\0` first, exactly as `paste.c` does,
    /// so `-d ''` means "no delimiter" rather than "a list with no escapes".
    /// A trailing lone backslash is dropped and reported, because the escape it
    /// starts has nothing to escape.
    pub fn from_argument(argument: &[u8]) -> Result<Vec<u8>, Vec<u8>> {
        let source: &[u8] = if argument.is_empty() { b"\\0" } else { argument };
        let mut out = Vec::with_capacity(source.len());
        let mut index = 0usize;
        while index < source.len() {
            let byte = source[index];
            if byte != b'\\' {
                out.push(byte);
                index += 1;
                continue;
            }
            index += 1;
            // A backslash at the very end has nothing to escape: remember it,
            // drop it and stop, leaving what came before intact.
            let Some(&escaped) = source.get(index) else {
                return Err(source.to_vec());
            };
            index += 1;
            match escaped {
                b'0' => out.push(EMPTY_DELIM),
                b'b' => out.push(0x08),
                b'f' => out.push(0x0c),
                b'n' => out.push(b'\n'),
                b'r' => out.push(b'\r'),
                b't' => out.push(b'\t'),
                b'v' => out.push(0x0b),
                b'\\' => out.push(b'\\'),
                // An unknown escape keeps the character and loses the backslash,
                // so "\q" is just "q".
                other => out.push(other),
            }
        }
        Ok(out)
    }
}

/// Where an input's bytes come from, and how far they have been read.
pub struct Cursor {
    bytes: Vec<u8>,
    position: usize,
}

impl Cursor {
    pub fn new(bytes: Vec<u8>) -> Cursor {
        Cursor { bytes, position: 0 }
    }
}

/// One input file, read a line at a time.
///
/// A `-` operand shares its cursor with every other `-`. coreutils hands them
/// all the *same* `FILE *`, so `paste - -` reads one stream in sequence and
/// fills the columns from it one after the other rather than handing the first
/// column everything.
pub struct Input {
    pub name: String,
    cursor: Rc<RefCell<Cursor>>,
    pub open: bool,
}

impl Input {
    pub fn new(name: String, bytes: Vec<u8>) -> Input {
        Input {
            name,
            cursor: Rc::new(RefCell::new(Cursor { bytes, position: 0 })),
            open: true,
        }
    }

    /// Another operand on the same stream.
    pub fn sharing(name: String, cursor: Rc<RefCell<Cursor>>) -> Input {
        Input {
            name,
            cursor,
            open: true,
        }
    }

    /// The next line, without its terminator, or None once the file is spent.
    ///
    /// A last line with no terminator comes back without one, which is how the
    /// callers tell a real line from a missing one. The line comes back owned
    /// because a shared cursor cannot lend out a slice.
    pub fn next_line(&mut self, terminator: u8) -> Option<Vec<u8>> {
        if !self.open {
            return None;
        }
        let mut cursor = self.cursor.borrow_mut();
        if cursor.position >= cursor.bytes.len() {
            return None;
        }
        let start = cursor.position;
        while cursor.position < cursor.bytes.len() && cursor.bytes[cursor.position] != terminator
        {
            cursor.position += 1;
        }
        let end = cursor.position;
        if cursor.position < cursor.bytes.len() {
            cursor.position += 1;
        }
        Some(cursor.bytes[start..end].to_vec())
    }

    /// Everything still unread.
    ///
    /// Serial mode wants the whole stream in one go, and going through the
    /// cursor is what makes a second `-` see an empty stream instead of a
    /// second copy of the first one's lines.
    pub fn drain(&mut self) -> Vec<u8> {
        let mut cursor = self.cursor.borrow_mut();
        let rest = cursor.bytes[cursor.position..].to_vec();
        cursor.position = cursor.bytes.len();
        rest
    }
}

/// Put `byte` in the gap, unless that gap is an `\0`.
pub fn put_delimiter(out: &mut Vec<u8>, byte: u8) {
    if byte != EMPTY_DELIM {
        out.push(byte);
    }
}

/// `-s`: every line of one file becomes one row.
pub fn serial(bytes: &[u8], options: &Options, out: &mut Vec<u8>) {
    // An empty file still owes a row, otherwise the output would have fewer
    // lines than there were files.
    let Some(&last) = bytes.last() else {
        out.push(options.terminator);
        return;
    };
    // The delimiter cursor restarts for every file, unlike the parallel case.
    let mut index = 0usize;
    for byte in &bytes[..bytes.len() - 1] {
        if *byte == options.terminator {
            put_delimiter(out, options.delimiter(index));
            index += 1;
        } else {
            out.push(*byte);
        }
    }
    // The final byte is content either way, and a file that did not end on the
    // terminator gets one so the row is closed.
    out.push(last);
    if last != options.terminator {
        out.push(options.terminator);
    }
}

/// The default: line *n* of every file becomes row *n*.
pub fn parallel(inputs: &mut [Input], options: &Options, out: &mut Vec<u8>) {
    let mut open = inputs.iter().filter(|input| input.open).count();
    while open > 0 {
        // Every row starts the delimiter list over, and the gaps of exhausted
        // files are held back until we know a later file has something to put
        // after them.
        let mut index = 0usize;
        let mut produced = false;
        let mut held: Vec<u8> = Vec::new();
        for position in 0..inputs.len() {
            if open == 0 {
                break;
            }
            let last = position + 1 == inputs.len();
            let line = inputs[position].next_line(options.terminator);
            match line {
                Some(bytes) => {
                    out.append(&mut held);
                    produced = true;
                    out.extend_from_slice(&bytes);
                    if last {
                        // The row always closes, even when the final line of
                        // the final file came without a terminator.
                        out.push(options.terminator);
                    } else {
                        put_delimiter(out, options.delimiter(index));
                        index += 1;
                    }
                }
                None => {
                    if inputs[position].open {
                        inputs[position].open = false;
                        open -= 1;
                    }
                    if last {
                        if produced {
                            out.append(&mut held);
                            out.push(options.terminator);
                        }
                    } else {
                        let byte = options.delimiter(index);
                        index += 1;
                        if byte != EMPTY_DELIM {
                            held.push(byte);
                        }
                    }
                }
            }
        }
    }
}

/// The diagnostics coreutils prints.
pub fn unrecognized_option_message(name: &str) -> String {
    format!("paste: unrecognized option '--{}'", name)
}

pub fn invalid_option_message(letter: char) -> String {
    format!("paste: invalid option -- '{}'", letter)
}

pub fn requires_argument_message(letter: char) -> String {
    format!("paste: option requires an argument -- '{}'", letter)
}

pub fn long_requires_argument_message(name: &str) -> String {
    format!("paste: option '--{}' requires an argument", name)
}

pub fn try_help_message() -> String {
    "Try 'paste --help' for more information.".to_string()
}

pub fn trailing_backslash_message(argument: &[u8]) -> String {
    format!(
        "paste: delimiter list ends with an unescaped backslash: {}",
        String::from_utf8_lossy(&quotearg(argument))
    )
}

pub fn cannot_open_message(name: &[u8], reason: &str) -> String {
    format!(
        "paste: {}: {}",
        String::from_utf8_lossy(&quotef(name)),
        reason
    )
}

/// Whether a byte can go into a diagnostic unquoted.
///
/// This is gnulib's shell-escape table read off the real binary: letters,
/// digits and a short list of punctuation pass through, and everything a shell
/// would chew on -- or that a reader would otherwise misread -- does not.
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
        // Octal with exactly three digits, which is what gnulib emits.
        other => format!("\\{:03o}", other),
    }
}

/// gnulib's `quotef`: the quoting coreutils uses for a file name.
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
            // Nothing inside single quotes needs escaping, so an apostrophe is
            // the only byte that forces the double-quoted form instead.
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
        // A chunk at the very front is preceded by empty quotes so it cannot be
        // read as more of the bare prefix.
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

/// `quotearg_n_style_colon`, the C-style quoting `paste.c` uses for the `-d`
/// value.
///
/// It exists so the trailing-backslash message shows the list once instead of
/// doubling every backslash the way the typographic quotes would. Quoting
/// kicks in for a double quote, for the colon that marks the quoting style, and
/// for anything unprintable -- but *not* for a plain space.
pub fn quotearg(text: &[u8]) -> Vec<u8> {
    let needs_quotes = text
        .iter()
        .any(|byte| *byte == b'"' || *byte == b':' || !is_printable(*byte));
    if !needs_quotes {
        return text.to_vec();
    }
    let mut out: Vec<u8> = Vec::with_capacity(text.len() + 8);
    out.push(b'"');
    for byte in text {
        match *byte {
            b'"' => out.extend_from_slice(b"\\\""),
            b'\\' => out.extend_from_slice(b"\\\\"),
            0x07 => out.extend_from_slice(b"\\a"),
            0x08 => out.extend_from_slice(b"\\b"),
            b'\t' => out.extend_from_slice(b"\\t"),
            b'\n' => out.extend_from_slice(b"\\n"),
            0x0b => out.extend_from_slice(b"\\v"),
            0x0c => out.extend_from_slice(b"\\f"),
            b'\r' => out.extend_from_slice(b"\\r"),
            other if is_printable(other) => out.push(other),
            other => out.extend_from_slice(format!("\\{:03o}", other).as_bytes()),
        }
    }
    out.push(b'"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<&[u8]> {
        let bytes = text.as_bytes();
        let mut out = Vec::new();
        let mut start = 0usize;
        for index in 0..bytes.len() {
            if bytes[index] == b'\n' {
                out.push(&bytes[start..index]);
                start = index + 1;
            }
        }
        if start < bytes.len() {
            out.push(&bytes[start..]);
        }
        out
    }

    fn run(texts: &[&str], options: &Options) -> String {
        let mut inputs: Vec<Input> = texts
            .iter()
            .enumerate()
            .map(|(index, text)| Input::new(format!("f{}", index), text.as_bytes().to_vec()))
            .collect();
        let mut out = Vec::new();
        match options.mode {
            Mode::Serial => {
                for input in inputs.iter_mut() {
                    let bytes = input.drain();
                    serial(&bytes, options, &mut out);
                }
            }
            Mode::Parallel => parallel(&mut inputs, options, &mut out),
        }
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn defaults_are_a_tab_and_newlines() {
        let options = Options::default();
        assert_eq!(options.delimiters, b"\t");
        assert_eq!(options.terminator, b'\n');
        assert_eq!(options.mode, Mode::Parallel);
    }

    #[test]
    fn parallel_joins_corresponding_lines() {
        assert_eq!(run(&["a\nb\nc\n", "x\ny\nz\n"], &Options::default()), "a\tx\nb\ty\nc\tz\n");
    }

    #[test]
    fn a_short_file_leaves_a_hole_instead_of_shifting() {
        assert_eq!(run(&["a\nb\nc\n", "x\n"], &Options::default()), "a\tx\nb\t\nc\t\n");
    }

    #[test]
    fn serial_puts_one_file_on_one_row() {
        let options = Options {
            mode: Mode::Serial,
            ..Options::default()
        };
        assert_eq!(run(&["a\nb\nc\n", "x\ny\nz\n"], &options), "a\tb\tc\nx\ty\tz\n");
    }

    #[test]
    fn a_serial_empty_file_still_closes_a_row() {
        let options = Options {
            mode: Mode::Serial,
            ..Options::default()
        };
        assert_eq!(run(&["a\nb\n", "", "x\n"], &options), "a\tb\n\nx\n");
    }

    #[test]
    fn the_delimiter_cursor_restarts_on_every_row() {
        let options = Options {
            delimiters: b"ab".to_vec(),
            ..Options::default()
        };
        assert_eq!(
            run(&["1\n2\n", "3\n4\n", "5\n6\n"], &options),
            "1a3b5\n2a4b6\n"
        );
    }

    #[test]
    fn a_serial_delimiter_cursor_restarts_on_every_file() {
        let options = Options {
            delimiters: b"ab".to_vec(),
            mode: Mode::Serial,
            ..Options::default()
        };
        assert_eq!(run(&["1\n2\n", "3\n4\n"], &options), "1a2\n3a4\n");
    }

    #[test]
    fn a_single_delimiter_is_used_everywhere() {
        let options = Options {
            delimiters: b"x".to_vec(),
            ..Options::default()
        };
        assert_eq!(run(&["a\nb\n", "1\n2\n", "9\n"], &options), "ax1x9\nbx2x\n");
    }

    #[test]
    fn an_empty_list_writes_nothing_between_fields() {
        let options = Options {
            delimiters: vec![EMPTY_DELIM],
            ..Options::default()
        };
        assert_eq!(run(&["a\nb\n", "1\n2\n"], &options), "a1\nb2\n");
    }

    #[test]
    fn escapes_are_collapsed_like_coreutils() {
        assert_eq!(Options::from_argument(b"xy").unwrap(), b"xy".to_vec());
        assert_eq!(Options::from_argument(b"\\t").unwrap(), b"\t".to_vec());
        assert_eq!(Options::from_argument(b"\\n\\r\\v\\b\\f").unwrap(), b"\n\r\x0b\x08\x0c".to_vec());
        assert_eq!(Options::from_argument(b"\\\\").unwrap(), b"\\".to_vec());
        assert_eq!(Options::from_argument(b"\\0").unwrap(), vec![EMPTY_DELIM]);
        // An unknown escape keeps the character and drops the backslash.
        assert_eq!(Options::from_argument(b"\\q").unwrap(), b"q".to_vec());
        // A backslash is not special unless it starts one of those.
        assert_eq!(Options::from_argument(b"a\\qb").unwrap(), b"aqb".to_vec());
    }

    #[test]
    fn an_empty_delimiter_argument_becomes_a_single_nul() {
        assert_eq!(Options::from_argument(b"").unwrap(), vec![EMPTY_DELIM]);
    }

    #[test]
    fn a_trailing_lone_backslash_is_reported_and_dropped() {
        assert_eq!(Options::from_argument(b"xy\\"), Err(b"xy\\".to_vec()));
        // An escaped backslash at the end is fine.
        assert_eq!(Options::from_argument(b"xy\\\\").unwrap(), b"xy\\".to_vec());
        // Three in a row: the first two pair up, the third is left over.
        assert_eq!(Options::from_argument(b"xy\\\\\\"), Err(b"xy\\\\\\".to_vec()));
        // A backslash is left alone when nothing follows it at all.
        assert_eq!(Options::from_argument(b"\\"), Err(b"\\".to_vec()));
    }

    #[test]
    fn a_line_without_a_terminator_is_still_a_line() {
        assert_eq!(lines("a\nb"), vec![&b"a"[..], &b"b"[..]]);
        assert_eq!(lines("a\n"), vec![&b"a"[..]]);
        assert_eq!(lines(""), Vec::<&[u8]>::new());
    }

    #[test]
    fn parallel_adds_the_missing_terminator_of_the_last_file() {
        assert_eq!(run(&["a\nb", "1"], &Options::default()), "a\t1\nb\t\n");
    }

    #[test]
    fn serial_adds_the_missing_terminator_too() {
        let options = Options {
            mode: Mode::Serial,
            ..Options::default()
        };
        assert_eq!(run(&["a\nb", "1"], &options), "a\tb\n1\n");
    }

    #[test]
    fn nul_termination_moves_the_line_boundary() {
        let options = Options {
            terminator: 0,
            ..Options::default()
        };
        assert_eq!(run(&["a\0b\0"], &options), "a\0b\0");
        assert_eq!(run(&["a\0b"], &options), "a\0b\0");
        let serial = Options {
            terminator: 0,
            mode: Mode::Serial,
            ..Options::default()
        };
        assert_eq!(run(&["a\0b\0"], &serial), "a\tb\0");
    }

    #[test]
    fn two_dashes_take_turns_on_one_stream() {
        // coreutils passes the same FILE * twice, so the columns read one
        // stream in sequence instead of the first one taking all of it.
        let cursor = Rc::new(RefCell::new(Cursor::new(b"a\nb\nc\n".to_vec())));
        let left = Input::sharing("-".to_string(), Rc::clone(&cursor));
        let right = Input::sharing("-".to_string(), cursor);
        let options = Options::default();
        let mut inputs = vec![left, right];
        let mut out = Vec::new();
        parallel(&mut inputs, &options, &mut out);
        assert_eq!(String::from_utf8(out).unwrap(), "a\tb\nc\t\n");
    }

    #[test]
    fn a_second_serial_dash_sees_an_empty_stream() {
        let cursor = Rc::new(RefCell::new(Cursor::new(b"a\nb\n".to_vec())));
        let mut first = Input::sharing("-".to_string(), Rc::clone(&cursor));
        let mut second = Input::sharing("-".to_string(), cursor);
        let options = Options {
            mode: Mode::Serial,
            ..Options::default()
        };
        let mut out = Vec::new();
        serial(&first.drain(), &options, &mut out);
        serial(&second.drain(), &options, &mut out);
        assert_eq!(String::from_utf8(out).unwrap(), "a\tb\n\n");
    }

    #[test]
    fn quotef_leaves_a_plain_name_alone() {
        assert_eq!(quotef(b"nosuchfile"), b"nosuchfile".to_vec());
        assert_eq!(quotef(b"/a/b-c_d.txt"), b"/a/b-c_d.txt".to_vec());
        // Letters, digits and this punctuation pass through unquoted.
        assert_eq!(quotef(b"a#b%c+d,e-f.g/h@i]j_k{l}m~n"), b"a#b%c+d,e-f.g/h@i]j_k{l}m~n".to_vec());
        assert_eq!(quotef(b""), b"''".to_vec());
    }

    #[test]
    fn quotef_wraps_printable_specials_in_single_quotes() {
        assert_eq!(quotef(b"no pe"), b"'no pe'".to_vec());
        assert_eq!(quotef(b"no=pe"), b"'no=pe'".to_vec());
        assert_eq!(quotef(b"no\"pe"), b"'no\"pe'".to_vec());
        assert_eq!(quotef(b"no\\pe"), b"'no\\pe'".to_vec());
        // Nothing inside single quotes has to be escaped, so a double quote is
        // fine there; an apostrophe is not, so the run switches to double quotes.
        assert_eq!(quotef(b"no'pe"), b"\"no'pe\"".to_vec());
        assert_eq!(quotef(b"a b'c"), b"\"a b'c\"".to_vec());
    }

    #[test]
    fn quotef_uses_a_dollar_chunk_for_unprintable_bytes() {
        assert_eq!(quotef(b"a\tb"), b"'a'$'\\t''b'".to_vec());
        assert_eq!(quotef(b"a\t\tb"), b"'a'$'\\t\\t''b'".to_vec());
        assert_eq!(quotef(b"a\nb\nc"), b"'a'$'\\n''b'$'\\n''c'".to_vec());
        assert_eq!(quotef(b"a\x07b"), b"'a'$'\\a''b'".to_vec());
        assert_eq!(quotef(b"a\xffb"), b"'a'$'\\377''b'".to_vec());
        assert_eq!(quotef(b"a\x7fb"), b"'a'$'\\177''b'".to_vec());
        // A chunk at the very front is preceded by empty quotes, so it cannot
        // be read as more of the bare prefix.
        assert_eq!(quotef(b"\ta"), b"''$'\\t''a'".to_vec());
        assert_eq!(quotef(b"\t\t"), b"''$'\\t\\t'".to_vec());
    }

    #[test]
    fn quotearg_only_quotes_when_it_has_to() {
        // A space alone is not enough; the colon that marks the style and a
        // double quote are.
        assert_eq!(quotearg(b"xy"), b"xy".to_vec());
        assert_eq!(quotearg(b"a b"), b"a b".to_vec());
        assert_eq!(quotearg(b"a:b"), b"\"a:b\"".to_vec());
        assert_eq!(quotearg(b"a\"b"), b"\"a\\\"b\"".to_vec());
        assert_eq!(quotearg(b"a\\b"), b"a\\b".to_vec());
        assert_eq!(quotearg(b"a\tb"), b"\"a\\tb\"".to_vec());
        assert_eq!(quotearg(b"a\x01b"), b"\"a\\001b\"".to_vec());
        assert_eq!(quotearg(b"a\xffb"), b"\"a\\377b\"".to_vec());
        assert_eq!(quotearg(b"a'b"), b"a'b".to_vec());
        // A backslash is printable and is not the quoting char, so it needs no
        // quoting of its own; only the escape inside matters.
        assert_eq!(quotearg(b"a\\"), b"a\\".to_vec());
        assert_eq!(quotearg(b"a\\b"), b"a\\b".to_vec());
        assert_eq!(quotearg(b"a:\\"), b"\"a:\\\\\"".to_vec());
    }

    #[test]
    fn messages_match_coreutils() {
        assert_eq!(
            unrecognized_option_message("nonsense"),
            "paste: unrecognized option '--nonsense'"
        );
        assert_eq!(invalid_option_message('Q'), "paste: invalid option -- 'Q'");
        assert_eq!(
            requires_argument_message('d'),
            "paste: option requires an argument -- 'd'"
        );
        assert_eq!(
            long_requires_argument_message("delimiters"),
            "paste: option '--delimiters' requires an argument"
        );
        assert_eq!(
            trailing_backslash_message(b"xy\\"),
            "paste: delimiter list ends with an unescaped backslash: xy\\"
        );
        assert_eq!(
            trailing_backslash_message(b"a\"b\\"),
            "paste: delimiter list ends with an unescaped backslash: \"a\\\"b\\\\\""
        );
        assert_eq!(
            trailing_backslash_message(b"a b\\"),
            "paste: delimiter list ends with an unescaped backslash: a b\\"
        );
        assert_eq!(
            cannot_open_message(b"nope", "No such file or directory"),
            "paste: nope: No such file or directory"
        );
        assert_eq!(
            cannot_open_message(b"no pe", "No such file or directory"),
            "paste: 'no pe': No such file or directory"
        );
        assert_eq!(try_help_message(), "Try 'paste --help' for more information.");
    }
}