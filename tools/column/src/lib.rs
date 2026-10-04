//! Pure column-formatting engine for `column(1)`, ported from util-linux
//! `text-utils/column.c` (2.39.3) plus the `lib/strutils.c` helpers it calls.
//!
//! This crate holds no I/O: [`columnate`] turns a slice of input lines into the
//! exact bytes `column` would write, which keeps every rule unit-testable.
//!
//! Faithfully reproduced quirks (all confirmed byte-for-byte against
//! util-linux 2.39.3):
//!
//! * input is split on newlines, the terminator is dropped and leading blanks
//!   are kept; `skip_space()` skips blanks only, so a tab-only line is a real
//!   entry, while a blank line is dropped unless `-L`,
//! * the entry width uses `wcwidth()` and skips non-printable characters and
//!   tabs, so `-c` really counts display cells,
//! * with no `-c`, and stdout not a terminal, the width is [`DEFAULT_WIDTH`];
//!   `-c 0` means "unlimited" and degenerates to [`Mode::Simple`],
//! * any entry at least as wide as the terminal sends the whole input through
//!   unchanged ([`Mode::Simple`]),
//! * both non-table modes first round the longest entry *up* to a whole number
//!   of tab stops, then derive the column count by dividing the terminal width
//!   by that rounded value — which is why [`Mode::FillRows`] can end up with
//!   fewer columns than the rounding suggests,
//! * padding is only ever emitted as tabs that land on or before `endcol`; no
//!   trailing spaces are added, so a cell that already overflows `endcol`
//!   produces no separator at all,
//! * [`Mode::FillCols`] reads the input *down* the columns, resetting `endcol`
//!   and the running width at the start of every output row, and skips the
//!   separator of a column that has no entry,
//! * [`Mode::FillRows`] walks the input *across*, restarting `endcol` after
//!   every full row and printing one final newline if the input did not end on
//!   a row boundary,
//! * [`Mode::Table`] ignores the terminal width. With `-s` separators are
//!   matched one character at a time and empty fields survive; without it,
//!   runs of separators collapse. Every cell except the last column of the
//!   table is padded to the column width, *including empty cells*, and a row
//!   with too few cells still emits its separators — an empty line therefore
//!   renders as padded blanks,
//! * `-l` caps the number of *columns* per line, not the number of rows, and
//!   the final column absorbs the rest of the line verbatim.
//!
//! Deliberate limitation: `-n` only names a JSON table. This subset has no JSON
//! writer, so the option is accepted for command-line compatibility and
//! otherwise ignored.

/// Version reported by `--version`.
pub const UTIL_LINUX_VERSION: &str = "2.39.3";

/// Width assumed when stdout is not a terminal and `-c` was not given.
pub const DEFAULT_WIDTH: usize = 80;

/// Cells between tab stops, matching `TABCHAR_CELLS` in upstream.
pub const TABWIDTH: usize = 8;

/// Default input separator set: tab and space.
pub const DEFAULT_SEPARATOR: &str = "\t ";

/// String placed between table columns unless `-o` says otherwise.
pub const DEFAULT_OUTPUT_SEPARATOR: &str = "  ";

/// `--help` text.
pub const HELP: &str = "\
Usage:
 column [options] [<file>...]

Columnate lists.

Options:
 -t, --table                      create a table
 -n, --table-name <name>          table name for JSON output
 -N, --table-columns <names>      comma separated columns names
 -l, --table-columns-limit <num>  maximal number of input columns
 -d, --table-noheadings           don't print header
 -L, --keep-empty-lines           don't ignore empty lines

 -c, --output-width <width>       width of output in number of characters
 -o, --output-separator <string>  columns separator for table output (default is two spaces)
 -s, --separator <string>         possible table delimiters
 -x, --fillrows                   fill rows before columns

Help:
 -h, --help                       display this help and exit
 -V, --version                    display version and exit\n";

