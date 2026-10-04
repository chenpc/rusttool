//! Selection logic of `tail(1)`.

/// What to count: lines (with an optional count) or bytes. `tail` keeps its
/// own copy so the two crates stay independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Count {
    Lines(u64),
    Bytes(u64),
}

/// The last `count` worth of `input`.
pub fn take(input: &[u8], count: Count) -> Vec<u8> {
    match count {
        Count::Bytes(n) => {
            let n = (n as usize).min(input.len());
            input[input.len() - n..].to_vec()
        }
        Count::Lines(n) => {
            // Count lines the way wc does: newlines, plus a trailing partial
            // line. Then skip everything before the last n.
            let newlines = input.iter().filter(|&&b| b == b'\n').count() as u64;
            let total = newlines + u64::from(!input.is_empty() && !input.ends_with(b"\n"));
            let skip = total.saturating_sub(n) as usize;
            let mut index = 0usize;
            let mut skipped = 0usize;
            while skipped < skip && index < input.len() {
                if input[index] == b'\n' {
                    skipped += 1;
                }
                index += 1;
            }
            input[index..].to_vec()
        }
    }
}

/// Parse `-n -3` / `-n +3` style arguments: a leading `-` counts from the end.
pub fn parse_lines(text: &str) -> Option<Count> {
    match text.strip_prefix('+') {
        Some(rest) => rest.parse::<u64>().ok().map(|n| Count::Lines(n)),
        None => {
            let digits = text.strip_prefix('-').unwrap_or(text);
            digits.parse::<u64>().ok().map(Count::Lines)
        }
    }
}

/// The name GNU puts in the "==> file <==" banner: the last path component,
/// with any trailing slashes dropped.
pub fn banner_name(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return "/";
    }
    trimmed.rsplit('/').next().unwrap_or(trimmed)
}

/// GNU prints "==> file <==" before each file's output unless -q.
pub fn header(path: &str) -> String {
    format!("==> {} <==\n", banner_name(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_lines() {
        let input = b"a\nb\nc\nd\n";
        assert_eq!(take(input, Count::Lines(2)), b"c\nd\n");
    }

    #[test]
    fn more_lines_than_input_is_everything() {
        let input = b"a\nb\n";
        assert_eq!(take(input, Count::Lines(9)), input);
    }

    #[test]
    fn unterminated_last_line_counts() {
        assert_eq!(take(b"a\nb\nc", Count::Lines(2)), b"b\nc");
    }

    #[test]
    fn bytes_mode_takes_the_end() {
        assert_eq!(take(b"abcdef", Count::Bytes(2)), b"ef");
    }

    #[test]
    fn parses_signed_line_counts() {
        assert_eq!(parse_lines("-2"), Some(Count::Lines(2)));
        assert_eq!(parse_lines("2"), Some(Count::Lines(2)));
        assert_eq!(parse_lines("+2"), Some(Count::Lines(2)));
    }
}
