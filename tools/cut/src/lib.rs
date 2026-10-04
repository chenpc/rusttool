//! Selection logic of `cut(1)`, following its manual page:
//!
//! ```text
//! cut OPTION... [FILE]...
//! ```
//!
//! The manual is explicit about the parts that are easy to get wrong: use one
//! and only one of -b, -c or -f; a LIST is ranges of the forms `N`, `N-`, `N-M`
//! and `-M`; selected input is written in the order it is read and exactly
//! once; and with -f a line that contains no delimiter is printed anyway,
//! unless -s was given.

/// Which unit a LIST counts in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// -b/--bytes
    Bytes,
    /// -c/--characters
    Characters,
    /// -f/--fields
    Fields,
}

/// The options `cut` accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub unit: Option<Unit>,
    /// -d/--delimiter=DELIM
    pub delimiter: Option<u8>,
    /// --complement
    pub complement: bool,
    /// -s/--only-delimited
    pub only_delimited: bool,
    /// --output-delimiter=STRING
    pub output_delimiter: Option<Vec<u8>>,
    /// -z/--zero-terminated
    pub zero_terminated: bool,
    pub files: Vec<String>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            unit: None,
            delimiter: None,
            complement: false,
            only_delimited: false,
            output_delimiter: None,
            zero_terminated: false,
            files: Vec::new(),
        }
    }
}

/// Why an operand list is not usable. The wording differs between -f and
/// -b/-c, so the variants carry what is needed to say it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// A range is written backwards, as in 3-1.
    DecreasingRange,
    /// A piece of the LIST is not a number at all.
    BadValue(String, Unit),
    /// A piece looks like a range but is not one.
    BadRange(Unit),
    /// A dash with nothing on one side of it.
    RangeWithNoEndpoint(String),
    /// The LIST is empty, or a position is zero.
    NumberedFromOne(Unit),
    /// Neither -b, -c nor -f was given.
    NoUnit,
    /// More than one of -b, -c and -f was given.
    SeveralUnits,
    /// -d takes exactly one byte.
    BadDelimiter,
}

/// One range of a LIST, with 1-based bounds and `usize::MAX` for "to the end".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub from: usize,
    pub to: usize,
}

impl Range {
    fn contains(&self, index: usize) -> bool {
        index + 1 >= self.from && index + 1 <= self.to
    }
}

/// Parse a LIST: comma-separated ranges of `N`, `N-`, `N-M` and `-M`.
///
/// `unit` only decides the wording of the diagnostics, which coreutils words
/// differently for fields and for bytes or characters.
pub fn parse_list(list: &str, unit: Unit) -> Result<Vec<Range>, Problem> {
    // An empty LIST is the "numbered from 1" complaint, since there is no
    // position in it at all.
    if list.is_empty() {
        return Err(Problem::NumberedFromOne(unit));
    }
    let mut ranges = Vec::new();
    for item in list.split(',') {
        ranges.push(parse_range(item, unit)?);
    }
    if ranges.is_empty() {
        return Err(Problem::NumberedFromOne(unit));
    }
    Ok(ranges)
}

/// One item of a LIST: `N`, `N-`, `N-M` or `-M`.
fn parse_range(item: &str, unit: Unit) -> Result<Range, Problem> {
    // A lone dash has an endpoint missing on both sides, and an empty item
    // comes from a stray comma.
    if item == "-" {
        return Err(Problem::RangeWithNoEndpoint(item.to_string()));
    }
    if item.is_empty() {
        return Err(Problem::NumberedFromOne(unit));
    }
    let is_number = |text: &str| -> Option<usize> {
        match text.parse::<usize>() {
            Ok(number) if number >= 1 => Some(number),
            _ => None,
        }
    };
    match item.split_once('-') {
        // N: a position on its own.
        None => {
            let number = number_or_bad(item, unit)?;
            Ok(Range {
                from: number,
                to: number,
            })
        }
        // -M: from the first position to M.
        Some(("", to)) => match is_number(to) {
            Some(to) => Ok(Range { from: 1, to }),
            // An empty right side means the item was a bare dash, which is
            // handled above; anything else is not a range.
            None if to.is_empty() => Err(Problem::RangeWithNoEndpoint(item.to_string())),
            None => Err(Problem::BadRange(unit)),
        },
        // N-: from N to the end.
        Some((from, "")) => match is_number(from) {
            Some(from) => Ok(Range {
                from,
                to: usize::MAX,
            }),
            None if from.is_empty() => Err(Problem::RangeWithNoEndpoint(item.to_string())),
            None => Err(Problem::BadRange(unit)),
        },
        // N-M.
        Some((from, to)) => match (is_number(from), is_number(to)) {
            (Some(from), Some(to)) => {
                if from > to {
                    return Err(Problem::DecreasingRange);
                }
                Ok(Range { from, to })
            }
            _ => Err(Problem::BadRange(unit)),
        },
    }
}