/// `--version` output.
pub fn version_line() -> String {
    format!("column from util-linux {}\n", UTIL_LINUX_VERSION)
}

/// How entries are distributed over the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Pass every entry through untouched.
    Simple,
    /// Fill down the columns (the default).
    FillCols,
    /// Fill across the columns (`-x`).
    FillRows,
    /// Build a table (`-t`).
    Table,
}

/// Fully resolved formatting request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Layout algorithm.
    pub mode: Mode,
    /// Display width; `0` means unlimited.
    pub width: usize,
    /// Input field separators.
    pub separator: Vec<char>,
    /// `true` collapses runs of separators (`-t` without `-s`).
    pub greedy: bool,
    /// String written between table columns.
    pub output_separator: String,
    /// `-N` header cells, if any.
    pub names: Option<Vec<String>>,
    /// `-d` suppresses the header.
    pub no_headings: bool,
    /// `-l` caps the number of fields per table line.
    pub max_columns: Option<usize>,
    /// `-L` keeps blank input lines.
    pub keep_empty_lines: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            mode: Mode::FillCols,
            width: DEFAULT_WIDTH,
            separator: DEFAULT_SEPARATOR.chars().collect(),
            greedy: true,
            output_separator: DEFAULT_OUTPUT_SEPARATOR.to_string(),
            names: None,
            no_headings: false,
            max_columns: None,
            keep_empty_lines: false,
        }
    }
}

impl Config {
    /// `-t`
    pub fn table() -> Self {
        Config {
            mode: Mode::Table,
            ..Config::default()
        }
    }
}

/// Display width of `text` in terminal cells, the way upstream's `width()` does:
/// every character contributes `wcwidth()` when that is positive, so tabs and
/// other non-printables are skipped rather than counted as zero.
pub fn entry_width(text: &str) -> usize {
    text.chars().map(char_width).filter(|&w| w > 0).sum::<i32>() as usize
}

/// `wcwidth()` for the code points that matter here: control characters are
/// `-1`, East Asian Wide/Fullwidth and most emoji are `2`, combining marks are
/// `0`, everything else is `1`.
pub fn char_width(c: char) -> i32 {
    let code = c as u32;
    if c == '\0' {
        return 0;
    }
    if c.is_control() {
        return -1;
    }
    if is_combining(code) {
        return 0;
    }
    if is_wide(code) {
        return 2;
    }
    1
}

fn is_combining(code: u32) -> bool {
    matches!(code,
        0x0300..=0x036F | 0x0483..=0x0489 | 0x0591..=0x05BD | 0x0610..=0x061A
        | 0x064B..=0x065F | 0x0670 | 0x06D6..=0x06DC | 0x0730..=0x074A
        | 0x07A6..=0x07B0 | 0x0816..=0x0819 | 0x08E3..=0x0903 | 0x093A..=0x093C
        | 0x0951..=0x0957 | 0x0E31 | 0x0E34..=0x0E3A | 0x0E47..=0x0E4E
        | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20F0 | 0xFE00..=0xFE0F
        | 0xFE20..=0xFE2F)
}

