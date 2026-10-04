//! Array logic of `tr(1)`, following its manual page:
//!
//! ```text
//! tr [OPTION]... STRING1 [STRING2]
//! ```
//!
//! A STRING is not a plain list of characters: it can carry octal escapes, the
//! usual backslash escapes, `CHAR1-CHAR2` ranges, the class names in brackets,
//! the equivalence class `[=C=]` and the repeats `[C*n]`. What ARRAY1 and ARRAY2
//! mean then depends on which of -d, -s and -t were given.

/// The options `tr` accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// -c/-C/--complement
    pub complement: bool,
    /// -d/--delete
    pub delete: bool,
    /// -s/--squeeze-repeats
    pub squeeze: bool,
    /// -t/--truncate-set1
    pub truncate: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            complement: false,
            delete: false,
            squeeze: false,
            truncate: false,
        }
    }
}

/// Why a STRING cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// A class name that does not exist.
    BadClass(String),
    /// A range whose end comes before its start.
    BadRange(String),
}

/// Why the operands do not fit the options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperandProblem {
    /// No STRING at all.
    Missing,
    /// One STRING while translating, which needs a second one.
    MissingAfterTranslating(String),
    /// One STRING with -d and -s, which need two.
    MissingAfterDeletingAndSqueezing(String),
    /// One STRING too many, naming the first extra one.
    Extra(String),
    /// -d with two STRINGs and no -s.
    ExtraWhileDeleting(String),
}

/// Expand one STRING into the bytes of ARRAY1 or ARRAY2.
///
/// `for_set2` turns on the two forms that only exist in ARRAY2: `[C*]` pads with
/// copies of C, and `[C*n]` repeats.
pub fn expand(text: &str, for_set2: bool) -> Result<Vec<u8>, Problem> {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'\\' => {
                index += 1;
                if index >= bytes.len() {
                    return Err(Problem::BadClass(text.to_string()));
                }
                let escaped = bytes[index];
                index += 1;
                match escaped {
                    // \NNN is one to three octal digits.
                    b'0'..=b'7' => {
                        let mut value = (escaped - b'0') as u32;
                        let mut digits = 1;
                        while digits < 3 && index < bytes.len() && matches!(bytes[index], b'0'..=b'7')
                        {
                            value = value * 8 + (bytes[index] - b'0') as u32;
                            index += 1;
                            digits += 1;
                        }
                        out.push(value as u8);
                    }
                    // The manual lists these escapes; anything else keeps both
                    // characters, which is what GNU tr does.
                    other => match other {
                        b'a' => out.push(0x07),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0c),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'v' => out.push(0x0b),
                        b'\\' => out.push(b'\\'),
                        _ => {
                            out.push(b'\\');
                            out.push(other);
                        }
                    },
                }
            }
            b'[' => {
                let (expanded, next) = expand_bracket(text, index, for_set2)?;
                out.extend_from_slice(&expanded);
                index = next;
            }
            _ => {
                // CHAR1-CHAR2: a dash with a character on both sides.
                if index + 2 < bytes.len() && bytes[index + 1] == b'-' {
                    let (upper, after) = read_character(text, index + 2)?;
                    if byte <= upper {
                        for value in byte..=upper {
                            out.push(value);
                        }
                        index = after;
                        continue;
                    }
                    return Err(Problem::BadRange(format!("{}-{}", byte as char, upper as char)));
                }
                out.push(byte);
                index += 1;
            }
        }
    }
    Ok(out)
}

/// Read one character, which may be an escape, starting at `index`.
fn read_character(text: &str, index: usize) -> Result<(u8, usize), Problem> {
    let bytes = text.as_bytes();
    if index >= bytes.len() {
        return Err(Problem::BadClass(text.to_string()));
    }
    if bytes[index] == b'\\' {
        let expanded = expand(&text[index..index + 2.min(text.len() - index)], false)?;
        return match expanded.first() {
            Some(byte) => Ok((*byte, index + 2)),
            None => Err(Problem::BadClass(text.to_string())),
        };
    }
    Ok((bytes[index], index + 1))
}