/// One position of a LIST: a number, or the reason it is not one.
fn number_or_bad(text: &str, unit: Unit) -> Result<usize, Problem> {
    match text.parse::<usize>() {
        Ok(number) if number >= 1 => Ok(number),
        Ok(_) => Err(Problem::NumberedFromOne(unit)),
        Err(_) => Err(Problem::BadValue(text.to_string(), unit)),
    }
}

/// Parse one number of a LIST; zero is refused because the manual numbers
/// positions from 1.
pub fn parse_number(text: &str) -> Option<usize> {
    let number: usize = text.parse().ok()?;
    if number == 0 {
        return None;
    }
    Some(number)
}

/// The delimiter a run uses: the one -d gave, or TAB.
pub fn delimiter_of(options: &Options) -> u8 {
    options.delimiter.unwrap_or(b'\t')
}

/// What should happen to one line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// Print the selected parts, joined by the output delimiter.
    Selected(Vec<Vec<u8>>),
    /// The line has no delimiter: -f prints it whole, unless -s was given.
    Undelimited(Vec<u8>),
    /// Skip it, which is what -s asks for.
    Skip,
}

/// Apply the selection to one line.
pub fn select(line: &[u8], ranges: &[Range], options: &Options) -> Line {
    let complement = options.complement;
    let wanted = |index: usize| ranges.iter().any(|range| range.contains(index)) != complement;

    match options.unit {
        Some(Unit::Fields) => {
            let delimiter = delimiter_of(options);
            if !line.contains(&delimiter) {
                return if options.only_delimited {
                    Line::Skip
                } else {
                    Line::Undelimited(line.to_vec())
                };
            }
            let mut selected = Vec::new();
            for (index, field) in line.split(|byte| *byte == delimiter).enumerate() {
                if wanted(index) {
                    selected.push(field.to_vec());
                }
            }
            Line::Selected(selected)
        }
        Some(Unit::Bytes) | Some(Unit::Characters) => {
            // In the C locale a byte is a character, which is the case the
            // manual's BUGS section describes as safe.
            let mut selected = Vec::new();
            for (index, byte) in line.iter().enumerate() {
                if wanted(index) {
                    selected.push(vec![*byte]);
                }
            }
            Line::Selected(selected)
        }
        None => Line::Skip,
    }
}

/// Join the selected parts with the output delimiter.
pub fn join(parts: &[Vec<u8>], options: &Options) -> Vec<u8> {
    // The manual says the default output delimiter is the input delimiter,
    // which for -b and -c is nothing at all.
    let mut separator: &[u8] = match &options.output_delimiter {
        Some(delimiter) => delimiter,
        None => &[],
    };
    let mut fallback = delimiter_of(options);
    if options.output_delimiter.is_none() && options.unit == Some(Unit::Fields) {
        separator = std::slice::from_mut(&mut fallback);
    }
    let mut out = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            out.extend_from_slice(separator);
        }
        out.extend_from_slice(part);
    }
    out
}

/// The line delimiter in force.
pub fn line_terminator(options: &Options) -> u8 {
    if options.zero_terminated {
        0
    } else {
        b'\n'
    }
}

/// GNU diagnostics.
pub const LEFT_QUOTE: &str = "\u{2018}";
pub const RIGHT_QUOTE: &str = "\u{2019}";

pub fn quoted(text: &str) -> String {
    format!("{}{}{}", LEFT_QUOTE, text, RIGHT_QUOTE)
}

pub fn missing_unit_message() -> String {
    "cut: you must specify a list of bytes, characters, or fields".to_string()
}

pub fn several_units_message() -> String {
    "cut: only one list may be specified".to_string()
}

pub fn decreasing_range_message() -> String {
    "cut: invalid decreasing range".to_string()
}

/// "invalid byte or character range" for -b/-c, "invalid field range" for -f.
pub fn bad_range_message(unit: Unit) -> String {
    match unit {
        Unit::Fields => "cut: invalid field range".to_string(),
        _ => "cut: invalid byte or character range".to_string(),
    }
}

pub fn complement_needs_range_message() -> String {
    "cut: invalid byte or character range".to_string()
}

/// "fields are numbered from 1" for -f, "byte/character positions ..." for -b/-c.
pub fn numbered_from_one_message(unit: Unit) -> String {
    match unit {
        Unit::Fields => "cut: fields are numbered from 1".to_string(),
        _ => "cut: byte/character positions are numbered from 1".to_string(),
    }
}

pub fn bad_value_message(value: &str, unit: Unit) -> String {
    match unit {
        Unit::Fields => format!("cut: invalid field value {}", quoted(value)),
        _ => format!("cut: invalid byte/character position {}", quoted(value)),
    }
}