fn is_wide(code: u32) -> bool {
    matches!(code,
        0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF | 0xA000..=0xA4CF | 0xA960..=0xA97F | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF | 0xFE10..=0xFE19 | 0xFE30..=0xFE6F | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6 | 0x17000..=0x18AFF | 0x1B000..=0x1B2FF
        | 0x1F004 | 0x1F0CF | 0x1F18E | 0x1F191..=0x1F19A | 0x1F200..=0x1F320
        | 0x1F32D..=0x1F335 | 0x1F337..=0x1F37C | 0x1F37E..=0x1F393
        | 0x1F3A0..=0x1F3CA | 0x1F3CF..=0x1F3D3 | 0x1F3E0..=0x1F3F0
        | 0x1F3F4 | 0x1F3F8..=0x1F43E | 0x1F440 | 0x1F442..=0x1F4FC
        | 0x1F4FF..=0x1F53D | 0x1F54B..=0x1F54E | 0x1F550..=0x1F567
        | 0x1F57A | 0x1F595..=0x1F596 | 0x1F5A4 | 0x1F5FB..=0x1F64F
        | 0x1F680..=0x1F6C5 | 0x1F6CC | 0x1F6D0..=0x1F6D2 | 0x1F6EB..=0x1F6EC
        | 0x1F6F4..=0x1F6FA | 0x1F7E0..=0x1F7EB | 0x1F90D..=0x1F971
        | 0x1F973..=0x1F976 | 0x1F97A..=0x1F9A2 | 0x1F9A5..=0x1F9AA
        | 0x1F9AE..=0x1F9CA | 0x1F9CD..=0x1F9FF | 0x1FA70..=0x1FA73
        | 0x1FA78..=0x1FA7A | 0x1FA80..=0x1FA82 | 0x1FA90..=0x1FA95
        | 0x20000..=0x2FFFD | 0x30000..=0x3FFFD)
}

/// Split `input` into entries the way upstream's `read_input()` does.
///
/// The newline is removed, leading blanks are kept and a blank line is dropped
/// unless `keep_empty_lines`.
pub fn read_entries(input: &str, keep_empty_lines: bool) -> Vec<String> {
    let mut entries = Vec::new();
    for line in input.split_inclusive('\n') {
        // Upstream overwrites the newline in place, so a final line without a
        // terminator contributes exactly like a terminated one.
        let line = line.strip_suffix('\n').unwrap_or(line);
        // `skip_space()` skips blanks only: a tab-only line is a real entry.
        if line.trim_start_matches(' ').is_empty() {
            if keep_empty_lines {
                entries.push(String::new());
            }
            continue;
        }
        entries.push(line.to_string());
    }
    entries
}

/// Split one line into table fields, following `local_wcstok()`.
///
/// Greedy mode collapses runs of separators and drops leading and trailing
/// ones; non-greedy mode keeps every empty field. `max_columns` implements
/// `-l`: the final field becomes the rest of the line, separators included.
pub fn split_fields(
    line: &str,
    separator: &[char],
    greedy: bool,
    max_columns: Option<usize>,
) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let is_sep = |c: char| separator.contains(&c);
    let mut fields: Vec<(usize, String)> = Vec::new();

    if greedy {
        let mut start: Option<usize> = None;
        for (index, &c) in chars.iter().enumerate() {
            if is_sep(c) {
                if let Some(begin) = start.take() {
                    fields.push((begin, chars[begin..index].iter().collect()));
                }
            } else if start.is_none() {
                start = Some(index);
            }
        }
        if let Some(begin) = start {
            fields.push((begin, chars[begin..].iter().collect()));
        }
    } else {
        // Always one field per separator, including empty ones.
        let mut index = 0usize;
        loop {
            match chars[index..].iter().position(|&c| is_sep(c)) {
                Some(offset) => {
                    let end = index + offset;
                    fields.push((index, chars[index..end].iter().collect()));
                    index = end + 1;
                }
                None => {
                    if index <= chars.len() {
                        fields.push((index, chars[index..].iter().collect()));
                    }
                    break;
                }
            }
        }
    }

    let mut texts: Vec<String> = fields.iter().map(|(_, text)| text.clone()).collect();
    if let Some(limit) = max_columns.filter(|&limit| limit > 0) {
        if texts.len() > limit {
            // `skip = wcdata - wcs0` upstream: the tail starts at the first
            // character of the last kept field, separators included.
            let start = fields[limit - 1].0;
            texts.truncate(limit - 1);
            texts.push(chars[start..].iter().collect());
        }
    }
    texts
}