/// Expand a bracketed construct, returning the bytes and where to continue.
fn expand_bracket(text: &str, start: usize, for_set2: bool) -> Result<(Vec<u8>, usize), Problem> {
    let bytes = text.as_bytes();
    if start + 1 >= bytes.len() {
        return Err(Problem::BadClass(text.to_string()));
    }
    let kind = bytes[start + 1];
    match kind {
        b':' => {
            // [:class:]
            let close = find_close(text, start + 2, b':')
                .ok_or_else(|| Problem::BadClass(text[start..].to_string()))?;
            let name = &text[start + 2..close];
            let class = class_members(name).ok_or_else(|| Problem::BadClass(name.to_string()))?;
            Ok((class, close + 2))
        }
        b'=' => {
            // [=C=]: the characters equivalent to C.
            let close = find_close(text, start + 2, b'=').ok_or_else(|| {
                Problem::BadClass(text[start..].to_string())
            })?;
            let name = &text[start + 2..close];
            // In the C locale a character is equivalent to itself.
            if name.chars().count() != 1 {
                return Err(Problem::BadClass(name.to_string()));
            }
            Ok((name.as_bytes().to_vec(), close + 2))
        }
        _ => {
            // [C*n] and [C*]: only meaningful in ARRAY2, as the manual says.
            let close = find_single(text, start + 1, b']')
                .ok_or_else(|| Problem::BadClass(text[start..].to_string()))?;
            let body = &text[start + 1..close];
            let (repeat, character) = split_repeat(body)
                .ok_or_else(|| Problem::BadClass(text[start..].to_string()))?;
            let expanded = expand(character, false)?;
            let byte = *expanded
                .first()
                .ok_or_else(|| Problem::BadClass(text[start..].to_string()))?;
            let count = match repeat {
                Some(count) if for_set2 => count,
                // The manual puts both repeat forms in ARRAY2 only.
                Some(_) => return Err(Problem::BadClass(text[start..].to_string())),
                None => {
                    if for_set2 {
                        1
                    } else {
                        return Err(Problem::BadClass(text[start..].to_string()));
                    }
                }
            };
            Ok((vec![byte; count], close + 1))
        }
    }
}

/// Find the closing part of a bracketed construct: the `close` character
/// followed by `]`, as in `:]` for a class or `=]` for an equivalence class.
fn find_close(text: &str, from: usize, close: u8) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = from;
    while index + 1 < bytes.len() {
        if bytes[index] == close && bytes[index + 1] == b']' {
            return Some(index);
        }
        index += 1;
    }
    None
}

/// Find a single closing character, which is what `[C*n]` ends with.
fn find_single(text: &str, from: usize, close: u8) -> Option<usize> {
    text.as_bytes()[from..]
        .iter()
        .position(|byte| *byte == close)
        .map(|offset| from + offset)
}

/// Split `C3` or `C*` into the repeat count and the character.
///
/// The manual's forms are `[CHAR*N]` and `[CHAR*NREPEAT]`, so the character comes
/// first and REPEAT, octal when it starts with a zero, comes after it.
fn split_repeat(body: &str) -> Option<(Option<usize>, &str)> {
    let split = body
        .char_indices()
        .find(|(_, c)| c.is_ascii_digit() || *c == '*')
        .map(|(index, _)| index)
        .unwrap_or(body.len());
    let (character, rest) = body.split_at(split);
    if character.is_empty() {
        return None;
    }
    let rest = rest.strip_prefix('*').unwrap_or(rest);
    if rest.is_empty() {
        // [C*] means "until ARRAY1 is long enough", which the caller decides.
        return Some((None, character));
    }
    if !rest.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    // REPEAT is octal, as the manual says.
    let count = usize::from_str_radix(rest, 8).ok()?;
    Some((Some(count), character))
}

