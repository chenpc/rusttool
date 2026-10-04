//! Comparison and key logic of `sort(1)`, following its manual page:
//!
//! ```text
//! sort [OPTION]... [FILE]...
//! ```
//!
//! The manual separates the ordering options (`-b -d -f -g -h -i -M -n -R -V`)
//! from everything else, and describes KEYDEF as
//! `F[.C][OPTS][,F[.C][OPTS]]` with both positions origin 1. Without -t or -b a
//! field starts at the preceding whitespace, which is the detail that decides
//! where a key begins.

/// One ordering option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ordering {
    /// -b/--ignore-leading-blanks
    IgnoreBlanks,
    /// -d/--dictionary-order
    Dictionary,
    /// -f/--ignore-case
    FoldCase,
    /// -g/--general-numeric-sort
    GeneralNumeric,
    /// -h/--human-numeric-sort
    HumanNumeric,
    /// -i/--ignore-nonprinting
    IgnoreNonprinting,
    /// -M/--month-sort
    Month,
    /// -n/--numeric-sort
    Numeric,
    /// -R/--random-sort
    Random,
    /// -V/--version-sort
    Version,
    /// No option: compare the bytes as they are.
    None,
}

impl Ordering {
    /// The name --sort=WORD uses, and the letters a key's OPTS may contain.
    pub fn parse(flag: char) -> Option<Ordering> {
        match flag {
            'b' => Some(Ordering::IgnoreBlanks),
            'd' => Some(Ordering::Dictionary),
            'f' => Some(Ordering::FoldCase),
            'g' => Some(Ordering::GeneralNumeric),
            'h' => Some(Ordering::HumanNumeric),
            'i' => Some(Ordering::IgnoreNonprinting),
            'M' => Some(Ordering::Month),
            'n' => Some(Ordering::Numeric),
            'R' => Some(Ordering::Random),
            'V' => Some(Ordering::Version),
            _ => None,
        }
    }

    /// The long names --sort=WORD accepts, in the order the manual lists them.
    pub fn from_word(word: &str) -> Option<Ordering> {
        match word {
            "b" => Some(Ordering::IgnoreBlanks),
            "d" => Some(Ordering::Dictionary),
            "f" => Some(Ordering::FoldCase),
            "g" => Some(Ordering::GeneralNumeric),
            "h" => Some(Ordering::HumanNumeric),
            "i" => Some(Ordering::IgnoreNonprinting),
            "m" | "M" | "month" => Some(Ordering::Month),
            "n" | "numeric" => Some(Ordering::Numeric),
            "R" | "r" | "random" => Some(Ordering::Random),
            "V" | "version" => Some(Ordering::Version),
            _ => None,
        }
    }

    /// The word --sort=WORD uses for this option.
    pub fn word(&self) -> &'static str {
        match self {
            Ordering::IgnoreBlanks => "b",
            Ordering::Dictionary => "d",
            Ordering::FoldCase => "f",
            Ordering::GeneralNumeric => "g",
            Ordering::HumanNumeric => "h",
            Ordering::IgnoreNonprinting => "i",
            Ordering::Month => "month",
            Ordering::Numeric => "numeric",
            Ordering::Random => "random",
            Ordering::Version => "version",
            Ordering::None => "none",
        }
    }
}

/// One key: a field number, a character position inside it, and the options
/// that override the global ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub field: usize,
    pub character: usize,
    /// The options this key sets, which override the global ordering.
    pub options: Vec<Ordering>,
    /// An explicit end field or character, when the KEYDEF gave one.
    pub end_field: Option<usize>,
    pub end_character: Option<usize>,
    /// -r inside the KEYDEF reverses this key alone.
    pub reverse: bool,
    /// -s inside the KEYDEF stabilises it.
    pub stable: bool,
}

impl Key {
    /// Parse one `F[.C][OPTS]` piece of a KEYDEF.
    pub fn parse(text: &str) -> Option<Key> {
        if text.is_empty() {
            return None;
        }
        // The letters come last, so the digits and dot come first.
        let split = text
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(text.len());
        let (positions, letters) = text.split_at(split);
        let mut pieces = positions.split('.');
        let field = pieces.next()?.parse::<usize>().ok()?;
        if field == 0 {
            return None;
        }
        let character = match pieces.next() {
            Some(text) => {
                let value = text.parse::<usize>().ok()?;
                if value == 0 {
                    return None;
                }
                value
            }
            None => 1,
        };
        let mut options = Vec::new();
        for letter in letters.chars() {
            // A letter that is not an ordering option is ignored, as the manual
            // allows for OPTS letters that do not apply.
            if let Some(option) = Ordering::parse(letter) {
                options.push(option);
            }
        }
        Some(Key {
            field,
            character,
            options,
            end_field: None,
            end_character: None,
            reverse: letters.contains('r'),
            stable: letters.contains('s'),
        })
    }
}

