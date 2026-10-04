//! `nl(1)`: number lines of files.
//!
//! The manual's defaults are `-bt -d':\' -fn -hn -i1 -l1 -n'rn' -s<TAB> -v1 -w6`,
//! so [`Options::default`] starts from those values.
//!
//! Two details the manual only hints at, taken from coreutils' own `nl.c`:
//!
//! * The `-d` value is a *prefix*, not a line. A line counts as a delimiter
//!   when it is exactly one of the three repeated forms, so `\:` alone is the
//!   footer, `\:\:` the body and `\:\:\:` the header. A missing second
//!   character implies `:`, which is why `-da` looks for `a:` and `-d:` for
//!   `::`.
//! * A delimiter line is replaced by an empty line, it is not echoed.
//!
//! A line that gets no number is still indented: the number field and the
//! separator become spaces, so the text lines up.

pub mod bre;

/// Which lines of a section get a number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Style {
    /// `a`: every line.
    All,
    /// `t`: only the lines that are not empty.
    NonEmpty,
    /// `n`: no line.
    None,
    /// `pBRE`: only the lines matching a basic regular expression.
    Pattern(bre::Regex),
}

/// How the number itself is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// `ln`: left justified, no leading zeros.
    Left,
    /// `rn`: right justified, no leading zeros.
    Right,
    /// `rz`: right justified, leading zeros.
    Zero,
}

impl Format {
    /// The manual's three names, written without the quotes.
    pub fn parse(text: &str) -> Option<Format> {
        match text {
            "ln" => Some(Format::Left),
            "rn" => Some(Format::Right),
            "rz" => Some(Format::Zero),
            _ => None,
        }
    }

    /// Write `number` in `width` columns.
    pub fn render(self, number: i64, width: usize) -> String {
        let digits = number.to_string();
        if digits.len() >= width {
            return digits;
        }
        let padding = width - digits.len();
        match self {
            Format::Left => format!("{}{}", digits, " ".repeat(padding)),
            Format::Right => format!("{}{}", " ".repeat(padding), digits),
            // `%0*` keeps the sign in front of the zeros.
            Format::Zero if number < 0 => format!(
                "-{}{}",
                "0".repeat(padding),
                digits.trim_start_matches('-')
            ),
            Format::Zero => format!("{}{}", "0".repeat(padding), digits),
        }
    }
}

/// The three delimiter lines the manual builds from `-d`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delimiters {
    pub header: Vec<u8>,
    pub body: Vec<u8>,
    pub footer: Vec<u8>,
}

impl Delimiters {
    /// Build the three forms from the `-d` value.
    ///
    /// POSIX takes two characters and says a missing second one implies `:`,
    /// so `-da` builds on `a:` and not on `aa`. As a GNU extension any length
    /// is allowed and taken whole, and the empty value disables section
    /// matching altogether.
    pub fn from_option(value: &[u8]) -> Delimiters {
        let base: Vec<u8> = match value.len() {
            0 => Vec::new(),
            1 => vec![value[0], b':'],
            _ => value.to_vec(),
        };
        let repeat = |times: usize| -> Vec<u8> {
            let mut out = Vec::with_capacity(base.len() * times);
            for _ in 0..times {
                out.extend_from_slice(&base);
            }
            out
        };
        Delimiters {
            header: repeat(3),
            body: repeat(2),
            footer: repeat(1),
        }
    }

    /// Whether a line is one of the three delimiters.
    pub fn classify(&self, line: &[u8]) -> Option<Section> {
        // A delimiter is at least two characters, so a one-character line can
        // never be one.
        if line.len() < 2 || self.footer.len() < 2 {
            return None;
        }
        if line == self.header {
            Some(Section::Header)
        } else if line == self.body {
            Some(Section::Body)
        } else if line == self.footer {
            Some(Section::Footer)
        } else {
            None
        }
    }
}

/// Which part of the file a line belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Header,
    Body,
    Footer,
}

