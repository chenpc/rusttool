//! Comparison and grouping logic of `uniq(1)`, following its manual page:
//!
//! ```text
//! uniq [OPTION]... [INPUT [OUTPUT]]
//! ```
//!
//! The manual is precise about what is compared: a field is a run of blanks then
//! non-blanks, fields are skipped before characters, -w compares no more than N
//! characters, -f and -s skip parts of the line, -i folds case, and the manual
//! also states plainly that uniq only sees adjacent lines.

/// Which comparison options are in force.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Compare {
    /// -f/--skip-fields=N
    pub skip_fields: usize,
    /// -s/--skip-chars=N
    pub skip_chars: usize,
    /// -w/--check-chars=N
    pub check_chars: Option<usize>,
    /// -i/--ignore-case
    pub ignore_case: bool,
}

impl Default for Compare {
    fn default() -> Compare {
        Compare {
            skip_fields: 0,
            skip_chars: 0,
            check_chars: None,
            ignore_case: false,
        }
    }
}

/// The options `uniq` accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub compare: Compare,
    /// -c/--count
    pub count: bool,
    /// -d/--repeated
    pub repeated: bool,
    /// -D
    pub all_repeated: bool,
    /// -u/--unique
    pub unique: bool,
    /// --all-repeated[=METHOD]
    pub all_repeated_method: Option<GroupMethod>,
    /// --group[=METHOD]
    pub group: Option<GroupMethod>,
    /// -z/--zero-terminated
    pub zero_terminated: bool,
}

/// The grouping methods the manual lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupMethod {
    None,
    Prepend,
    Separate,
    Append,
    Both,
}

impl GroupMethod {
    pub fn parse(text: &str) -> Option<GroupMethod> {
        match text {
            "" | "none" => Some(GroupMethod::None),
            "prepend" => Some(GroupMethod::Prepend),
            "separate" => Some(GroupMethod::Separate),
            "append" => Some(GroupMethod::Append),
            "both" => Some(GroupMethod::Both),
            _ => None,
        }
    }

    /// Whether an empty line is printed between two groups.
    pub fn separates(&self) -> bool {
        matches!(
            self,
            GroupMethod::Separate | GroupMethod::Append | GroupMethod::Both | GroupMethod::Prepend
        )
    }

    /// Whether an empty line is printed before the first group.
    pub fn prepends(&self) -> bool {
        matches!(self, GroupMethod::Prepend | GroupMethod::Both)
    }

    /// Whether an empty line is printed after the last group.
    pub fn appends(&self) -> bool {
        matches!(self, GroupMethod::Append | GroupMethod::Both)
    }
}

impl Default for Options {
    fn default() -> Options {
        Options {
            compare: Compare::default(),
            count: false,
            repeated: false,
            all_repeated: false,
            unique: false,
            all_repeated_method: None,
            group: None,
            zero_terminated: false,
        }
    }
}

impl Options {
    /// The --all-repeated method, which defaults to none.
    pub fn all_repeated_method(&self) -> GroupMethod {
        self.all_repeated_method.unwrap_or(GroupMethod::None)
    }

    /// The --group method, which defaults to separate.
    pub fn group_method(&self) -> GroupMethod {
        self.group.unwrap_or(GroupMethod::Separate)
    }
}

/// Why the operands cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    TooManyOperands(String),
    BadNumber(String),
    BadMethod(String, &'static str),
}

/// The part of a line that is compared, after -f and -s have been applied.
pub fn compared(line: &[u8], compare: &Compare) -> Vec<u8> {
    let mut start = 0usize;
    if compare.skip_fields > 0 {
        // Skip whole fields, each of which is a run of blanks then non-blanks.
        let mut fields = 0usize;
        let mut index = 0usize;
        while index < line.len() && fields < compare.skip_fields {
            while index < line.len() && is_blank(line[index]) {
                index += 1;
            }
            while index < line.len() && !is_blank(line[index]) {
                index += 1;
            }
            fields += 1;
        }
        start = index;
    }
    start += compare.skip_chars.min(line.len().saturating_sub(start));
    let mut end = line.len();
    if let Some(limit) = compare.check_chars {
        end = (start + limit).min(line.len());
    }
    let mut slice = line[start.min(line.len())..end].to_vec();
    if compare.ignore_case {
        slice.make_ascii_lowercase();
    }
    slice
}