/// Parse a whole KEYDEF, which is a start and a stop position joined by a comma.
pub fn parse_keydef(text: &str) -> Option<Vec<Key>> {
    let (start, stop) = match text.split_once(',') {
        Some((start, stop)) => (start, Some(stop)),
        None => (text, None),
    };
    let mut key = Key::parse(start)?;
    if let Some(stop) = stop {
        // The stop position may carry its own options, which are dropped: they
        // would apply to a comparison sort uses no further.
        let stop_base = stop
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(stop.len());
        let stop_positions = &stop[..stop_base];
        let mut pieces = stop_positions.split('.');
        let end_field = pieces.next().and_then(|value| value.parse::<usize>().ok());
        let end_character = pieces.next().and_then(|value| value.parse::<usize>().ok());
        key.end_field = end_field;
        key.end_character = end_character;
        // The stop position may carry the same letters the start does, and they
        // apply to the key: "1,1r" reverses it.
        for letter in stop[stop_base..].chars() {
            match letter {
                'r' => key.reverse = true,
                's' => key.stable = true,
                _ => {
                    if let Some(option) = Ordering::parse(letter) {
                        key.options.retain(|existing| *existing != option);
                        key.options.push(option);
                    }
                }
            }
        }
    }
    Some(vec![key])
}

/// The options `sort` accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The global ordering option, if any.
    pub ordering: Ordering,
    pub reverse: bool,
    /// -s/--stable
    pub stable: bool,
    /// -u/--unique
    pub unique: bool,
    /// -c/--check, and -C for the quiet form.
    pub check: bool,
    pub check_quiet: bool,
    /// -m/--merge
    pub merge: bool,
    /// -z/--zero-terminated
    pub zero_terminated: bool,
    /// -t/--field-separator=SEP
    pub separator: Option<u8>,
    pub keys: Vec<Key>,
    pub files: Vec<String>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            ordering: Ordering::None,
            reverse: false,
            stable: false,
            unique: false,
            check: false,
            check_quiet: false,
            merge: false,
            zero_terminated: false,
            separator: None,
            keys: Vec::new(),
            files: Vec::new(),
        }
    }
}


/// The fields of a line, following the manual's rule: with -t the separator
/// splits the fields, without it a field runs from the preceding whitespace to
/// the next one.
pub fn fields(line: &[u8], separator: Option<u8>) -> Vec<&[u8]> {
    match separator {
        Some(separator) => line.split(|byte| *byte == separator).collect(),
        None => {
            let mut out = Vec::new();
            let mut index = 0usize;
            while index < line.len() {
                // A field starts at the first non-blank after a blank.
                while index < line.len() && is_blank(line[index]) {
                    index += 1;
                }
                let start = index;
                while index < line.len() && !is_blank(line[index]) {
                    index += 1;
                }
                if start == index {
                    // A run of blanks at the end still ends the last field.
                    break;
                }
                out.push(&line[start..index]);
            }
            out
        }
    }
}

/// Whether a byte is one of the blanks the manual means.
pub fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

/// The byte offset of a character position inside a field, counting characters
/// rather than bytes so that UTF-8 text still works.
pub fn character_offset(field: &[u8], character: usize) -> usize {
    if character <= 1 {
        return 0;
    }
    let mut count = 1usize;
    let mut index = 0usize;
    while index < field.len() && count < character {
        index += 1;
        // Advance over the continuation bytes of a multi-byte character.
        while index < field.len() && (field[index] & 0xc0) == 0x80 {
            index += 1;
        }
        count += 1;
    }
    index.min(field.len())
}

/// Where one position of a KEYDEF falls in a line: the field's character
/// position, as an absolute byte offset.
///
/// Without -t the manual counts characters from the beginning of the preceding
/// whitespace, so the key of a field that follows blanks starts at those blanks
/// rather than at the field itself.
fn position_offset(
    line: &[u8],
    field: usize,
    character: usize,
    separator: Option<u8>,
    from_whitespace: bool,
) -> Option<usize> {
    let fields = fields(line, separator);
    let slice = fields.get(field.checked_sub(1)?)?;
    let base = slice.as_ptr() as usize - line.as_ptr() as usize;
    if from_whitespace && separator.is_none() && character <= 1 && field == 1 {
        // The first field is a run of blanks and then non-blanks, so its key
        // starts at the beginning of the line, blanks included. Every later
        // field starts after the blanks that separate it.
        return Some(0);
    }
    Some(base + character_offset(slice, character))
}

/// Where one key starts in a line.
pub fn key_start(line: &[u8], key: &Key, separator: Option<u8>) -> Option<usize> {
    // With -b in effect the blanks in front of the field do not count, and
    // with -t the fields are separated exactly.
    let from_whitespace = !key.options.contains(&Ordering::IgnoreBlanks);
    position_offset(line, key.field, key.character, separator, from_whitespace)
}