/// The parsed command line.
#[derive(Clone, Debug)]
pub struct Options {
    pub body: Style,
    pub header: Style,
    pub footer: Style,
    pub delimiters: Delimiters,
    pub increment: i64,
    /// How many empty lines count as one, `-l`; only STYLE `a` looks at it.
    pub join_blank: i64,
    pub format: Format,
    pub separator: Vec<u8>,
    pub start: i64,
    pub width: usize,
    /// `-p`: keep counting across sections.
    pub no_renumber: bool,
    pub files: Vec<String>,
}

impl Default for Options {
    /// The manual's "Default options" line.
    fn default() -> Self {
        Options {
            body: Style::NonEmpty,
            header: Style::None,
            footer: Style::None,
            delimiters: Delimiters::from_option(b"\\:"),
            increment: 1,
            join_blank: 1,
            format: Format::Right,
            separator: b"\t".to_vec(),
            start: 1,
            width: 6,
            no_renumber: false,
            files: Vec::new(),
        }
    }
}

impl Options {
    /// The style that decides numbering for `section`.
    pub fn style_for(&self, section: Section) -> &Style {
        match section {
            Section::Header => &self.header,
            Section::Body => &self.body,
            Section::Footer => &self.footer,
        }
    }

    /// The spaces that stand in for a missing number, so the text lines up.
    ///
    /// coreutils builds this from the field width and the separator length,
    /// whatever format was chosen, so zero padding never shows up here.
    pub fn blank_prefix(&self) -> Vec<u8> {
        vec![b' '; self.width + self.separator.len()]
    }
}

/// The numbering state machine.
///
/// The counter starts at `-v` and moves on each *printed* number, so `-i 0`
/// repeats the same number and a style that prints nothing leaves it alone.
#[derive(Debug)]
pub struct Numberer {
    line_no: i64,
    /// Consecutive empty lines seen so far, for `-l`.
    blank_lines: i64,
    overflowed: bool,
}

impl Numberer {
    pub fn new(options: &Options) -> Numberer {
        Numberer {
            line_no: options.start,
            blank_lines: 0,
            overflowed: false,
        }
    }

    /// A delimiter starts a new section, which restarts the numbering unless
    /// `-p` asked for it to continue.
    pub fn enter_section(&mut self, options: &Options) {
        if !options.no_renumber {
            self.line_no = options.start;
            self.overflowed = false;
        }
        self.blank_lines = 0;
    }

    /// Whether the next number would leave the range coreutils uses.
    pub fn overflowed(&self) -> bool {
        self.overflowed
    }

    /// The number for `line`, or None when the style says to print none.
    pub fn number(&mut self, line: &[u8], section: Section, options: &Options) -> Option<i64> {
        let empty = line.is_empty();
        let wanted = match options.style_for(section) {
            Style::All => {
                // Only STYLE `a` groups empty lines, and it charges the group
                // on the blank_join'th line rather than the first.
                if options.join_blank > 1 && empty {
                    self.blank_lines += 1;
                    if self.blank_lines == options.join_blank {
                        self.blank_lines = 0;
                        true
                    } else {
                        false
                    }
                } else {
                    if !empty {
                        self.blank_lines = 0;
                    }
                    true
                }
            }
            Style::NonEmpty => {
                if !empty {
                    self.blank_lines = 0;
                }
                !empty
            }
            Style::None => false,
            Style::Pattern(regex) => regex.is_match(line),
        };
        if !wanted {
            return None;
        }
        let number = self.line_no;
        match self.line_no.checked_add(options.increment) {
            Some(next) => self.line_no = next,
            None => self.overflowed = true,
        }
        Some(number)
    }
}

/// A delimiter line comes out as a bare newline, with the number field dropped.
pub fn delimiter_line() -> Vec<u8> {
    vec![b'\n']
}