/// Whether a byte is one of the blanks the manual's field definition uses.
pub fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

/// Whether two lines match under the comparison options.
pub fn same(left: &[u8], right: &[u8], compare: &Compare) -> bool {
    compared(left, compare) == compared(right, compare)
}

/// One run of equal lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group<'a> {
    /// The first line of the run, which is the one printed by default.
    pub first: &'a [u8],
    pub count: usize,
    /// Every line of the run, which -D and --group print.
    pub lines: Vec<&'a [u8]>,
}

/// Split a slice of lines into runs of equal lines.
pub fn group<'a>(lines: &[&'a [u8]], compare: &Compare) -> Vec<Group<'a>> {
    let mut out: Vec<Group<'a>> = Vec::new();
    for line in lines.iter().copied() {
        match out.last_mut() {
            Some(group) if same(group.first, line, compare) => {
                group.count += 1;
                group.lines.push(line);
            }
            _ => out.push(Group {
                first: line,
                count: 1,
                lines: vec![line],
            }),
        }
    }
    out
}

/// What should be printed for one group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Print the first line of the group.
    Line,
    /// Print every line of the group, which is -D and --group.
    AllLines,
    /// Print the first line with its count in front, which is -c.
    Counted,
    /// Print nothing at all.
    Nothing,
}

/// Decide what to print for one group.
///
/// The combinations the manual allows are: the default, -c, -d, -u, -D and
/// --all-repeated, and --group. -u with -D prints both halves, and -c with -D is
/// refused before this point.
pub fn decide(_index: usize, group: &Group<'_>, options: &Options) -> Action {
    let repeated = group.count > 1;

    if options.group.is_some() {
        // --group shows every item of every group.
        return Action::AllLines;
    }

    let all_repeated = options.all_repeated || options.all_repeated_method.is_some();
    if all_repeated {
        if repeated {
            return Action::AllLines;
        }
        // -u with -D keeps the unique lines as well.
        return if options.unique {
            Action::Line
        } else {
            Action::Nothing
        };
    }

    if options.repeated && !repeated {
        return Action::Nothing;
    }
    if options.unique && repeated {
        return Action::Nothing;
    }
    if options.count {
        Action::Counted
    } else {
        Action::Line
    }
}

impl Options {
    /// The group method that decides where the empty lines go: --group and
    /// --all-repeated both take one, with their own defaults.
    pub fn group_method_check(&self) -> Option<GroupMethod> {
        match self.group {
            Some(method) => Some(method),
            None => self
                .all_repeated_method
                .or(if self.all_repeated {
                    Some(GroupMethod::None)
                } else {
                    None
                }),
        }
    }
}

/// The diagnostic -c with -D gets, which the manual's options rule out.
pub fn count_with_all_repeated_message() -> String {
    "uniq: printing all duplicated lines and repeat counts is meaningless".to_string()
}

/// The text -c prints in front of a line.
pub fn count_text(count: usize) -> String {
    format!("{:>7} ", count)
}

/// The line delimiter in force.
pub fn terminator(options: &Options) -> u8 {
    if options.zero_terminated {
        0
    } else {
        b'\n'
    }
}

/// GNU diagnostics. The quotes coreutils puts around an operand are the
/// typographic ones.
pub const LEFT_QUOTE: &str = "\u{2018}";
pub const RIGHT_QUOTE: &str = "\u{2019}";