/// Where one key ends. Without a stop position the manual says the key runs to
/// the end of the line.
pub fn key_end(line: &[u8], key: &Key, separator: Option<u8>) -> Option<usize> {
    match (key.end_field, key.end_character) {
        (None, _) => Some(line.len()),
        (Some(field), None) => {
            let fields = fields(line, separator);
            let slice = fields.get(field.checked_sub(1)?)?;
            let base = slice.as_ptr() as usize - line.as_ptr() as usize;
            Some(base + slice.len())
        }
        (Some(field), Some(character)) => {
            let from_whitespace = !key.options.contains(&Ordering::IgnoreBlanks);
            position_offset(line, field, character, separator, from_whitespace)
        }
    }
}

/// The bytes one key selects, which is what the ordering options are applied to.
pub fn key_bytes<'a>(line: &'a [u8], key: &Key, options: &Options) -> &'a [u8] {
    let start = key_start(line, key, options.separator);
    let end = key_end(line, key, options.separator);
    match (start, end) {
        (Some(start), Some(end)) if start <= end && end <= line.len() => &line[start..end],
        _ => line,
    }
}

/// The ordering options that apply to a key: the key's own letters first, then
/// the global ones, with -b always applying because it says where a field starts.
pub fn effective_options(key: &Key, options: &Options) -> Vec<Ordering> {
    let mut out: Vec<Ordering> = Vec::new();
    if options.ordering != Ordering::None {
        out.push(options.ordering);
    }
    for option in &key.options {
        out.retain(|existing| *existing != *option);
        out.push(*option);
    }
    out
}

/// Apply the options that change what is compared.
pub fn prepare(bytes: &[u8], applied: &[Ordering]) -> Vec<u8> {
    let mut out: Vec<u8> = bytes.to_vec();
    if applied.contains(&Ordering::IgnoreNonprinting) {
        out.retain(|byte| byte.is_ascii_graphic() || *byte == b' ');
    }
    if applied.contains(&Ordering::Dictionary) {
        out.retain(|byte| byte.is_ascii_alphanumeric() || *byte == b' ');
    }
    if applied.contains(&Ordering::FoldCase) {
        out.make_ascii_uppercase();
    }
    if applied.contains(&Ordering::IgnoreBlanks) {
        while out.first().is_some_and(|byte| is_blank(*byte)) {
            out.remove(0);
        }
    }
    out
}

/// The month names -M understands, in the order it ranks them.
pub fn month_number(bytes: &[u8]) -> Option<u32> {
    const MONTHS: [&[u8]; 12] = [
        b"JAN", b"FEB", b"MAR", b"APR", b"MAY", b"JUN", b"JUL", b"AUG", b"SEP", b"OCT", b"NOV", b"DEC",
    ];
    let upper: Vec<u8> = bytes
        .iter()
        .take(3)
        .map(|byte| byte.to_ascii_uppercase())
        .collect();
    MONTHS
        .iter()
        .position(|month| *month == upper.as_slice())
        .map(|index| index as u32 + 1)
}

/// Compare two keys under the ordering options, in the C locale. The options
/// that change what is compared are applied here, so a caller cannot forget.
pub fn compare(left: &[u8], right: &[u8], applied: &[Ordering]) -> std::cmp::Ordering {
    let left = prepare(left, applied);
    let right = prepare(right, applied);
    compare_prepared(&left, &right, applied)
}

/// Compare two already-prepared keys.
fn compare_prepared(left: &[u8], right: &[u8], applied: &[Ordering]) -> std::cmp::Ordering {
    use std::cmp::Ordering as Cmp;
    if applied.contains(&Ordering::Numeric) {
        return compare_numeric(left, right);
    }
    if applied.contains(&Ordering::GeneralNumeric) {
        return compare_general_numeric(left, right);
    }
    if applied.contains(&Ordering::HumanNumeric) {
        return compare_human(left, right);
    }
    if applied.contains(&Ordering::Month) {
        let left_month = month_number(left);
        let right_month = month_number(right);
        return match (left_month, right_month) {
            (Some(a), Some(b)) => a.cmp(&b),
            // The manual puts an unknown month before every known one.
            (Some(_), None) => Cmp::Greater,
            (None, Some(_)) => Cmp::Less,
            (None, None) => left.cmp(right),
        };
    }
    if applied.contains(&Ordering::Version) {
        return compare_version(left, right);
    }
    left.cmp(right)
}