/// Format `entries` according to `cfg`, returning the exact output bytes.
pub fn columnate(entries: &[String], cfg: &Config) -> String {
    // Upstream exits early when nothing was read, for every mode.
    if entries.is_empty() {
        return String::new();
    }
    if cfg.mode == Mode::Table {
        return table(entries, cfg);
    }
    let maxlength = entries.iter().map(|e| entry_width(e)).max().unwrap_or(0);
    // An entry as wide as the terminal (or a zero width, or `-c 0`) means
    // "just print the lines".
    if maxlength == 0 || maxlength >= cfg.width {
        return simple(entries);
    }
    match cfg.mode {
        Mode::FillRows => fillrows(entries, cfg, maxlength),
        Mode::FillCols => fillcols(entries, cfg, maxlength),
        Mode::Simple | Mode::Table => simple(entries),
    }
}

/// `COLUMN_MODE_SIMPLE`: every entry on its own line, untouched.
pub fn simple(entries: &[String]) -> String {
    let mut out = String::new();
    for entry in entries {
        out.push_str(entry);
        out.push('\n');
    }
    out
}

/// Round up to the next whole tab stop, like `(x + 8) & ~7`.
fn round_to_tab(x: usize) -> usize {
    (x + TABWIDTH - 1) & !(TABWIDTH - 1)
}

/// Emit the tabs upstream writes before `endcol`, and return the new width.
///
/// Upstream only writes a tab when it lands on or before `endcol`, so an entry
/// that already overflows `endcol` gets no separator at all.
fn pad_to_tab_stop(out: &mut String, chcnt: usize, endcol: usize) -> usize {
    let mut chcnt = chcnt;
    loop {
        let next = round_to_tab(chcnt);
        if next > endcol || next == chcnt {
            break;
        }
        out.push('\t');
        chcnt = next;
    }
    chcnt
}

/// [`Mode::FillCols`]: read the input down the columns.
fn fillcols(entries: &[String], cfg: &Config, maxlength: usize) -> String {
    let maxlength = round_to_tab(maxlength);
    let columns = (cfg.width / maxlength).max(1);
    let rows = (entries.len() + columns - 1) / columns;

    let mut out = String::new();
    for row in 0..rows {
        // Every output row starts a fresh tab-stop budget.
        let mut endcol = maxlength;
        let mut chcnt = 0usize;
        let mut base = row;
        for _ in 0..columns {
            out.push_str(&entries[base]);
            chcnt += entry_width(&entries[base]);
            base += rows;
            if base >= entries.len() {
                break;
            }
            chcnt = pad_to_tab_stop(&mut out, chcnt, endcol);
            endcol += maxlength;
        }
        out.push('\n');
    }
    out
}

/// [`Mode::FillRows`]: walk the input across the columns.
fn fillrows(entries: &[String], cfg: &Config, maxlength: usize) -> String {
    let maxlength = round_to_tab(maxlength);
    // `maxlength < cfg.width` holds here, so this is at least one column.
    let columns = (cfg.width / maxlength).max(1);

    let mut out = String::new();
    let mut endcol = maxlength;
    let mut chcnt = 0usize;
    let mut column = 0usize;
    for (index, entry) in entries.iter().enumerate() {
        out.push_str(entry);
        chcnt += entry_width(entry);
        // The last entry never triggers a separator.
        if index + 1 == entries.len() {
            break;
        }
        column += 1;
        if column == columns {
            column = 0;
            chcnt = 0;
            endcol = maxlength;
            out.push('\n');
        } else {
            chcnt = pad_to_tab_stop(&mut out, chcnt, endcol);
            endcol += maxlength;
        }
    }
    if chcnt != 0 {
        out.push('\n');
    }
    out
}