pub fn quoted(text: &str) -> String {
    format!("{}{}{}", LEFT_QUOTE, text, RIGHT_QUOTE)
}

pub fn too_many_operands_message(operand: &str) -> String {
    format!("uniq: extra operand {}", quoted(operand))
}

pub fn invalid_skip_message(text: &str) -> String {
    format!("uniq: {}: invalid number of fields to skip", text)
}

pub fn invalid_check_chars_message(text: &str) -> String {
    format!("uniq: {}: invalid number of characters to compare", text)
}

/// The invalid-method diagnostic lists what would have been accepted.
pub fn invalid_method_lines(value: &str, option: &str) -> Vec<String> {
    vec![
        format!(
            "uniq: invalid argument {} for {}",
            quoted(value),
            quoted(option)
        ),
        "Valid arguments are:".to_string(),
        format!("  - {}", quoted("prepend")),
        format!("  - {}", quoted("append")),
        format!("  - {}", quoted("separate")),
        format!("  - {}", quoted("both")),
    ]
}

pub fn cannot_open_message(file: &str, reason: &str) -> String {
    format!("uniq: {}: {}", file, reason)
}

pub fn cannot_write_message(file: &str, reason: &str) -> String {
    format!("uniq: {}: {}", file, reason)
}

pub fn unrecognized_option_message(option: &str) -> String {
    format!("uniq: unrecognized option '{}'", option)
}

pub fn invalid_option_message(letter: char) -> String {
    format!("uniq: invalid option -- '{}'", letter)
}

pub fn requires_argument_message(letter: char) -> String {
    format!("uniq: option requires an argument -- '{}'", letter)
}