/// The leading number of a string, for -n: anything that is not a digit, a sign
/// or a decimal point ends it, and what follows compares as text.
pub fn leading_number(bytes: &[u8]) -> (Option<f64>, &[u8]) {
    let mut index = 0usize;
    let mut seen_digit = false;
    while index < bytes.len() && matches!(bytes[index], b' ' | b'\t') {
        index += 1;
    }
    let start = index;
    if index < bytes.len() && matches!(bytes[index], b'-' | b'+') {
        index += 1;
    }
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
        seen_digit = true;
    }
    if index < bytes.len() && bytes[index] == b'.' {
        let mut lookahead = index + 1;
        while lookahead < bytes.len() && bytes[lookahead].is_ascii_digit() {
            lookahead += 1;
            seen_digit = true;
        }
        if seen_digit {
            index = lookahead;
        }
    }
    if !seen_digit {
        return (None, bytes);
    }
    let text = std::str::from_utf8(&bytes[start..index]).unwrap_or("0");
    let value = text.parse::<f64>().unwrap_or(0.0);
    (Some(value), &bytes[index..])
}

/// The numeric value and the trailing text for -g, where the text never
/// compares.
pub fn leading_general(bytes: &[u8]) -> Option<f64> {
    let (value, _) = leading_number(bytes);
    value
}

/// Compare by string numerical value, as -n does.
pub fn compare_numeric(left: &[u8], right: &[u8]) -> std::cmp::Ordering {
    let (left_value, left_rest) = leading_number(left);
    let (right_value, right_rest) = leading_number(right);
    let first = match (left_value, right_value) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
    };
    // Ties fall back to a byte comparison, which is what makes -n stable in the
    // way the manual describes.
    if first != std::cmp::Ordering::Equal {
        return first;
    }
    left_rest.cmp(right_rest).then_with(|| left.cmp(right))
}

/// Compare two keys by their numbers alone, which is what -u needs: with -n,
/// "1" and "01" are the same number, so only the first of them is kept.
pub fn compare_keys_only(left: &[u8], right: &[u8], applied: &[Ordering]) -> std::cmp::Ordering {
    use std::cmp::Ordering as Cmp;
    let left = prepare(left, applied);
    let right = prepare(right, applied);
    if applied.contains(&Ordering::Numeric) {
        let (a, _) = leading_number(&left);
        let (b, _) = leading_number(&right);
        return match (a, b) {
            (None, None) => Cmp::Equal,
            (None, Some(_)) => Cmp::Less,
            (Some(_), None) => Cmp::Greater,
            (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(Cmp::Equal),
        };
    }
    if applied.contains(&Ordering::GeneralNumeric) {
        return match (leading_general(&left), leading_general(&right)) {
            (None, None) => Cmp::Equal,
            (None, Some(_)) => Cmp::Less,
            (Some(_), None) => Cmp::Greater,
            (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(Cmp::Equal),
        };
    }
    if applied.contains(&Ordering::Month) {
        return match (month_number(&left), month_number(&right)) {
            (Some(a), Some(b)) => a.cmp(&b),
            (Some(_), None) => Cmp::Greater,
            (None, Some(_)) => Cmp::Less,
            (None, None) => left.cmp(&right),
        };
    }
    if applied.contains(&Ordering::Version) {
        return compare_version(&left, &right);
    }
    left.cmp(&right)
}

/// Compare by general numerical value, as -g does: the trailing text is ignored.
pub fn compare_general_numeric(left: &[u8], right: &[u8]) -> std::cmp::Ordering {
    match (leading_general(left), leading_general(right)) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
    }
}

/// Compare human readable numbers, as -h does: 2K is 2000 and 1G is 1e9.
pub fn compare_human(left: &[u8], right: &[u8]) -> std::cmp::Ordering {
    fn parse(bytes: &[u8]) -> Option<f64> {
        let (value, rest) = leading_number(bytes);
        let mut text: &[u8] = rest;
        while text.first().is_some_and(|byte| is_blank(*byte)) {
            text = &text[1..];
        }
        let multiplier = match text.first() {
            None => 1.0,
            Some(byte) => match byte.to_ascii_lowercase() {
                b'k' => 1e3,
                b'm' => 1e6,
                b'g' => 1e9,
                b't' => 1e12,
                b'p' => 1e15,
                b'e' => 1e18,
                _ => return None,
            },
        };
        value.map(|value| value * multiplier)
    }
    match (parse(left), parse(right)) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
    }
}

/// The byte order -V compares with: a blank comes after every other character,
/// which is what coreutils' version order does.
fn version_byte(byte: u8) -> u8 {
    match byte {
        b' ' | b'\t' => 0xff,
        other => other,
    }
}