/// The members of a character class, in the C locale the manual's BUGS section
/// describes as the safe case.
pub fn class_members(name: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let push = |out: &mut Vec<u8>, from: u8, to: u8| {
        for value in from..=to {
            out.push(value);
        }
    };
    match name {
        "alnum" => {
            push(&mut out, b'0', b'9');
            push(&mut out, b'A', b'Z');
            push(&mut out, b'a', b'z');
        }
        "alpha" => {
            push(&mut out, b'A', b'Z');
            push(&mut out, b'a', b'z');
        }
        "blank" => {
            push(&mut out, b'\t', b'\t');
            push(&mut out, b' ', b' ');
        }
        "cntrl" => push(&mut out, 0x00, 0x1f),
        "digit" => push(&mut out, b'0', b'9'),
        "graph" => push(&mut out, 0x21, 0x7e),
        "lower" => push(&mut out, b'a', b'z'),
        "print" => push(&mut out, 0x20, 0x7e),
        "punct" => {
            push(&mut out, 0x21, 0x2f);
            push(&mut out, 0x3a, 0x40);
            push(&mut out, 0x5b, 0x60);
            push(&mut out, 0x7b, 0x7e);
        }
        "space" => {
            push(&mut out, 0x09, 0x0d);
            push(&mut out, 0x20, 0x20);
        }
        "upper" => push(&mut out, b'A', b'Z'),
        "xdigit" => {
            push(&mut out, b'0', b'9');
            push(&mut out, b'A', b'F');
            push(&mut out, b'a', b'f');
        }
        _ => return None,
    }
    Some(out)
}

/// The full byte set, for -c: every value from 0 to 255.
pub fn all_bytes() -> Vec<u8> {
    (0..=255u8).collect()
}

/// Translate one byte according to the two arrays.
pub fn translate(byte: u8, set1: &[u8], set2: &[u8]) -> Option<u8> {
    let index = set1.iter().position(|value| *value == byte)?;
    set2.get(index).copied()
}

/// Whether a byte is in a set.
pub fn contains(set: &[u8], byte: u8) -> bool {
    set.contains(&byte)
}

/// What the operands have to look like for the options in force.
///
/// * translating needs two STRINGs,
/// * -d alone takes one, and refuses a second,
/// * -s alone takes one,
/// * -d with -s takes two.
pub fn check_operands(strings: &[String], options: Options) -> Result<(String, Option<String>), OperandProblem> {
    let first = strings.first().cloned();
    let second = strings.get(1).cloned();
    if strings.is_empty() {
        return Err(OperandProblem::Missing);
    }
    if let Some(extra) = strings.get(2) {
        return Err(OperandProblem::Extra(extra.clone()));
    }
    match (options.delete, options.squeeze) {
        (true, false) => {
            if second.is_some() {
                return Err(OperandProblem::ExtraWhileDeleting(second.unwrap()));
            }
        }
        (true, true) => {
            // The manual's -d and -s together translate as well as squeeze, so
            // both strings are needed.
            if second.is_none() {
                return Err(OperandProblem::MissingAfterDeletingAndSqueezing(first.unwrap()));
            }
        }
        (false, true) => {}
        (false, false) => {
            if second.is_none() {
                return Err(OperandProblem::MissingAfterTranslating(first.unwrap()));
            }
        }
    }
    Ok((first.unwrap(), second))
}