pub fn try_help_message() -> String {
    "Try 'uniq --help' for more information.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines() -> Vec<Vec<u8>> {
        vec![
            b"a one".to_vec(),
            b"a two".to_vec(),
            b"a one".to_vec(),
            b"b".to_vec(),
            b"b".to_vec(),
            b"c".to_vec(),
        ]
    }

    #[test]
    fn only_adjacent_lines_join_a_group() {
        // The fixture holds one adjacent pair, so the six lines make five groups.
        let input: Vec<Vec<u8>> = lines();
        let borrowed: Vec<&[u8]> = input.iter().map(|line| line.as_slice()).collect();
        let groups = group(&borrowed, &Compare::default());
        assert_eq!(groups.len(), 5);
        assert_eq!(groups[0].count, 1);
        assert_eq!(groups[3].count, 2);
    }

    #[test]
    fn adjacent_equal_lines_form_a_group() {
        let input: Vec<Vec<u8>> = vec![b"a".to_vec(), b"a".to_vec(), b"b".to_vec()];
        let borrowed: Vec<&[u8]> = input.iter().map(|line| line.as_slice()).collect();
        let groups = group(&borrowed, &Compare::default());
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].count, 2);
        assert_eq!(groups[0].lines.len(), 2);
        assert_eq!(groups[1].count, 1);
    }

    #[test]
    fn non_adjacent_lines_are_not_a_group() {
        // The manual says this plainly: uniq does not see repeats that are not
        // adjacent.
        let input: Vec<Vec<u8>> = vec![b"a".to_vec(), b"b".to_vec(), b"a".to_vec()];
        let borrowed: Vec<&[u8]> = input.iter().map(|line| line.as_slice()).collect();
        let groups = group(&borrowed, &Compare::default());
        assert_eq!(groups.len(), 3);
    }

    #[test]
    fn skip_fields_ignores_the_leading_fields() {
        // -f 1 compares what follows the first field, so "a one" and "b one"
        // match while "a one" and "a two" do not.
        let compare = Compare {
            skip_fields: 1,
            ..Compare::default()
        };
        assert!(same(b"a one", b"b one", &compare));
        assert!(!same(b"a one", b"a two", &compare));
        assert!(!same(b"a one", b"b one more", &compare));
    }

    #[test]
    fn skip_chars_ignores_the_leading_characters() {
        // Skipping the first two characters leaves "-one" in both lines.
        let compare = Compare {
            skip_chars: 2,
            ..Compare::default()
        };
        assert!(same(b"ab-one", b"ac-one", &compare));
        // Skipping one leaves "b-one" against "c-one", which differ.
        let compare = Compare {
            skip_chars: 1,
            ..Compare::default()
        };
        assert!(!same(b"ab-one", b"ac-one", &compare));
    }

    #[test]
    fn check_chars_limits_the_comparison() {
        let compare = Compare {
            check_chars: Some(1),
            ..Compare::default()
        };
        assert!(same(b"a one", b"a two", &compare));
        assert!(!same(b"a one", b"b one", &compare));
    }

    #[test]
    fn fields_are_skipped_before_characters() {
        // -f 1 skips the first field, so "a one" and "b one" compare equal on
        // what is left.
        let compare = Compare {
            skip_fields: 1,
            ..Compare::default()
        };
        assert!(same(b"a one", b"b one", &compare));
        assert!(!same(b"a one", b"b two", &compare));

        // -f 1 -s 1 skips the field and one more character, so the "e" of
        // "one" is what is left.
        let compare = Compare {
            skip_fields: 1,
            skip_chars: 1,
            ..Compare::default()
        };
        assert!(same(b"a one", b"b one", &compare));
        assert!(!same(b"a one", b"b two", &compare));
    }

    #[test]
    fn ignore_case_folds_the_comparison() {
        let compare = Compare {
            ignore_case: true,
            ..Compare::default()
        };
        assert!(same(b"ABC", b"abc", &compare));
        assert!(!same(b"ABC", b"abd", &compare));
    }

    #[test]
    fn the_default_prints_the_first_line_of_a_group() {
        let input: Vec<Vec<u8>> = vec![b"a".to_vec(), b"a".to_vec()];
        let borrowed: Vec<&[u8]> = input.iter().map(|line| line.as_slice()).collect();
        let groups = group(&borrowed, &Compare::default());
        assert_eq!(decide(0, &groups[0], &Options::default()), Action::Line);
    }

    #[test]
    fn count_prefixes_the_line() {
        let input: Vec<Vec<u8>> = vec![b"a".to_vec(), b"a".to_vec(), b"b".to_vec()];
        let borrowed: Vec<&[u8]> = input.iter().map(|line| line.as_slice()).collect();
        let groups = group(&borrowed, &Compare::default());
        let options = Options {
            count: true,
            ..Options::default()
        };
        assert_eq!(decide(0, &groups[0], &options), Action::Counted);
        assert_eq!(count_text(2), "      2 ");
    }

    #[test]
    fn repeated_prints_only_the_duplicates() {
        let input: Vec<Vec<u8>> = vec![b"a".to_vec(), b"a".to_vec(), b"b".to_vec()];
        let borrowed: Vec<&[u8]> = input.iter().map(|line| line.as_slice()).collect();
        let groups = group(&borrowed, &Compare::default());
        let options = Options {
            repeated: true,
            ..Options::default()
        };
        assert_eq!(decide(0, &groups[0], &options), Action::Line);
        assert_eq!(decide(1, &groups[1], &options), Action::Nothing);
    }

    #[test]
    fn unique_prints_only_the_lonely_lines() {
        let input: Vec<Vec<u8>> = vec![b"a".to_vec(), b"a".to_vec(), b"b".to_vec()];
        let borrowed: Vec<&[u8]> = input.iter().map(|line| line.as_slice()).collect();
        let groups = group(&borrowed, &Compare::default());
        let options = Options {
            unique: true,
            ..Options::default()
        };
        assert_eq!(decide(0, &groups[0], &options), Action::Nothing);
        assert_eq!(decide(1, &groups[1], &options), Action::Line);
    }

    #[test]
    fn all_repeated_prints_every_line_of_the_repeats() {
        let input: Vec<Vec<u8>> = vec![b"a".to_vec(), b"a".to_vec(), b"b".to_vec()];
        let borrowed: Vec<&[u8]> = input.iter().map(|line| line.as_slice()).collect();
        let groups = group(&borrowed, &Compare::default());
        let options = Options {
            all_repeated: true,
            ..Options::default()
        };
        assert_eq!(decide(0, &groups[0], &options), Action::AllLines);
        assert_eq!(decide(1, &groups[1], &options), Action::Nothing);
    }

    #[test]
    fn unique_with_all_repeated_keeps_both_halves() {
        let input: Vec<Vec<u8>> = vec![b"a".to_vec(), b"b".to_vec(), b"b".to_vec()];
        let borrowed: Vec<&[u8]> = input.iter().map(|line| line.as_slice()).collect();
        let groups = group(&borrowed, &Compare::default());
        let options = Options {
            all_repeated: true,
            unique: true,
            ..Options::default()
        };
        assert_eq!(decide(0, &groups[0], &options), Action::Line);
        assert_eq!(decide(1, &groups[1], &options), Action::AllLines);
    }

    #[test]
    fn group_shows_everything() {
        let input: Vec<Vec<u8>> = vec![b"a".to_vec(), b"a".to_vec()];
        let borrowed: Vec<&[u8]> = input.iter().map(|line| line.as_slice()).collect();
        let groups = group(&borrowed, &Compare::default());
        let options = Options {
            group: Some(GroupMethod::Separate),
            ..Options::default()
        };
        assert_eq!(decide(0, &groups[0], &options), Action::AllLines);
    }

    #[test]
    fn the_group_methods_place_the_empty_lines() {
        // separate: between the groups only.
        assert!(GroupMethod::Separate.separates());
        assert!(!GroupMethod::Separate.prepends());
        assert!(!GroupMethod::Separate.appends());
        // prepend: before and between.
        assert!(GroupMethod::Prepend.separates());
        assert!(GroupMethod::Prepend.prepends());
        assert!(!GroupMethod::Prepend.appends());
        // append: between and after.
        assert!(GroupMethod::Append.separates());
        assert!(!GroupMethod::Append.prepends());
        assert!(GroupMethod::Append.appends());
        // both: everywhere.
        assert!(GroupMethod::Both.separates());
        assert!(GroupMethod::Both.prepends());
        assert!(GroupMethod::Both.appends());
        assert!(!GroupMethod::None.separates());
    }

    #[test]
    fn the_group_methods_parse() {
        assert_eq!(GroupMethod::parse(""), Some(GroupMethod::None));
        assert_eq!(GroupMethod::parse("none"), Some(GroupMethod::None));
        assert_eq!(GroupMethod::parse("prepend"), Some(GroupMethod::Prepend));
        assert_eq!(GroupMethod::parse("separate"), Some(GroupMethod::Separate));
        assert_eq!(GroupMethod::parse("append"), Some(GroupMethod::Append));
        assert_eq!(GroupMethod::parse("both"), Some(GroupMethod::Both));
        assert_eq!(GroupMethod::parse("nope"), None);
    }

    #[test]
    fn the_terminator_follows_the_option() {
        assert_eq!(terminator(&Options::default()), b'\n');
        let options = Options {
            zero_terminated: true,
            ..Options::default()
        };
        assert_eq!(terminator(&options), 0);
    }

    #[test]
    fn messages_match_coreutils() {
        assert_eq!(
            too_many_operands_message("extra"),
            "uniq: extra operand \u{2018}extra\u{2019}"
        );
        assert_eq!(
            invalid_skip_message("-1"),
            "uniq: -1: invalid number of fields to skip"
        );
        assert_eq!(invalid_option_message('Z'), "uniq: invalid option -- 'Z'");
        assert_eq!(invalid_method_lines("nope", "--group").len(), 6);
    }
}