/// Compare version strings, as -V does: the digit runs compare as numbers.
pub fn compare_version(left: &[u8], right: &[u8]) -> std::cmp::Ordering {
    let left: Vec<u8> = left.iter().map(|byte| version_byte(*byte)).collect();
    let right: Vec<u8> = right.iter().map(|byte| version_byte(*byte)).collect();
    let (left, right) = (left.as_slice(), right.as_slice());
    let mut a = 0usize;
    let mut b = 0usize;
    loop {
        // Skip the parts that are not digits in both, then compare a number.
        while a < left.len() && b < right.len() {
            let left_digit = left[a].is_ascii_digit();
            let right_digit = right[b].is_ascii_digit();
            if left_digit && right_digit {
                break;
            }
            if left_digit != right_digit {
                // A digit sorts before a letter, as GNU sort does.
                return if left_digit {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                };
            }
            match left[a].cmp(&right[b]) {
                std::cmp::Ordering::Equal => {
                    a += 1;
                    b += 1;
                }
                other => return other,
            }
        }
        if a >= left.len() || b >= right.len() {
            break;
        }
        let left_start = a;
        while a < left.len() && left[a].is_ascii_digit() {
            a += 1;
        }
        let left_number: u128 = std::str::from_utf8(&left[left_start..a])
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);
        let right_start = b;
        while b < right.len() && right[b].is_ascii_digit() {
            b += 1;
        }
        let right_number: u128 = std::str::from_utf8(&right[right_start..b])
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);
        match left_number.cmp(&right_number) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
    }
    match left.len().cmp(&right.len()) {
        std::cmp::Ordering::Equal => left.cmp(right),
        other => other,
    }
}

/// The full ordering of two lines, keys first and the last-resort comparison
/// after, which -s removes.
pub fn compare_lines(
    left: &[u8],
    right: &[u8],
    options: &Options,
) -> std::cmp::Ordering {
    let mut result = std::cmp::Ordering::Equal;
    for key in &options.keys {
        let left_key = key_bytes(left, key, options);
        let right_key = key_bytes(right, key, options);
        let applied = effective_options(key, options);
        result = compare(left_key, right_key, &applied);
        if result != std::cmp::Ordering::Equal {
            if options.reverse || key.reverse {
                return result.reverse();
            }
            return result;
        }
    }
    if options.keys.is_empty() || !options.stable {
        // With no key the whole line is the key; with keys, equal keys fall
        // back to the whole line unless -s was given. Either way the fallback
        // uses the global ordering options first and the raw bytes when that
        // still ties, which is what decides between lines the ordering cannot
        // separate.
        let applied = match options.ordering {
            Ordering::None => Vec::new(),
            ordering => vec![ordering],
        };
        let ordered = compare(left, right, &applied);
        result = if ordered == std::cmp::Ordering::Equal {
            left.cmp(right)
        } else {
            ordered
        };
    }
    if options.reverse {
        result.reverse()
    } else {
        result
    }
}

/// What a check reports about the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    Sorted,
    /// The first line that is out of order, with its number and its text.
    OutOfOrder {
        line: usize,
        text: Vec<u8>,
    },
}

/// Check whether the lines are sorted, as -c does.
///
/// With -u the order has to be strict, so a line equal to the one before it is
/// the disorder coreutils reports.
pub fn check_sorted(lines: &[Vec<u8>], options: &Options) -> Check {
    for index in 1..lines.len() {
        let order = compare_lines(&lines[index - 1], &lines[index], options);
        let out_of_order = order == std::cmp::Ordering::Greater
            || (options.unique && order == std::cmp::Ordering::Equal);
        if out_of_order {
            return Check::OutOfOrder {
                line: index + 1,
                text: lines[index].clone(),
            };
        }
    }
    Check::Sorted
}

/// Reduce a sorted run to the lines -u keeps: one per set of equal keys, and
/// the one that came first in the input.
///
/// `sorted` is the input with each line's original position, so "first" means
/// first in the input, which is what coreutils keeps.
pub fn unique_in_input_order(
    sorted: &mut [(Vec<u8>, usize)],
    options: &Options,
) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::with_capacity(sorted.len());
    let mut index = 0usize;
    while index < sorted.len() {
        let start = index;
        while index + 1 < sorted.len()
            && is_duplicate(&sorted[index + 1].0, &sorted[index].0, options)
        {
            index += 1;
        }
        // Among the equal lines, the one with the smallest position came first.
        let first = (start..=index).min_by_key(|slot| sorted[*slot].1).unwrap();
        out.push(sorted[first].0.clone());
        index += 1;
    }
    out
}

/// Whether -u should drop a line, given that `previous` is the line before it.
///
/// The keys decide, and nothing else: with -n "1" and "01" are the same number,
/// so only the first of them is kept.
pub fn is_duplicate(previous: &[u8], line: &[u8], options: &Options) -> bool {
    if options.keys.is_empty() {
        let applied = match options.ordering {
            Ordering::None => Vec::new(),
            ordering => vec![ordering],
        };
        return compare_keys_only(previous, line, &applied) == std::cmp::Ordering::Equal;
    }
    for key in &options.keys {
        let applied = effective_options(key, options);
        if compare_keys_only(
            key_bytes(previous, key, options),
            key_bytes(line, key, options),
            &applied,
        ) != std::cmp::Ordering::Equal
        {
            return false;
        }
    }
    true
}

