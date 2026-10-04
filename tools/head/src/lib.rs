//! Selection logic of `head(1)`, independent of where the bytes come from.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Count {
    Lines(u64),
    Bytes(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub count: Count,
    pub quiet: bool,
    pub verbose: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { count: Count::Lines(10), quiet: false, verbose: false }
    }
}

/// The first `options.count` worth of `input`, and how many bytes were used.
/// `*` in a numeric argument means "everything" (GNU convention).
pub fn take(input: &[u8], count: Count) -> (Vec<u8>, usize) {
    match count {
        Count::Bytes(n) => {
            let n = (n as usize).min(input.len());
            (input[..n].to_vec(), n)
        }
        Count::Lines(n) => {
            let mut out = Vec::new();
            let mut lines = 0u64;
            let mut index = 0usize;
            while index < input.len() && lines < n {
                let byte = input[index];
                out.push(byte);
                index += 1;
                if byte == b'\n' {
                    lines += 1;
                }
            }
            (out, index)
        }
    }
}

/// Parse a count argument: a plain number, `+N`, or `*`/start `-` (all).
pub fn parse_count(text: &str) -> Option<Count> {
    let trimmed = text.trim_start_matches('+');
    if trimmed == "*" || trimmed == "-" {
        return Some(Count::Bytes(u64::MAX));
    }
    trimmed.parse::<u64>().ok().map(Count::Lines)
}

/// The name GNU puts in the "==> file <==" banner: the last path component,
/// with any trailing slashes dropped, so `head /usr/bin/sort` says `sort`.
pub fn banner_name(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        // "/" and "//" keep their single slash rather than becoming empty.
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
    fn default_is_ten_lines() {
        let input = b"a\nb\nc\n";
        let (out, _) = take(input, Count::Lines(10));
        assert_eq!(out, input);
    }

    #[test]
    fn counts_lines_not_bytes() {
        let (out, _) = take(b"a\nb\nc\n", Count::Lines(2));
        assert_eq!(out, b"a\nb\n");
    }

    #[test]
    fn unterminated_last_line_still_counts() {
        let (out, _) = take(b"a\nb", Count::Lines(2));
        assert_eq!(out, b"a\nb");
    }

    #[test]
    fn byte_mode_can_split_a_line() {
        let (out, _) = take(b"abcdef", Count::Bytes(3));
        assert_eq!(out, b"abc");
    }

    #[test]
    fn star_means_everything() {
        assert_eq!(parse_count("*"), Some(Count::Bytes(u64::MAX)));
        assert_eq!(parse_count("+3"), Some(Count::Lines(3)));
        assert_eq!(parse_count("7"), Some(Count::Lines(7)));
        assert_eq!(parse_count("x"), None);
    }

    #[test]
    fn header_shape() {
        assert_eq!(header("f"), "==> f <==\n");
        assert_eq!(header("/usr/bin/sort"), "==> sort <==\n");
        assert_eq!(header("dir/"), "==> dir <==\n");
        assert_eq!(header("/"), "==> / <==\n");
    }
}