/// Build the two arrays from the operands.
///
/// ARRAY2 is extended to the length of ARRAY1 by repeating its last character,
/// and any excess of ARRAY2 is ignored, as the manual describes.
pub fn arrays(text1: &str, text2: Option<&str>, options: Options) -> Result<(Vec<u8>, Vec<u8>), Problem> {
    let set1 = expand(text1, false)?;
    let mut set1 = if options.complement {
        all_bytes()
            .into_iter()
            .filter(|byte| !set1.contains(byte))
            .collect::<Vec<u8>>()
    } else {
        set1
    };

    // Deleting without squeezing never translates, so ARRAY2 is empty.
    if options.delete && !options.squeeze {
        return Ok((set1, Vec::new()));
    }

    let text2 = match text2 {
        Some(text2) => text2,
        // A squeeze on its own has nothing to translate.
        None => return Ok((set1, Vec::new())),
    };
    let mut set2 = expand(text2, true)?;

    if options.truncate && set2.len() < set1.len() {
        // -t truncates ARRAY1 to the length of ARRAY2.
        set1.truncate(set2.len());
        return Ok((set1, set2));
    }
    // ARRAY2 is extended by repeating its last character.
    if set2.len() < set1.len() {
        // An empty ARRAY2 cannot be padded, and check_operands has already
        // refused the cases where that could happen.
        let last = *set2.last().ok_or_else(|| Problem::BadClass(text2.to_string()))?;
        while set2.len() < set1.len() {
            set2.push(last);
        }
    }
    set2.truncate(set1.len().max(set2.len().min(set1.len())));
    Ok((set1, set2))
}

/// GNU diagnostics. The quotes coreutils puts around an operand are the
/// typographic ones, U+2018 and U+2019.
pub const LEFT_QUOTE: &str = "\u{2018}";
pub const RIGHT_QUOTE: &str = "\u{2019}";

pub fn quoted(text: &str) -> String {
    format!("{}{}{}", LEFT_QUOTE, text, RIGHT_QUOTE)
}

pub fn missing_operand_message() -> String {
    "tr: missing operand".to_string()
}

pub fn missing_after_message(operand: &str) -> String {
    format!("tr: missing operand after {}", quoted(operand))
}

pub fn extra_operand_message(operand: &str) -> String {
    format!("tr: extra operand {}", quoted(operand))
}

pub fn extra_while_deleting_message(operand: &str) -> String {
    format!("tr: extra operand {}", quoted(operand))
}

/// The second line of the -d with two STRINGs diagnostic.
pub fn only_one_string_message() -> String {
    "Only one string may be given when deleting without squeezing repeats.".to_string()
}

/// The second line of the -d -s with one STRING diagnostic.
pub fn two_strings_message() -> String {
    "Two strings must be given when both deleting and squeezing repeats.".to_string()
}

/// The second line of the one STRING while translating diagnostic.
pub fn two_strings_translating_message() -> String {
    "Two strings must be given when translating.".to_string()
}

pub fn bad_class_message(name: &str) -> String {
    format!("tr: invalid character class {}", quoted(name))
}

pub fn bad_range_message(text: &str) -> String {
    format!(
        "tr: range-endpoints of '{}' are in reverse collating sequence order",
        text
    )
}

pub fn unrecognized_option_message(option: &str) -> String {
    format!("tr: unrecognized option '{}'", option)
}

pub fn invalid_option_message(letter: char) -> String {
    format!("tr: invalid option -- '{}'", letter)
}