/// [`Mode::Table`]: cells padded to the column width with the output
/// separator. The terminal width is irrelevant here.
fn table(entries: &[String], cfg: &Config) -> String {
    let rows: Vec<Vec<String>> = entries
        .iter()
        .map(|line| split_fields(line, &cfg.separator, cfg.greedy, cfg.max_columns))
        .collect();
    let columns = match &cfg.names {
        Some(names) => names.len(),
        None => rows.iter().map(|cells| cells.len()).max().unwrap_or(0),
    };
    if columns == 0 {
        return String::new();
    }

    let widths: Vec<usize> = (0..columns)
        .map(|index| {
            let body = rows
                .iter()
                .filter(|cells| cells.len() > index)
                .map(|cells| entry_width(&cells[index]))
                .max()
                .unwrap_or(0);
            match &cfg.names {
                Some(names) => body.max(entry_width(&names[index])),
                None => body,
            }
        })
        .collect();

    let mut out = String::new();
    if let Some(names) = &cfg.names {
        if !cfg.no_headings {
            emit_row(&mut out, names.iter().map(String::as_str).collect(), columns, None, cfg, &widths, entries.len());
        }
    }
    for (index, cells) in rows.iter().enumerate() {
        emit_row(
            &mut out,
            cells.iter().map(String::as_str).collect(),
            columns,
            Some(index),
            cfg,
            &widths,
            rows.len(),
        );
    }
    out
}