/// GNU diagnostics.
/// The -c diagnostic names the file, the line number and the line itself, as
/// coreutils does.
pub fn disorder_message(file: &str, line: usize, text: &[u8]) -> String {
    format!(
        "sort: {}:{}: disorder: {}",
        file,
        line,
        String::from_utf8_lossy(text)
    )
}

/// A key field of zero, which coreutils words as an invalid specification.
pub fn zero_field_message(field: &str) -> String {
    format!(
        "sort: field number is zero: invalid field specification \u{2018}{}\u{2019}",
        field
    )
}

/// A -t value that is not a single character.
pub fn multi_character_separator_message(value: &str) -> String {
    format!("sort: multi-character tab \u{2018}{}\u{2019}", value)
}

/// The invalid --sort word, with the list of what would have been accepted.
pub fn bad_sort_word_lines(word: &str) -> Vec<String> {
    let quoted = |text: &str| format!("\u{2018}{}\u{2019}", text);
    let mut out = vec![
        format!(
            "sort: invalid argument {} for {}",
            quoted(word),
            quoted("--sort")
        ),
        "Valid arguments are:".to_string(),
    ];
    for name in [
        "general-numeric",
        "human-numeric",
        "month",
        "numeric",
        "random",
        "version",
    ] {
        out.push(format!("  - {}", quoted(name)));
    }
    out.push(try_help_message());
    out
}

pub fn unrecognized_option_message(option: &str) -> String {
    format!("sort: unrecognized option '{}'", option)
}

pub fn invalid_option_message(letter: char) -> String {
    format!("sort: invalid option -- '{}'", letter)
}

pub fn requires_argument_message(name: &str) -> String {
    format!("sort: option '--{}' requires an argument", name)
}

pub fn try_help_message() -> String {
    "Try 'sort --help' for more information.".to_string()
}

/// A file that cannot be read, worded the way coreutils words it.
pub fn cannot_read_message(file: &str, reason: &str) -> String {
    format!("sort: cannot read: {}: {}", file, reason)
}