pub fn try_help_message() -> String {
    "Try 'tr --help' for more information.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_characters_pass_through() {
        assert_eq!(expand("abc", false).unwrap(), b"abc".to_vec());
    }

    #[test]
    fn octal_escapes_take_one_to_three_digits() {
        assert_eq!(expand("\\7", false).unwrap(), vec![7]);
        assert_eq!(expand("\\77", false).unwrap(), vec![0o77]);
        assert_eq!(expand("\\101", false).unwrap(), vec![0o101]);
        // A fourth digit is a separate character.
        assert_eq!(expand("\\1011", false).unwrap(), vec![0o101, b'1']);
    }

    #[test]
    fn the_named_escapes_are_the_usual_ones() {
        assert_eq!(expand("\\a", false).unwrap(), vec![0x07]);
        assert_eq!(expand("\\b", false).unwrap(), vec![0x08]);
        assert_eq!(expand("\\f", false).unwrap(), vec![0x0c]);
        assert_eq!(expand("\\n", false).unwrap(), vec![b'\n']);
        assert_eq!(expand("\\r", false).unwrap(), vec![b'\r']);
        assert_eq!(expand("\\t", false).unwrap(), vec![b'\t']);
        assert_eq!(expand("\\v", false).unwrap(), vec![0x0b]);
        assert_eq!(expand("\\\\", false).unwrap(), vec![b'\\']);
    }

    #[test]
    fn ranges_expand_in_ascending_order() {
        assert_eq!(expand("a-c", false).unwrap(), b"abc".to_vec());
        assert_eq!(expand("0-3x", false).unwrap(), b"0123x".to_vec());
        assert_eq!(expand("x-z", false).unwrap(), b"xyz".to_vec());
    }

    #[test]
    fn a_reversed_range_is_refused() {
        assert!(expand("c-a", false).is_err());
    }

    #[test]
    fn classes_expand() {
        assert_eq!(expand("[:digit:]", false).unwrap(), b"0123456789".to_vec());
        assert_eq!(
            expand("[:lower:]", false).unwrap(),
            (b'a'..=b'z').collect::<Vec<u8>>()
        );
        assert_eq!(
            expand("[:upper:]", false).unwrap(),
            (b'A'..=b'Z').collect::<Vec<u8>>()
        );
        let punct = expand("[:punct:]", false).unwrap();
        assert!(punct.contains(&b'!'));
        assert!(punct.contains(&b'~'));
        assert!(!punct.contains(&b'a'));
    }

    #[test]
    fn an_unknown_class_is_refused() {
        assert!(expand("[:nope:]", false).is_err());
    }

    #[test]
    fn repeats_are_only_for_the_second_array() {
        assert_eq!(expand("[a*]", true).unwrap().len(), 1);
        assert_eq!(expand("[a*3]", true).unwrap(), b"aaa".to_vec());
        assert_eq!(expand("[a3]", true).unwrap(), b"aaa".to_vec());
        assert!(expand("[a*3]", false).is_err());
    }

    #[test]
    fn equivalence_classes_give_the_character_itself() {
        assert_eq!(expand("[=a=]", false).unwrap(), b"a".to_vec());
    }

    #[test]
    fn mixed_escapes_ranges_and_classes() {
        assert_eq!(
            expand("\\n[:digit:]a-c", false).unwrap(),
            b"\n0123456789abc".to_vec()
        );
    }

    #[test]
    fn translating_maps_position_for_position() {
        let (set1, set2) = arrays("abc", Some("xyz"), Options::default()).unwrap();
        assert_eq!(translate(b'a', &set1, &set2), Some(b'x'));
        assert_eq!(translate(b'd', &set1, &set2), None);
    }

    #[test]
    fn the_second_array_is_padded_with_its_last_character() {
        let (set1, set2) = arrays("abcd", Some("xy"), Options::default()).unwrap();
        assert_eq!(set2, b"xyyy".to_vec());
        assert_eq!(translate(b'c', &set1, &set2), Some(b'y'));
    }

    #[test]
    fn excess_of_the_second_array_is_ignored() {
        let (set1, set2) = arrays("ab", Some("xyz"), Options::default()).unwrap();
        assert_eq!(set1, b"ab".to_vec());
        assert_eq!(set2, b"xy".to_vec());
    }

    #[test]
    fn truncate_set1_cuts_to_the_second_array() {
        let options = Options {
            truncate: true,
            ..Options::default()
        };
        let (set1, set2) = arrays("abcdef", Some("xy"), options).unwrap();
        assert_eq!(set1, b"ab".to_vec());
        assert_eq!(set2, b"xy".to_vec());
    }

    #[test]
    fn complement_takes_everything_else() {
        let options = Options {
            complement: true,
            ..Options::default()
        };
        let (set1, _) = arrays("abc", None, options).unwrap();
        assert_eq!(set1.len(), 253);
        assert!(!contains(&set1, b'a'));
        assert!(contains(&set1, b'z'));
    }

    #[test]
    fn delete_wants_no_second_array() {
        let options = Options {
            delete: true,
            ..Options::default()
        };
        let (set1, set2) = arrays("abc", None, options).unwrap();
        assert_eq!(set1, b"abc".to_vec());
        assert!(set2.is_empty());
    }

    #[test]
    fn squeeze_alone_needs_no_second_array() {
        let options = Options {
            squeeze: true,
            ..Options::default()
        };
        let (set1, set2) = arrays("abc", None, options).unwrap();
        assert_eq!(set1, b"abc".to_vec());
        assert!(set2.is_empty());
    }

    #[test]
    fn lower_and_upper_convert_case() {
        let (set1, set2) = arrays("[:lower:]", Some("[:upper:]"), Options::default()).unwrap();
        assert_eq!(translate(b'a', &set1, &set2), Some(b'A'));
        assert_eq!(translate(b'z', &set1, &set2), Some(b'Z'));
    }

    #[test]
    fn messages_match_coreutils() {
        assert_eq!(missing_operand_message(), "tr: missing operand");
        assert_eq!(
            extra_operand_message("c"),
            "tr: extra operand \u{2018}c\u{2019}"
        );
        assert_eq!(
            bad_class_message("nope"),
            "tr: invalid character class \u{2018}nope\u{2019}"
        );
        assert_eq!(
            bad_range_message("c-a"),
            "tr: range-endpoints of 'c-a' are in reverse collating sequence order"
        );
        assert_eq!(
            invalid_option_message('Z'),
            "tr: invalid option -- 'Z'"
        );
    }

    #[test]
    fn one_string_cannot_translate() {
        let strings = vec!["a".to_string()];
        assert_eq!(
            check_operands(&strings, Options::default()).unwrap_err(),
            OperandProblem::MissingAfterTranslating("a".into())
        );
        let strings = vec!["a".to_string(), "b".to_string()];
        assert_eq!(
            check_operands(&strings, Options::default()).unwrap(),
            ("a".to_string(), Some("b".to_string()))
        );
    }

    #[test]
    fn deleting_alone_takes_one_string() {
        let options = Options {
            delete: true,
            ..Options::default()
        };
        let strings = vec!["a".to_string()];
        assert!(check_operands(&strings, options).is_ok());
        let strings = vec!["a".to_string(), "b".to_string()];
        assert_eq!(
            check_operands(&strings, options).unwrap_err(),
            OperandProblem::ExtraWhileDeleting("b".into())
        );
    }

    #[test]
    fn deleting_and_squeezing_takes_two() {
        let options = Options {
            delete: true,
            squeeze: true,
            ..Options::default()
        };
        let strings = vec!["a".to_string()];
        assert_eq!(
            check_operands(&strings, options).unwrap_err(),
            OperandProblem::MissingAfterDeletingAndSqueezing("a".into())
        );
        let strings = vec!["a".to_string(), "b".to_string()];
        assert!(check_operands(&strings, options).is_ok());
    }

    #[test]
    fn squeezing_alone_takes_one_string() {
        let options = Options {
            squeeze: true,
            ..Options::default()
        };
        let strings = vec!["a".to_string()];
        assert!(check_operands(&strings, options).is_ok());
    }

    #[test]
    fn a_third_operand_is_extra() {
        let strings = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(
            check_operands(&strings, Options::default()).unwrap_err(),
            OperandProblem::Extra("c".into())
        );
    }

    #[test]
    fn no_operand_at_all_is_refused() {
        assert_eq!(
            check_operands(&[], Options::default()).unwrap_err(),
            OperandProblem::Missing
        );
    }

    #[test]
    fn an_unknown_escape_keeps_both_characters() {
        assert_eq!(expand("\\q", false).unwrap(), b"\\q".to_vec());
        assert_eq!(expand("\\q", true).unwrap(), b"\\q".to_vec());
    }
}