/// One line of output: the number, the separator and the line itself.
pub fn text_line(number: Option<i64>, line: &[u8], options: &Options) -> Vec<u8> {
    let mut out = Vec::new();
    match number {
        Some(value) => {
            out.extend_from_slice(options.format.render(value, options.width).as_bytes());
            out.extend_from_slice(&options.separator);
        }
        None => out.extend_from_slice(&options.blank_prefix()),
    }
    out.extend_from_slice(line);
    out.push(b'\n');
    out
}

/// The diagnostics coreutils prints.
pub fn unrecognized_option_message(name: &str) -> String {
    format!("nl: unrecognized option '--{}'", name)
}

pub fn invalid_option_message(letter: char) -> String {
    format!("nl: invalid option -- '{}'", letter)
}

/// Getopt words the two shapes differently: a long option names itself, a
/// short one has only its letter left to give.
pub fn requires_argument_message(letter: char) -> String {
    format!("nl: option requires an argument -- '{}'", letter)
}

pub fn long_requires_argument_message(name: &str) -> String {
    format!("nl: option '--{}' requires an argument", name)
}

pub fn try_help_message() -> String {
    "Try 'nl --help' for more information.".to_string()
}

/// The style diagnostics name the option they came from, and coreutils quotes
/// the value with typographic quotes.
pub fn bad_style_message(option: char, style: &str) -> String {
    let which = match option {
        'h' => "header",
        'f' => "footer",
        _ => "body",
    };
    format!("nl: invalid {} numbering style: \u{2018}{}\u{2019}", which, style)
}

pub fn bad_format_message(format: &str) -> String {
    format!("nl: invalid line numbering format: \u{2018}{}\u{2019}", format)
}

pub fn bad_number_message(what: &str, value: &str, reason: Option<&str>) -> String {
    match reason {
        Some(reason) => format!("nl: invalid {}: \u{2018}{}\u{2019}: {}", what, value, reason),
        None => format!("nl: invalid {}: \u{2018}{}\u{2019}", what, value),
    }
}

pub fn cannot_open_message(file: &str, reason: &str) -> String {
    format!("nl: {}: {}", file, reason)
}

pub fn invalid_regex_message(reason: &str) -> String {
    format!("nl: {}", reason)
}