pub fn cannot_write_message(reason: &str) -> String {
    format!("sort: cannot write standard output: {}", reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering as Cmp;

    fn defaults() -> Options {
        Options::default()
    }

    #[test]
    fn a_key_may_carry_the_r_and_s_letters() {
        let key = Key::parse("1r").unwrap();
        assert!(key.reverse);
        assert!(!key.stable);
        let key = Key::parse("1,1s").unwrap();
        assert!(key.stable);
    }

    #[test]
    fn a_key_is_a_field_and_a_character() {
        let key = Key::parse("2").unwrap();
        assert_eq!(key.field, 2);
        assert_eq!(key.character, 1);
        assert!(key.options.is_empty());

        let key = Key::parse("2.3").unwrap();
        assert_eq!(key.field, 2);
        assert_eq!(key.character, 3);

        let key = Key::parse("2.3n").unwrap();
        assert_eq!(key.options, vec![Ordering::Numeric]);
    }

    #[test]
    fn a_keydef_may_carry_a_stop_position() {
        let keys = parse_keydef("2,3").unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].field, 2);
        assert_eq!(keys[0].end_field, Some(3));

        let keys = parse_keydef("2.2,4.1").unwrap();
        assert_eq!(keys[0].character, 2);
        assert_eq!(keys[0].end_field, Some(4));
        assert_eq!(keys[0].end_character, Some(1));
    }

    #[test]
    fn nonsense_keys_are_refused() {
        assert!(Key::parse("").is_none());
        assert!(Key::parse("0").is_none());
        assert!(Key::parse("2.0").is_none());
        assert!(Key::parse("x").is_none());
    }

    #[test]
    fn fields_split_on_whitespace_by_default() {
        let line = b"  one   two three ";
        let fields = fields(line, None);
        assert_eq!(fields, vec![&b"one"[..], &b"two"[..], &b"three"[..]]);
    }

    #[test]
    fn fields_split_on_the_given_separator() {
        let line = b"one:two:three";
        let split = fields(line, Some(b':'));
        assert_eq!(split, vec![&b"one"[..], &b"two"[..], &b"three"[..]]);
        // An empty field is still a field.
        let with_empty = fields(b"a::b", Some(b':'));
        assert_eq!(with_empty.len(), 3);
    }

    #[test]
    fn the_first_fields_key_starts_at_the_line() {
        // A field is a run of blanks and then non-blanks, so the first field's
        // key includes the blanks the line starts with.
        let options = Options {
            keys: parse_keydef("1,1").unwrap(),
            ..defaults()
        };
        assert_eq!(key_bytes(b"  b 2", &options.keys[0], &options), b"  b");
        assert_eq!(key_bytes(b"a 3", &options.keys[0], &options), b"a");
        // -b takes them out of it again.
        let options = Options {
            keys: parse_keydef("1b,1b").unwrap(),
            ..defaults()
        };
        assert_eq!(key_bytes(b"  b 2", &options.keys[0], &options), b"b");
    }

    #[test]
    fn a_later_fields_key_starts_at_the_field() {
        let options = Options {
            keys: parse_keydef("2").unwrap(),
            ..defaults()
        };
        // The blanks that separate the fields are not part of the key.
        assert_eq!(key_bytes(b"one two three", &options.keys[0], &options), b"two three");
        assert_eq!(
            key_bytes(b"  one   two three", &options.keys[0], &options),
            b"two three"
        );
    }

    #[test]
    fn a_key_selects_the_field_it_names() {
        // Without a stop position the key runs to the end of the line.
        let options = Options {
            keys: parse_keydef("2").unwrap(),
            ..defaults()
        };
        assert_eq!(key_bytes(b"one two three", &options.keys[0], &options), b"two three");

        // A stop position limits it to that field.
        let options = Options {
            keys: parse_keydef("2,2").unwrap(),
            ..defaults()
        };
        assert_eq!(key_bytes(b"one two three", &options.keys[0], &options), b"two");
    }

    #[test]
    fn a_key_character_position_offsets_into_the_field() {
        let options = Options {
            keys: parse_keydef("1.2").unwrap(),
            ..defaults()
        };
        assert_eq!(key_bytes(b"abc", &options.keys[0], &options), b"bc");
    }

    #[test]
    fn a_stop_position_trims_the_key() {
        let options = Options {
            keys: parse_keydef("1.2,1.3").unwrap(),
            ..defaults()
        };
        assert_eq!(key_bytes(b"abcd", &options.keys[0], &options), b"b");
    }

    #[test]
    fn without_a_stop_the_key_runs_to_the_end_of_the_line() {
        let options = Options {
            keys: parse_keydef("1.2").unwrap(),
            ..defaults()
        };
        assert_eq!(key_bytes(b"abcd", &options.keys[0], &options), b"bcd");
    }

    #[test]
    fn the_last_resort_comparison_sorts_equal_keys() {
        // The key is the first character, so the lines tie on the key and the
        // last-resort comparison decides.
        let options = Options {
            keys: parse_keydef("1.1,1.1").unwrap(),
            ..defaults()
        };
        let mut lines = vec![b"b x".to_vec(), b"a z".to_vec(), b"a y".to_vec()];
        lines.sort_by(|left, right| compare_lines(left, right, &options));
        assert_eq!(lines[0], b"a y".to_vec());
        assert_eq!(lines[1], b"a z".to_vec());
        assert_eq!(lines[2], b"b x".to_vec());
    }

    #[test]
    fn stable_removes_the_last_resort_comparison() {
        // The key is the first character, so both lines have the same key.
        let options = Options {
            keys: parse_keydef("1.1,1.1").unwrap(),
            stable: true,
            ..defaults()
        };
        let mut lines = vec![b"a z".to_vec(), b"a y".to_vec()];
        lines.sort_by(|left, right| compare_lines(left, right, &options));
        // Equal keys keep their input order.
        assert_eq!(lines[0], b"a z".to_vec());
        assert_eq!(lines[1], b"a y".to_vec());
    }

    #[test]
    fn reverse_flips_the_result() {
        let options = Options {
            reverse: true,
            ..defaults()
        };
        assert_eq!(compare_lines(b"a", b"b", &options), Cmp::Greater);
    }

    #[test]
    fn numeric_ordering_compares_the_leading_number() {
        let options = Options {
            ordering: Ordering::Numeric,
            ..defaults()
        };
        let mut lines = vec![b"10".to_vec(), b"9".to_vec(), b"2".to_vec()];
        lines.sort_by(|left, right| compare_lines(left, right, &options));
        assert_eq!(lines[0], b"2".to_vec());
        assert_eq!(lines[2], b"10".to_vec());
    }

    #[test]
    fn numeric_ordering_falls_back_to_the_rest() {
        assert_eq!(leading_number(b"  12abc"), (Some(12.0), &b"abc"[..]));
        assert_eq!(leading_number(b"-3.5x"), (Some(-3.5), &b"x"[..]));
        assert_eq!(leading_number(b"abc"), (None, &b"abc"[..]));
    }

    #[test]
    fn general_numeric_ignores_the_trailing_text() {
        let applied = vec![Ordering::GeneralNumeric];
        assert_eq!(compare(b"2abc", b"10", &applied), Cmp::Less);
        assert_eq!(compare(b"abc", b"1", &applied), Cmp::Less);
    }

    #[test]
    fn human_numeric_knows_the_suffixes() {
        let applied = vec![Ordering::HumanNumeric];
        assert_eq!(compare(b"2K", b"1G", &applied), Cmp::Less);
        assert_eq!(compare(b"1M", b"999", &applied), Cmp::Greater);
        assert_eq!(compare(b"1k", b"1000", &applied), Cmp::Equal);
    }

    #[test]
    fn month_ordering_ranks_the_names() {
        let applied = vec![Ordering::Month];
        assert_eq!(compare(b"JAN", b"DEC", &applied), Cmp::Less);
        assert_eq!(compare(b"dec", b"JAN", &applied), Cmp::Greater);
        // An unknown month comes first.
        assert_eq!(compare(b"zzz", b"JAN", &applied), Cmp::Less);
        assert_eq!(compare(b"JAN", b"zzz", &applied), Cmp::Greater);
    }

    #[test]
    fn dictionary_order_ignores_punctuation() {
        let applied = vec![Ordering::Dictionary];
        assert_eq!(compare(b"a-b", b"ab", &applied), Cmp::Equal);
    }

    #[test]
    fn fold_case_makes_case_irrelevant() {
        let applied = vec![Ordering::FoldCase];
        assert_eq!(compare(b"ABC", b"abc", &applied), Cmp::Equal);
    }

    #[test]
    fn ignore_nonprinting_drops_control_bytes() {
        let applied = vec![Ordering::IgnoreNonprinting];
        assert_eq!(compare(b"a\x01b", b"ab", &applied), Cmp::Equal);
    }

    #[test]
    fn version_ordering_compares_numbers_as_numbers() {
        let applied = vec![Ordering::Version];
        assert_eq!(compare(b"a-2", b"a-10", &applied), Cmp::Less);
        assert_eq!(compare(b"1.0", b"1.0", &applied), Cmp::Equal);
        assert_eq!(compare(b"1.10", b"1.9", &applied), Cmp::Greater);
    }

    #[test]
    fn a_key_option_overrides_the_global_one() {
        let options = Options {
            ordering: Ordering::Numeric,
            keys: parse_keydef("1f").unwrap(),
            ..defaults()
        };
        let key = &options.keys[0];
        let applied = effective_options(key, &options);
        assert!(applied.contains(&Ordering::Numeric));
        assert!(applied.contains(&Ordering::FoldCase));
    }

    #[test]
    fn ignore_blanks_drops_leading_whitespace() {
        let applied = vec![Ordering::IgnoreBlanks];
        assert_eq!(compare(b"  a", b"a", &applied), Cmp::Equal);
    }

    #[test]
    fn checking_finds_the_first_disorder() {
        let options = defaults();
        let lines = vec![b"a".to_vec(), b"b".to_vec(), b"a".to_vec()];
        assert_eq!(
            check_sorted(&lines, &options),
            Check::OutOfOrder {
                line: 3,
                text: b"a".to_vec()
            }
        );
        let lines = vec![b"a".to_vec(), b"b".to_vec()];
        assert_eq!(check_sorted(&lines, &options), Check::Sorted);
    }

    #[test]
    fn check_with_unique_wants_a_strict_order() {
        let options = Options {
            unique: true,
            ..defaults()
        };
        let lines = vec![b"a".to_vec(), b"a".to_vec(), b"b".to_vec()];
        assert_eq!(
            check_sorted(&lines, &options),
            Check::OutOfOrder {
                line: 2,
                text: b"a".to_vec()
            }
        );
    }

    #[test]
    fn uniqueness_uses_the_keys_alone() {
        // With -n "1" and "01" are the same number, so -u keeps one of them.
        let options = Options {
            ordering: Ordering::Numeric,
            unique: true,
            ..defaults()
        };
        assert!(is_duplicate(b"1", b"01", &options));
        assert!(!is_duplicate(b"1", b"2", &options));
    }

    #[test]
    fn uniqueness_is_decided_by_the_comparison() {
        let options = defaults();
        assert!(is_duplicate(b"a", b"a", &options));
        assert!(!is_duplicate(b"a", b"b", &options));
    }

    #[test]
    fn messages_match_coreutils() {
        assert_eq!(disorder_message("f", 3, b"a"), "sort: f:3: disorder: a");
        assert_eq!(
            zero_field_message("0"),
            "sort: field number is zero: invalid field specification \u{2018}0\u{2019}"
        );
        assert_eq!(
            multi_character_separator_message("ab"),
            "sort: multi-character tab \u{2018}ab\u{2019}"
        );
        assert_eq!(
            cannot_read_message("nope", "No such file or directory"),
            "sort: cannot read: nope: No such file or directory"
        );
        assert_eq!(invalid_option_message('Z'), "sort: invalid option -- 'Z'");
        assert_eq!(bad_sort_word_lines("nope").len(), 9);
    }
}