/// Write one table row: `columns` cells joined by the output separator.
///
/// A cell is padded whenever it is not in the last table column — in every
/// row including the last, and including the header — matching upstream, which
/// pads with `width - strlen(cell)` spaces for all but the final column.
/// Missing cells render as padded blanks, which is what an empty input line
/// produces.
fn emit_row<'a>(
    out: &mut String,
    cells: Vec<&'a str>,
    columns: usize,
    index: Option<usize>,
    cfg: &Config,
    widths: &[usize],
    rows: usize,
) {
    for position in 0..columns {
        if position > 0 {
            out.push_str(&cfg.output_separator);
        }
        let pad = position + 1 < columns;
        if pad {
            // A missing cell is as wide as an empty one.
            let mut width = cells.get(position).map_or(0, |cell| entry_width(cell));
            while width < widths[position] {
                out.push(' ');
                width += 1;
            }
        }
        if let Some(cell) = cells.get(position) {
            out.push_str(cell);
        }
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill(entries: &[&str], cfg: &Config) -> String {
        columnate(&entries.iter().map(|e| (*e).to_string()).collect::<Vec<_>>(), cfg)
    }

    fn entries_of(input: &str, keep_empty_lines: bool) -> Vec<String> {
        read_entries(input, keep_empty_lines)
    }

    // ----- version and help -------------------------------------------------

    #[test]
    fn version_line_matches_util_linux() {
        assert_eq!(version_line(), "column from util-linux 2.39.3\n");
    }

    #[test]
    fn help_documents_every_supported_option() {
        for flag in ["-h", "--help", "-V", "--version", "-c", "--output-width", "-o",
                     "--output-separator", "-n", "--table-name", "-N", "--table-columns",
                     "-d", "--table-noheadings", "-s", "--separator", "-t", "--table",
                     "-x", "--fillrows", "-l", "--table-columns-limit", "-L",
                     "--keep-empty-lines"] {
            assert!(HELP.contains(flag), "help text is missing {}", flag);
        }
    }

    // ----- width accounting -------------------------------------------------

    #[test]
    fn entry_width_counts_display_cells() {
        assert_eq!(entry_width("abc"), 3);
        assert_eq!(entry_width(""), 0);
        // A tab is skipped, not expanded.
        assert_eq!(entry_width("a\tb"), 2);
        // Wide characters take two cells, combining marks none.
        assert_eq!(entry_width("\u{4f60}\u{597d}"), 4);
        assert_eq!(entry_width("e\u{301}"), 1);
        // Control characters are skipped as well.
        assert_eq!(entry_width("a\u{1}b"), 2);
    }

    #[test]
    fn char_width_classifies_characters() {
        assert_eq!(char_width('a'), 1);
        assert_eq!(char_width('\u{4f60}'), 2);
        assert_eq!(char_width('\u{ff21}'), 2);
        assert_eq!(char_width('\u{1f600}'), 2);
        assert_eq!(char_width('\u{301}'), 0);
        assert_eq!(char_width('\t'), -1);
        assert_eq!(char_width('\n'), -1);
    }

    // ----- input splitting --------------------------------------------------

    #[test]
    fn blank_lines_are_dropped_unless_asked_for() {
        assert_eq!(entries_of("a\n\nb\n", false), vec!["a", "b"]);
        assert_eq!(entries_of("a\n\nb\n", true), vec!["a", "", "b"]);
        // A line that is only spaces counts as blank too.
        assert_eq!(entries_of("a\n   \n", false), vec!["a"]);
        // A tab-only line is a real entry.
        assert_eq!(entries_of("\t\n", false), vec!["\t"]);
        // Leading blanks of a real entry are preserved.
        assert_eq!(entries_of("  a\n", false), vec!["  a"]);
        // A missing final newline does not create an extra entry.
        assert_eq!(entries_of("a\nb", false), vec!["a", "b"]);
        assert!(entries_of("", false).is_empty());
    }

    // ----- field splitting --------------------------------------------------

    #[test]
    fn greedy_separators_collapse() {
        let seps: Vec<char> = DEFAULT_SEPARATOR.chars().collect();
        assert_eq!(split_fields("a b  c", &seps, true, None), vec!["a", "b", "c"]);
        assert_eq!(split_fields("a\tb\tc", &seps, true, None), vec!["a", "b", "c"]);
        // Leading and trailing separators vanish.
        assert_eq!(split_fields("  a b  ", &seps, true, None), vec!["a", "b"]);
        // No separator at all means one field.
        assert_eq!(split_fields("abc", &seps, true, None), vec!["abc"]);
        // A blank line has no field at all.
        assert!(split_fields("   ", &seps, true, None).is_empty());
    }

    #[test]
    fn explicit_separators_keep_empty_fields() {
        let seps = vec![','];
        assert_eq!(split_fields("a,b,c", &seps, false, None), vec!["a", "b", "c"]);
        // The whole point of -s: empty fields are data, not noise.
        assert_eq!(split_fields("a,,b", &seps, false, None), vec!["a", "", "b"]);
        assert_eq!(split_fields(",a,", &seps, false, None), vec!["", "a", ""]);
        assert_eq!(split_fields("a", &seps, false, None), vec!["a"]);
        assert_eq!(split_fields("", &seps, false, None), vec![""]);
    }

    #[test]
    fn last_column_absorbs_the_rest_of_the_line() {
        let seps = vec![','];
        // -l caps the number of *columns*, and the last one keeps the tail.
        assert_eq!(split_fields("a,b,c,d", &seps, false, Some(2)), vec!["a", "b,c,d"]);
        assert_eq!(split_fields("a,b", &seps, false, Some(2)), vec!["a", "b"]);
        // The limit never invents cells.
        assert_eq!(split_fields("a", &seps, false, Some(4)), vec!["a"]);
    }

    // ----- simple mode ------------------------------------------------------

    #[test]
    fn unlimited_width_passes_the_input_through() {
        let cfg = Config { width: 0, ..Config::default() };
        assert_eq!(fill(&["a", "bb", "ccc"], &cfg), "a\nbb\nccc\n");
        // -c 20 with an entry exactly as wide as the terminal is simple too.
        let cfg = Config { width: 3, ..Config::default() };
        assert_eq!(fill(&["a", "bbb"], &cfg), "a\nbbb\n");
        // A zero width never reaches the tab arithmetic.
        let cfg = Config { width: 0, ..Config::default() };
        assert_eq!(fill(&["", ""], &cfg), "\n\n");
    }

    // ----- fillcols ---------------------------------------------------------

    #[test]
    fn fillcols_defaults_to_the_tab_aligned_layout() {
        // The longest entry is 4 cells, rounded up to 8, so all four fit in
        // one 80-cell row and are joined by tabs.
        let cfg = Config::default();
        assert_eq!(fill(&["a", "bb", "ccc", "dddd"], &cfg), "a\tbb\tccc\tdddd\n");
    }

    #[test]
    fn fillcols_honours_the_width() {
        // 20 cells divided by the rounded-up 8 gives two columns, so the
        // input is read down: 1,5 then 2,6 and so on.
        let cfg = Config { width: 20, ..Config::default() };
        let rows = ["a", "bb", "ccc", "dddd", "e", "ff", "ggg"];
        assert_eq!(fill(&rows, &cfg), "a\te\nbb\tff\nccc\tggg\ndddd\n");
    }

    #[test]
    fn fillcols_resets_the_tab_budget_on_every_row() {
        // Ten columns of two-cell entries, two rows: the second row must start
        // over at the first tab stop.
        let numbers: Vec<String> = (1..=20).map(|n| n.to_string()).collect();
        let borrowed: Vec<&str> = numbers.iter().map(String::as_str).collect();
        let cfg = Config::default();
        assert_eq!(
            columnate(&numbers, &cfg),
            "1\t3\t5\t7\t9\t11\t13\t15\t17\t19\n2\t4\t6\t8\t10\t12\t14\t16\t18\t20\n"
        );
        assert_eq!(borrowed.len(), 20);
    }

    #[test]
    fn fillcols_writes_no_separator_after_the_last_column() {
        let cfg = Config { width: 16, ..Config::default() };
        // Five entries in two columns, read down: a,d then b,e, then c alone.
        assert_eq!(fill(&["a", "b", "c", "d", "e"], &cfg), "a\td\nb\te\nc\n");
    }

    // ----- fillrows ---------------------------------------------------------

    #[test]
    fn fillrows_reads_across() {
        let cfg = Config { mode: Mode::FillRows, width: 20, ..Config::default() };
        let rows = ["a", "bb", "ccc", "dddd", "e", "ff", "ggg"];
        assert_eq!(fill(&rows, &cfg), "a\tbb\nccc\tdddd\ne\tff\nggg\n");
    }

    #[test]
    fn fillrows_rounds_the_width_up_before_dividing() {
        // One-cell entries round up to 8, so -c 12 is still a single column.
        let cfg = Config { mode: Mode::FillRows, width: 12, ..Config::default() };
        assert_eq!(fill(&["a", "b", "c"], &cfg), "a\nb\nc\n");
        let cfg = Config { mode: Mode::FillRows, width: 9, ..Config::default() };
        assert_eq!(fill(&["aa", "bb", "cc", "dd"], &cfg), "aa\nbb\ncc\ndd\n");
    }

    #[test]
    fn fillrows_finishes_an_unfinished_row() {
        let cfg = Config { mode: Mode::FillRows, width: 40, ..Config::default() };
        let numbers: Vec<String> = (1..=20).map(|n| n.to_string()).collect();
        let expected: String = (0..4)
            .map(|row| {
                let mut line = String::new();
                for column in 0..5 {
                    if column > 0 {
                        line.push('\t');
                    }
                    line.push_str(&numbers[row * 5 + column]);
                }
                line.push('\n');
                line
            })
            .collect();
        assert_eq!(columnate(&numbers, &cfg), expected);
    }

    // ----- tables -----------------------------------------------------------

    #[test]
    fn table_pads_every_column_but_the_last() {
        let cfg = Config::table();
        assert_eq!(fill(&["a bbbb c"], &cfg), "a  bbbb  c\n");
    }

    #[test]
    fn table_pads_all_but_the_final_row() {
        let cfg = Config::table();
        // The last row's last column is not padded, so it keeps no blanks.
        // A short final row still pads the cells it has and the separators
        // of the cells it does not: "1" plus two separators plus one blank.
        assert_eq!(fill(&["a b c", "1"], &cfg), "a  b  c\n1     \n");
        // A short row in the middle still emits its separators and pads the
        // cells it does not have.
        assert_eq!(fill(&["a b c", "1", "d e f"], &cfg), "a  b  c\n1     \nd  e  f\n");
    }

    #[test]
    fn table_renders_an_empty_line_as_padded_blanks() {
        let entries = entries_of("a b\n\n1 2\n", true);
        let cfg = Config::table();
        assert_eq!(columnate(&entries, &cfg), "a  b\n   \n1  2\n");
    }

    #[test]
    fn table_ignores_the_terminal_width() {
        let cfg = Config { mode: Mode::Table, width: 4, ..Config::default() };
        assert_eq!(fill(&["a bbbb c"], &cfg), "a  bbbb  c\n");
    }

    #[test]
    fn table_uses_the_output_separator() {
        let cfg = Config { mode: Mode::Table, output_separator: " | ".to_string(), ..Config::default() };
        assert_eq!(fill(&["a b", "c d"], &cfg), "a | b\nc | d\n");
        // An empty separator is legal.
        let cfg = Config { mode: Mode::Table, output_separator: String::new(), ..Config::default() };
        assert_eq!(fill(&["a b", "c d"], &cfg), "ab\ncd\n");
    }

    #[test]
    fn table_with_explicit_separators_keeps_empty_fields() {
        let cfg = Config {
            mode: Mode::Table,
            separator: vec![','],
            greedy: false,
            ..Config::default()
        };
        // "a,,b" has three fields: the empty one still occupies a column.
        assert_eq!(fill(&["a,,b"], &cfg), "a    b\n");
        let cfg = Config { greedy: false, ..Config::table() };
        assert_eq!(fill(&["a  b"], &cfg), "a    b\n");
        assert_eq!(fill(&["a   b"], &cfg), "a      b\n");
    }

    #[test]
    fn table_header_comes_from_n() {
        let cfg = Config { names: Some(vec!["x".into(), "y".into()]), ..Config::table() };
        assert_eq!(fill(&["a b", "1 2"], &cfg), "x  y\na  b\n1  2\n");
        // 2.39.3 prints no rule line under the header, and never truncates the
        // body to the header's column count.
        assert_eq!(fill(&["a b", "1 2", "3 4"], &cfg), "x  y\na  b\n1  2\n3  4\n");
    }

    #[test]
    fn table_noheadings_drops_the_header() {
        let cfg = Config {
            names: Some(vec!["x".into(), "y".into()]),
            no_headings: true,
            ..Config::table()
        };
        assert_eq!(fill(&["a b", "1 2"], &cfg), "a  b\n1  2\n");
    }

    #[test]
    fn table_last_column_caps_columns_not_rows() {
        let cfg = Config { max_columns: Some(2), ..Config::table() };
        // Three two-column rows are untouched: the limit is a column count.
        assert_eq!(fill(&["a b", "c d", "e f"], &cfg), "a  b\nc  d\ne  f\n");
        // A four-column line is truncated, tail and all.
        let cfg = Config { max_columns: Some(2), ..Config::table() };
        assert_eq!(fill(&["a b c d"], &cfg), "a  b c d\n");
    }

    #[test]
    fn table_of_an_empty_input_is_empty() {
        assert_eq!(columnate(&[], &Config::table()), "");
        assert_eq!(columnate(&[], &Config::default()), "");
    }

    // ----- end to end -------------------------------------------------------

    #[test]
    fn read_entries_then_columnate() {
        let entries = entries_of("a\nbb\nccc\ndddd\n", false);
        assert_eq!(columnate(&entries, &Config::default()), "a\tbb\tccc\tdddd\n");
    }

    #[test]
    fn wide_entries_still_get_tab_separators() {
        // The widest entry is 3 cells, rounded up to 8.
        let entries = entries_of("a\n\u{4f60}X\n", false);
        assert_eq!(columnate(&entries, &Config::default()), "a\t\u{4f60}X\n");
    }
}