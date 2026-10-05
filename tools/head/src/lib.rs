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

/// Streaming form of `take`: feed chunks, get back the bytes that belong to the
/// first `count` worth of input.
///
/// `take` needs the whole input in memory, which is fine for a file but not for a
/// stream that never ends. `head` must stop as soon as it has its count, so that
/// `head -n 5 /dev/zero` prints five lines instead of allocating forever.
pub struct Taker {
    count: Count,
    used: u64,
}

impl Taker {
    pub fn new(count: Count) -> Self {
        Taker { count, used: 0 }
    }

    /// True once the count is satisfied and nothing more should be read.
    pub fn done(&self) -> bool {
        match self.count {
            Count::Bytes(n) => self.used >= n,
            Count::Lines(n) => self.used >= n,
        }
    }

    /// The part of `chunk` that still belongs to the selection. Bytes past the
    /// count are not returned, so the caller can write them straight out.
    pub fn push<'a>(&mut self, chunk: &'a [u8]) -> &'a [u8] {
        let mut end = 0usize;
        for (index, &byte) in chunk.iter().enumerate() {
            if self.done() {
                break;
            }
            end = index + 1;
            if matches!(self.count, Count::Lines(_)) && byte == b'\n' {
                self.used += 1;
            } else if matches!(self.count, Count::Bytes(_)) {
                self.used += 1;
            }
        }
        &chunk[..end]
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
    fn taker_stops_at_the_count_across_chunks() {
        let mut t = Taker::new(Count::Lines(2));
        assert_eq!(t.push(b"ab\ncd"), b"ab\ncd");
        assert_eq!(t.push(b"\nef\n"), b"\n");
        assert!(t.done());
        assert_eq!(t.push(b"gh\n"), b"");
    }

    #[test]
    fn taker_byte_mode_stops_mid_line() {
        let mut t = Taker::new(Count::Bytes(3));
        assert_eq!(t.push(b"abcdef"), b"abc");
        assert!(t.done());
    }

    #[test]
    fn taker_star_reads_forever() {
        let mut t = Taker::new(Count::Bytes(u64::MAX));
        assert_eq!(t.push(b"abc"), b"abc");
        assert!(!t.done());
    }

    #[test]
    fn header_shape() {
        assert_eq!(header("f"), "==> f <==\n");
        assert_eq!(header("/usr/bin/sort"), "==> sort <==\n");
        assert_eq!(header("dir/"), "==> dir <==\n");
        assert_eq!(header("/"), "==> / <==\n");
    }
}