pub fn range_with_no_endpoint_message(text: &str) -> String {
    format!("cut: invalid range with no endpoint: {}", text)
}

pub fn bad_delimiter_message() -> String {
    "cut: the delimiter must be a single character".to_string()
}

pub fn unrecognized_option_message(option: &str) -> String {
    format!("cut: unrecognized option '{}'", option)
}

pub fn invalid_option_message(letter: char) -> String {
    format!("cut: invalid option -- '{}'", letter)
}

pub fn requires_argument_message(letter: char) -> String {
    format!("cut: option requires an argument -- '{}'", letter)
}

pub fn try_help_message() -> String {
    "Try 'cut --help' for more information.".to_string()
}

pub fn cannot_open_message(file: &str, reason: &str) -> String {
    format!("cut: {}: {}", file, reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field_options() -> Options {
        Options {
            unit: Some(Unit::Fields),
            ..Options::default()
        }
    }

    fn text_options() -> Options {
        Options {
            unit: Some(Unit::Characters),
            ..Options::default()
        }
    }

    fn selected(ranges: &[Range], options: &Options, line: &[u8]) -> Vec<String> {
        match select(line, ranges, options) {
            Line::Selected(parts) => parts
                .iter()
                .map(|part| String::from_utf8_lossy(part).into_owned())
                .collect(),
            other => panic!("expected a selection, got {:?}", other),
        }
    }

    #[test]
    fn a_single_number_is_its_own_range() {
        assert_eq!(
            parse_list("3", Unit::Fields).unwrap(),
            vec![Range { from: 3, to: 3 }]
        );
    }

    #[test]
    fn the_four_documented_forms_parse() {
        assert_eq!(
            parse_list("2-", Unit::Fields).unwrap(),
            vec![Range { from: 2, to: usize::MAX }]
        );
        assert_eq!(
            parse_list("2-4", Unit::Fields).unwrap(),
            vec![Range { from: 2, to: 4 }]
        );
        assert_eq!(
            parse_list("-3", Unit::Fields).unwrap(),
            vec![Range { from: 1, to: 3 }]
        );
        assert_eq!(
            parse_list("1,3,5-7", Unit::Fields).unwrap(),
            vec![
                Range { from: 1, to: 1 },
                Range { from: 3, to: 3 },
                Range { from: 5, to: 7 },
            ]
        );
    }

    #[test]
    fn zero_and_junk_are_refused() {
        assert_eq!(
            parse_list("0", Unit::Fields).unwrap_err(),
            Problem::NumberedFromOne(Unit::Fields)
        );
        assert_eq!(
            parse_list("a", Unit::Fields).unwrap_err(),
            Problem::BadValue("a".into(), Unit::Fields)
        );
        assert_eq!(
            parse_list("a", Unit::Characters).unwrap_err(),
            Problem::BadValue("a".into(), Unit::Characters)
        );
        assert_eq!(
            parse_list("1,,2", Unit::Fields).unwrap_err(),
            Problem::NumberedFromOne(Unit::Fields)
        );
        assert_eq!(parse_number("0"), None);
    }

    #[test]
    fn a_reversed_range_is_refused() {
        assert_eq!(
            parse_list("4-2", Unit::Fields).unwrap_err(),
            Problem::DecreasingRange
        );
    }

    #[test]
    fn a_range_that_is_not_one_is_refused() {
        // "1-2-3" and "--complement" are both range-shaped and neither is one.
        assert_eq!(
            parse_list("1-2-3", Unit::Characters).unwrap_err(),
            Problem::BadRange(Unit::Characters)
        );
        assert_eq!(
            parse_list("--complement", Unit::Characters).unwrap_err(),
            Problem::BadRange(Unit::Characters)
        );
        assert_eq!(
            parse_list("--complement", Unit::Fields).unwrap_err(),
            Problem::BadRange(Unit::Fields)
        );
    }

    #[test]
    fn a_lone_dash_is_reported_on_its_own() {
        assert_eq!(
            parse_list("-", Unit::Characters).unwrap_err(),
            Problem::RangeWithNoEndpoint("-".into())
        );
    }

    #[test]
    fn the_wording_follows_the_unit() {
        assert_eq!(
            bad_value_message("x", Unit::Fields),
            "cut: invalid field value \u{2018}x\u{2019}"
        );
        assert_eq!(
            bad_value_message("x", Unit::Characters),
            "cut: invalid byte/character position \u{2018}x\u{2019}"
        );
        assert_eq!(bad_range_message(Unit::Fields), "cut: invalid field range");
        assert_eq!(
            bad_range_message(Unit::Bytes),
            "cut: invalid byte or character range"
        );
        assert_eq!(
            numbered_from_one_message(Unit::Characters),
            "cut: byte/character positions are numbered from 1"
        );
    }

    #[test]
    fn fields_are_selected_in_input_order() {
        let ranges = parse_list("3,1", Unit::Fields).unwrap();
        assert_eq!(
            selected(&ranges, &field_options(), b"a\tb\tc"),
            vec!["a", "c"]
        );
    }

    #[test]
    fn a_field_appears_once_even_when_ranges_overlap() {
        let ranges = parse_list("1-2,2-3", Unit::Fields).unwrap();
        assert_eq!(
            selected(&ranges, &field_options(), b"a\tb\tc\td"),
            vec!["a", "b", "c"]
        );
    }

    #[test]
    fn a_line_without_a_delimiter_is_printed_whole() {
        let ranges = parse_list("1", Unit::Fields).unwrap();
        assert_eq!(
            select(b"lonely", &ranges, &field_options()),
            Line::Undelimited(b"lonely".to_vec())
        );
    }

    #[test]
    fn only_delimited_skips_those_lines() {
        let ranges = parse_list("1", Unit::Fields).unwrap();
        let options = Options {
            only_delimited: true,
            ..field_options()
        };
        assert_eq!(select(b"lonely", &ranges, &options), Line::Skip);
    }

    #[test]
    fn a_custom_delimiter_is_used() {
        let ranges = parse_list("2", Unit::Fields).unwrap();
        let options = Options {
            delimiter: Some(b':'),
            ..field_options()
        };
        assert_eq!(selected(&ranges, &options, b"a:b:c"), vec!["b"]);
    }

    #[test]
    fn the_output_delimiter_defaults_to_the_input_one() {
        let ranges = parse_list("1-2", Unit::Fields).unwrap();
        let options = Options {
            delimiter: Some(b':'),
            ..field_options()
        };
        let parts = match select(b"a:b:c", &ranges, &options) {
            Line::Selected(parts) => parts,
            other => panic!("expected a selection, got {:?}", other),
        };
        assert_eq!(join(&parts, &options), b"a:b".to_vec());
    }

    #[test]
    fn an_explicit_output_delimiter_wins() {
        let ranges = parse_list("1-2", Unit::Fields).unwrap();
        let options = Options {
            delimiter: Some(b':'),
            output_delimiter: Some(b",".to_vec()),
            ..field_options()
        };
        let parts = match select(b"a:b:c", &ranges, &options) {
            Line::Selected(parts) => parts,
            other => panic!("expected a selection, got {:?}", other),
        };
        assert_eq!(join(&parts, &options), b"a,b".to_vec());
    }

    #[test]
    fn characters_and_bytes_do_not_use_a_delimiter() {
        let ranges = parse_list("1,3", Unit::Fields).unwrap();
        assert_eq!(selected(&ranges, &text_options(), b"abc"), vec!["a", "c"]);
    }

    #[test]
    fn complement_selects_the_rest() {
        let ranges = parse_list("2", Unit::Fields).unwrap();
        let options = Options {
            complement: true,
            ..field_options()
        };
        assert_eq!(
            selected(&ranges, &options, b"a\tb\tc"),
            vec!["a", "c"]
        );
    }

    #[test]
    fn an_open_range_runs_to_the_end() {
        let ranges = parse_list("2-", Unit::Fields).unwrap();
        assert_eq!(
            selected(&ranges, &field_options(), b"a\tb\tc\td"),
            vec!["b", "c", "d"]
        );
    }

    #[test]
    fn past_the_end_selects_nothing() {
        let ranges = parse_list("9", Unit::Fields).unwrap();
        assert!(selected(&ranges, &field_options(), b"a\tb").is_empty());
    }

    #[test]
    fn the_line_terminator_follows_the_option() {
        assert_eq!(line_terminator(&Options::default()), b'\n');
        let zero = Options {
            zero_terminated: true,
            ..Options::default()
        };
        assert_eq!(line_terminator(&zero), 0);
    }

    #[test]
    fn the_default_delimiter_is_tab() {
        assert_eq!(delimiter_of(&Options::default()), b'\t');
        let options = Options {
            delimiter: Some(b','),
            ..Options::default()
        };
        assert_eq!(delimiter_of(&options), b',');
    }

    #[test]
    fn messages_match_coreutils() {
        assert_eq!(
            missing_unit_message(),
            "cut: you must specify a list of bytes, characters, or fields"
        );
        assert_eq!(several_units_message(), "cut: only one list may be specified");
        assert_eq!(decreasing_range_message(), "cut: invalid decreasing range");
        assert_eq!(
            numbered_from_one_message(Unit::Fields),
            "cut: fields are numbered from 1"
        );
        assert_eq!(
            bad_value_message("z", Unit::Fields),
            "cut: invalid field value \u{2018}z\u{2019}"
        );
        assert_eq!(
            bad_delimiter_message(),
            "cut: the delimiter must be a single character"
        );
    }
}