pub fn overflow_message() -> String {
    "nl: line number overflow".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_manual() {
        let options = Options::default();
        assert_eq!(options.body, Style::NonEmpty);
        assert_eq!(options.header, Style::None);
        assert_eq!(options.footer, Style::None);
        assert_eq!(options.delimiters.footer, b"\\:");
        assert_eq!(options.delimiters.body, b"\\:\\:");
        assert_eq!(options.delimiters.header, b"\\:\\:\\:");
        assert_eq!(options.increment, 1);
        assert_eq!(options.join_blank, 1);
        assert_eq!(options.format, Format::Right);
        assert_eq!(options.separator, b"\t");
        assert_eq!(options.start, 1);
        assert_eq!(options.width, 6);
        assert!(!options.no_renumber);
    }

    #[test]
    fn the_three_delimiters_are_the_prefix_repeated() {
        let delimiters = Delimiters::from_option(b"\\:");
        assert_eq!(delimiters.classify(b"\\:"), Some(Section::Footer));
        assert_eq!(delimiters.classify(b"\\:\\:"), Some(Section::Body));
        assert_eq!(delimiters.classify(b"\\:\\:\\:"), Some(Section::Header));
        assert_eq!(delimiters.classify(b"\\:\\:\\:\\:"), None);
        assert_eq!(delimiters.classify(b"\\:\\x"), None);
    }

    #[test]
    fn a_one_character_delimiter_gains_a_colon() {
        // A missing second character implies ':', so ':' becomes '::' and 'a'
        // becomes 'a:'. Only the two-character form matches 'aa'.
        let delimiters = Delimiters::from_option(b":");
        assert_eq!(delimiters.footer, b"::");
        assert_eq!(delimiters.body, b"::::");
        assert_eq!(delimiters.classify(b"::"), Some(Section::Footer));
        let delimiters = Delimiters::from_option(b"a");
        assert_eq!(delimiters.footer, b"a:");
        assert_eq!(delimiters.body, b"a:a:");
        assert_eq!(delimiters.classify(b"a:"), Some(Section::Footer));
        assert_eq!(delimiters.classify(b"a:a:"), Some(Section::Body));
        assert_eq!(delimiters.classify(b"aa"), None);
    }

    #[test]
    fn a_longer_delimiter_is_taken_whole() {
        let three = Delimiters::from_option(b"ab:");
        assert_eq!(three.footer, b"ab:");
        assert_eq!(three.body, b"ab:ab:");
        assert_eq!(three.classify(b"ab:"), Some(Section::Footer));
        assert_eq!(three.classify(b"ab"), None);
        let two = Delimiters::from_option(b"ab");
        assert_eq!(two.classify(b"ab"), Some(Section::Footer));
        assert_eq!(two.classify(b"abab"), Some(Section::Body));
        assert_eq!(two.classify(b"ababab"), Some(Section::Header));
    }

    #[test]
    fn an_empty_delimiter_never_matches() {
        let delimiters = Delimiters::from_option(b"");
        assert_eq!(delimiters.footer.len(), 0);
        assert_eq!(delimiters.classify(b"\\:"), None);
        assert_eq!(delimiters.classify(b""), None);
    }

    #[test]
    fn a_one_character_line_is_never_a_delimiter() {
        let delimiters = Delimiters::from_option(b"::");
        assert_eq!(delimiters.classify(b":"), None);
    }

    #[test]
    fn formats_pad_as_documented() {
        assert_eq!(Format::Right.render(7, 6), "     7");
        assert_eq!(Format::Left.render(7, 6), "7     ");
        assert_eq!(Format::Zero.render(7, 6), "000007");
        assert_eq!(Format::Right.render(1234567, 6), "1234567");
        assert_eq!(Format::Zero.render(-7, 4), "-007");
    }

    #[test]
    fn only_non_empty_lines_are_numbered_by_default() {
        let options = Options::default();
        let mut numberer = Numberer::new(&options);
        assert_eq!(numberer.number(b"a", Section::Body, &options), Some(1));
        assert_eq!(numberer.number(b"", Section::Body, &options), None);
        assert_eq!(numberer.number(b"b", Section::Body, &options), Some(2));
    }

    #[test]
    fn style_a_numbers_empty_lines_too() {
        let mut options = Options::default();
        options.body = Style::All;
        let mut numberer = Numberer::new(&options);
        assert_eq!(numberer.number(b"a", Section::Body, &options), Some(1));
        assert_eq!(numberer.number(b"", Section::Body, &options), Some(2));
        assert_eq!(numberer.number(b"", Section::Body, &options), Some(3));
        assert_eq!(numberer.number(b"b", Section::Body, &options), Some(4));
    }

    #[test]
    fn a_run_of_empty_lines_is_charged_on_the_last_one() {
        let mut options = Options::default();
        options.body = Style::All;
        options.join_blank = 2;
        let mut numberer = Numberer::new(&options);
        assert_eq!(numberer.number(b"a", Section::Body, &options), Some(1));
        assert_eq!(numberer.number(b"", Section::Body, &options), None);
        assert_eq!(numberer.number(b"", Section::Body, &options), Some(2));
        assert_eq!(numberer.number(b"", Section::Body, &options), None);
        assert_eq!(numberer.number(b"b", Section::Body, &options), Some(3));
    }

    #[test]
    fn join_blank_is_ignored_by_style_t() {
        let mut options = Options::default();
        options.join_blank = 2;
        let mut numberer = Numberer::new(&options);
        assert_eq!(numberer.number(b"", Section::Body, &options), None);
        assert_eq!(numberer.number(b"", Section::Body, &options), None);
    }

    #[test]
    fn the_counter_restarts_at_each_section_unless_p_is_given() {
        let mut options = Options::default();
        let mut numberer = Numberer::new(&options);
        assert_eq!(numberer.number(b"a", Section::Body, &options), Some(1));
        numberer.enter_section(&options);
        assert_eq!(numberer.number(b"b", Section::Body, &options), Some(1));

        options.no_renumber = true;
        let mut numberer = Numberer::new(&options);
        assert_eq!(numberer.number(b"a", Section::Body, &options), Some(1));
        numberer.enter_section(&options);
        assert_eq!(numberer.number(b"b", Section::Body, &options), Some(2));
    }

    #[test]
    fn the_increment_and_the_first_number_are_honoured() {
        let mut options = Options::default();
        options.start = 0;
        options.increment = 5;
        let mut numberer = Numberer::new(&options);
        assert_eq!(numberer.number(b"a", Section::Body, &options), Some(0));
        assert_eq!(numberer.number(b"b", Section::Body, &options), Some(5));
    }

    #[test]
    fn an_increment_of_zero_repeats_the_number() {
        let mut options = Options::default();
        options.increment = 0;
        let mut numberer = Numberer::new(&options);
        assert_eq!(numberer.number(b"a", Section::Body, &options), Some(1));
        assert_eq!(numberer.number(b"b", Section::Body, &options), Some(1));
    }

    #[test]
    fn an_overshoot_is_reported_once() {
        let mut options = Options::default();
        options.start = i64::MAX;
        let mut numberer = Numberer::new(&options);
        assert_eq!(numberer.number(b"a", Section::Body, &options), Some(i64::MAX));
        assert!(numberer.overflowed());
    }

    #[test]
    fn a_numbered_line_carries_the_number_and_the_separator() {
        let options = Options::default();
        assert_eq!(text_line(Some(1), b"a", &options), b"     1\ta\n".to_vec());
    }

    #[test]
    fn an_unnumbered_line_is_indented_by_width_and_separator() {
        let options = Options::default();
        assert_eq!(text_line(None, b"a", &options), b"       a\n".to_vec());
        assert_eq!(text_line(None, b"", &options), b"       \n".to_vec());
    }

    #[test]
    fn a_delimiter_line_becomes_a_bare_newline() {
        assert_eq!(delimiter_line(), b"\n".to_vec());
    }

    #[test]
    fn messages_match_coreutils() {
        assert_eq!(
            unrecognized_option_message("nonsense"),
            "nl: unrecognized option '--nonsense'"
        );
        assert_eq!(invalid_option_message('Z'), "nl: invalid option -- 'Z'");
        assert_eq!(
            requires_argument_message('w'),
            "nl: option requires an argument -- 'w'"
        );
        assert_eq!(
            long_requires_argument_message("number-width"),
            "nl: option '--number-width' requires an argument"
        );
        assert_eq!(
            bad_style_message('b', "z"),
            "nl: invalid body numbering style: \u{2018}z\u{2019}"
        );
        assert_eq!(
            bad_style_message('h', "z"),
            "nl: invalid header numbering style: \u{2018}z\u{2019}"
        );
        assert_eq!(
            bad_style_message('f', "z"),
            "nl: invalid footer numbering style: \u{2018}z\u{2019}"
        );
        assert_eq!(
            bad_format_message("xx"),
            "nl: invalid line numbering format: \u{2018}xx\u{2019}"
        );
        assert_eq!(
            bad_number_message("line number increment", "x", None),
            "nl: invalid line number increment: \u{2018}x\u{2019}"
        );
        assert_eq!(
            bad_number_message(
                "line number of blank lines",
                "0",
                Some("Numerical result out of range")
            ),
            "nl: invalid line number of blank lines: \u{2018}0\u{2019}: Numerical result out of range"
        );
        assert_eq!(
            cannot_open_message("missing", "No such file or directory"),
            "nl: missing: No such file or directory"
        );
        assert_eq!(
            invalid_regex_message("Invalid regular expression"),
            "nl: Invalid regular expression"
        );
    }
}