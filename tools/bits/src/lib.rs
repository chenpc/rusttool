//! Pure, I/O-free core of the `bits(1)` clone (util-linux `text-utils/bits.c`).
//!
//! Upstream semantics mirrored here:
//!
//! * operands are bit groups, each optionally prefixed with `&`, `^`, `~`
//!   or `|`; groups starting with `,` or `0x` are hex masks, everything else
//!   is a comma-separated list of bit IDs / `lo-hi` ranges,
//! * groups combine with OR by default (`&` = AND, `^` = XOR, `~` clears the
//!   group's bits from the result),
//! * `--width` caps the mask (default 8192, max 128Ki); mask bits beyond it
//!   are truncated, list IDs beyond it are ignored unless `--fail-width`,
//! * output modes: `--mask` (default, `0x...`), `--grouped-mask` (32-bit
//!   comma groups), `--binary` (`0b...` with `_` per nibble), `--expand`
//!   (flat list), `--list` (ranges like `9-11`); an empty set prints
//!   `0x0`/`0`/`0b0` for the mask modes and nothing at all for list modes.
//!
//! Deviation: this is a self-contained reimplementation (no libcpuset); hex
//! mask groups are concatenated big-endian exactly like `cpumask_parse` for
//! the documented forms, and list parsing accepts `N` / `N-M` decimal items.

/// The util-linux release whose observable behaviour this clone tracks.
pub const UTIL_LINUX_VERSION: &str = "2.42";

/// `bits -h` output, shaped like util-linux' usage blocks.
pub const HELP: &str = concat!(
    "\nUsage:\n",
    " bits [options] [<mask_or_list>...]\n",
    "\n",
    "Convert bit masks from/to various formats.\n",
    "\n",
    "Arguments:\n",
    " <mask_or_list>      bits specified as a hex mask (e.g. 0xeec2)\n",
    "                       or as a comma-separated list of bit IDs\n",
    "\n",
    " If not specified, arguments will be read from stdin.\n",
    "\n",
    "Options:\n",
    " -w, --width <num>    maximum width of bit masks (default 8192)\n",
    " -f, --fail-width     fail if bit list contains values wider than width\n",
    " -m, --mask           display bits as a hex mask value (default)\n",
    " -g, --grouped-mask   display bits as a hex mask value in 32bit\n",
    "                       comma separated groups\n",
    " -b, --binary         display bits as a binary mask value\n",
    " -e, --expand         display bits as an expanded list of bit IDs\n",
    " -l, --list           display bits as a compressed list of bit IDs\n",
    " -h, --help           display this help and exit\n",
    " -V, --version        output version information and exit\n",
);

/// The exact line `bits -V` prints.
pub fn version_line() -> String {
    format!("bits from util-linux {}\n", UTIL_LINUX_VERSION)
}

/// Maximum mask width accepted by `-w` (mirrors upstream's 128k cap).
pub const MAX_WIDTH: usize = 128 * 1024;

/// Default mask width.
pub const DEFAULT_WIDTH: usize = 8192;

/// Output conversion mode; the last mode flag on the command line wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Mask,
    GroupedMask,
    Binary,
    Expand,
    List,
}

/// A fixed-width bit set backed by `u64` words, bit N = word N/64, bit N%64.
#[derive(Debug, Clone)]
pub struct BitSet {
    width: usize,
    words: Vec<u64>,
}

impl BitSet {
    pub fn new(width: usize) -> Self {
        let words = vec![0u64; width.div_ceil(64)];
        Self { width, words }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn set(&mut self, bit: usize) {
        if bit < self.width {
            self.words[bit / 64] |= 1u64 << (bit % 64);
        }
    }

    pub fn clear(&mut self, bit: usize) {
        if bit < self.width {
            self.words[bit / 64] &= !(1u64 << (bit % 64));
        }
    }

    pub fn is_set(&self, bit: usize) -> bool {
        bit < self.width && (self.words[bit / 64] >> (bit % 64)) & 1 == 1
    }

    pub fn count(&self) -> u32 {
        self.words.iter().map(|w| w.count_ones()).sum()
    }

    /// Highest set bit, or `None` when empty.
    pub fn highest(&self) -> Option<usize> {
        for (i, &w) in self.words.iter().enumerate().rev() {
            if w != 0 {
                return Some(i * 64 + 63 - w.leading_zeros() as usize);
            }
        }
        None
    }

    pub fn or_with(&mut self, other: &BitSet) {
        for (a, b) in self.words.iter_mut().zip(other.words.iter()) {
            *a |= *b;
        }
    }

    pub fn and_with(&mut self, other: &BitSet) {
        for (a, b) in self.words.iter_mut().zip(other.words.iter()) {
            *a &= *b;
        }
    }

    pub fn xor_with(&mut self, other: &BitSet) {
        for (a, b) in self.words.iter_mut().zip(other.words.iter()) {
            *a ^= *b;
        }
    }

    /// Clear every bit set in `other` (`~` operator).
    pub fn clear_with(&mut self, other: &BitSet) {
        for (a, b) in self.words.iter_mut().zip(other.words.iter()) {
            *a &= !*b;
        }
    }

    /// Sorted list of all set bits.
    pub fn set_bits(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.count() as usize);
        for (i, &w) in self.words.iter().enumerate() {
            let mut w = w;
            while w != 0 {
                let b = w.trailing_zeros() as usize;
                out.push(i * 64 + b);
                w &= w - 1;
            }
        }
        out
    }
}

fn parse_decimal(s: &str) -> Option<usize> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // Reject leading `+`/`-` etc.; digits only. Leading zeros are fine.
    s.parse::<usize>().ok()
}

/// Parse a comma-separated ID list (`4,5-8,30`) into `set`.
///
/// An empty string contributes no bits (matches an empty stdin line).
/// With `fail_width`, any ID `>= width` is an error; otherwise such IDs are
/// silently ignored (upstream truncates them).
pub fn parse_list(s: &str, set: &mut BitSet, fail_width: bool) -> Result<(), String> {
    let width = set.width();
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    for token in trimmed.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if let Some((lo_s, hi_s)) = token.split_once('-') {
            let lo = parse_decimal(lo_s.trim())
                .ok_or_else(|| format!("invalid bit list: {}", s))?;
            let hi = parse_decimal(hi_s.trim())
                .ok_or_else(|| format!("invalid bit list: {}", s))?;
            if lo > hi {
                return Err(format!("invalid bit list: {}", s));
            }
            if fail_width && hi >= width {
                return Err(format!("bit list wider than cpuset size: {}", s));
            }
            for bit in lo..=hi {
                set.set(bit);
            }
        } else {
            let bit =
                parse_decimal(token).ok_or_else(|| format!("invalid bit list: {}", s))?;
            if fail_width && bit >= width {
                return Err(format!("bit list wider than cpuset size: {}", s));
            }
            set.set(bit);
        }
    }
    Ok(())
}

/// Parse a hex mask (`0xeec2` or `,00300000,03000000`) into `set`.
///
/// `raw` is the argument *after* stripping the `&`/`^`/`~`/`|` operator
/// prefix (but still carrying its `,` / `0x` marker). Commas are removed and
/// the remaining hex digits are interpreted big-endian, exactly like
/// `cpumask_parse` for the documented forms. Bits beyond `width` are dropped.
pub fn parse_mask(raw: &str, set: &mut BitSet) -> Result<(), String> {
    let fail = || format!("invalid bit mask: {}", raw);
    let hex: &str = if raw.starts_with(',') {
        &raw[1..]
    } else if let Some(rest) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        rest
    } else {
        return Err(fail());
    };
    let digits: String = hex.chars().filter(|&c| c != ',').collect();
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(fail());
    }
    let width = set.width();
    // Rightmost digit is the least significant nibble.
    for (i, ch) in digits.chars().rev().enumerate() {
        let v = ch.to_digit(16).ok_or_else(fail)? as usize;
        for b in 0..4 {
            if (v >> b) & 1 == 1 {
                let bit = i * 4 + b;
                if bit < width {
                    set.set(bit);
                }
            }
        }
    }
    Ok(())
}

/// Parse one command-line group (with optional `&`/`^`/`~`/`|` prefix) and
/// combine it into `all`, mirroring upstream `parse_mask_or_list`.
pub fn apply_group(arg: &str, all: &mut BitSet, fail_width: bool) -> Result<(), String> {
    let width = all.width();
    let (op, rest) = match arg.chars().next() {
        Some('&') => ('&', &arg[1..]),
        Some('^') => ('^', &arg[1..]),
        Some('~') => ('~', &arg[1..]),
        Some('|') => ('|', &arg[1..]),
        _ => ('|', arg),
    };
    let mut bits = BitSet::new(width);
    if rest.starts_with(',') || rest.starts_with("0x") || rest.starts_with("0X") {
        parse_mask(rest, &mut bits).map_err(|_| format!("invalid bit mask: {}", arg))?;
    } else {
        parse_list(rest, &mut bits, fail_width)
            .map_err(|e| {
                // Preserve upstream's two distinct messages.
                if e.starts_with("bit list wider") {
                    format!("bit list wider than cpuset size: {}", arg)
                } else {
                    format!("invalid bit list: {}", arg)
                }
            })?;
    }
    match op {
        '&' => all.and_with(&bits),
        '|' => all.or_with(&bits),
        '^' => all.xor_with(&bits),
        '~' => all.clear_with(&bits),
        _ => unreachable!(),
    }
    Ok(())
}

/// Full-width hex of `bits`, most significant digit first (no `0x` prefix).
fn full_hex(bits: &BitSet) -> String {
    let nibbles = bits.width().div_ceil(4);
    let mut out = String::with_capacity(nibbles);
    for i in (0..nibbles).rev() {
        let mut v = 0u32;
        for b in 0..4 {
            let bit = i * 4 + b;
            if bits.is_set(bit) {
                v |= 1 << b;
            }
        }
        out.push(char::from_digit(v, 16).unwrap());
    }
    out
}

/// Render `bits` in `mode`, including the trailing newline (list modes emit
/// nothing at all when empty, like upstream).
pub fn render(bits: &BitSet, mode: OutputMode) -> Vec<u8> {
    if bits.count() == 0 {
        return match mode {
            OutputMode::Mask => b"0x0\n".to_vec(),
            OutputMode::GroupedMask => b"0\n".to_vec(),
            OutputMode::Binary => b"0b0\n".to_vec(),
            OutputMode::Expand | OutputMode::List => Vec::new(),
        };
    }
    match mode {
        OutputMode::Mask => {
            let hex = full_hex(bits);
            let stripped = hex.trim_start_matches('0');
            format!("0x{}\n", stripped).into_bytes()
        }
        OutputMode::GroupedMask => {
            let hex = full_hex(bits);
            let stripped = hex.trim_start_matches('0');
            // Split the stripped hex from the right into 8-digit (32-bit) groups.
            let mut groups: Vec<&str> = Vec::new();
            let mut end = stripped.len();
            while end > 0 {
                let start = end.saturating_sub(8);
                groups.push(&stripped[start..end]);
                end = start;
            }
            groups.reverse();
            let mut out = groups.join(",");
            out.push('\n');
            out.into_bytes()
        }
        OutputMode::Binary => {
            let hi = bits.highest().unwrap();
            let mut out = String::from("0b");
            let mut started = false;
            for n in (0..=hi).rev() {
                if started && (n + 1) % 4 == 0 {
                    out.push('_');
                }
                if bits.is_set(n) {
                    started = true;
                    out.push('1');
                } else if started {
                    out.push('0');
                } else if n == hi {
                    // Highest bit is set by construction; keep the arm total.
                    started = true;
                    out.push('1');
                }
            }
            out.push('\n');
            out.into_bytes()
        }
        OutputMode::Expand => {
            let ids: Vec<String> = bits.set_bits().iter().map(|n| n.to_string()).collect();
            let mut out = ids.join(",");
            out.push('\n');
            out.into_bytes()
        }
        OutputMode::List => {
            let ids = bits.set_bits();
            let mut parts: Vec<String> = Vec::new();
            let mut i = 0;
            while i < ids.len() {
                let mut j = i;
                while j + 1 < ids.len() && ids[j + 1] == ids[j] + 1 {
                    j += 1;
                }
                let run = j - i + 1;
                if run >= 3 {
                    parts.push(format!("{}-{}", ids[i], ids[j]));
                } else {
                    for k in i..=j {
                        parts.push(ids[k].to_string());
                    }
                }
                i = j + 1;
            }
            let mut out = parts.join(",");
            out.push('\n');
            out.into_bytes()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn combined(args: &[&str], width: usize) -> BitSet {
        let mut all = BitSet::new(width);
        for a in args {
            apply_group(a, &mut all, false).unwrap();
        }
        all
    }

    fn s(bits: &BitSet, mode: OutputMode) -> String {
        String::from_utf8(render(bits, mode)).unwrap()
    }

    #[test]
    fn list_mask_example_from_man() {
        // bits --list 0xeec2 -> 1,6,7,9-11,13-15
        let all = combined(&["0xeec2"], DEFAULT_WIDTH);
        assert_eq!(s(&all, OutputMode::List), "1,6,7,9-11,13-15\n");
    }

    #[test]
    fn mask_example_from_man() {
        // bits --mask 4,5-8 16,30 -> 0x400101f0
        let all = combined(&["4,5-8", "16,30"], DEFAULT_WIDTH);
        assert_eq!(s(&all, OutputMode::Mask), "0x400101f0\n");
    }

    #[test]
    fn binary_example_from_man() {
        let all = combined(&["4,5-8", "16,30"], DEFAULT_WIDTH);
        assert_eq!(
            s(&all, OutputMode::Binary),
            "0b100_0000_0000_0001_0000_0001_1111_0000\n"
        );
    }

    #[test]
    fn grouped_mask_example_from_man() {
        let all = combined(&["2,22,74,79"], DEFAULT_WIDTH);
        assert_eq!(s(&all, OutputMode::GroupedMask), "8400,00000000,00400004\n");
    }

    #[test]
    fn comma_mask_form_matches_man() {
        // bits --list ,00300000,03000000,30000003 -> 0,1,28,29,56,57,84,85
        let all = combined(&[",00300000,03000000,30000003"], DEFAULT_WIDTH);
        assert_eq!(s(&all, OutputMode::List), "0,1,28,29,56,57,84,85\n");
    }

    #[test]
    fn bitwise_operators_combine() {
        // bits --list 1,2,3,4 ~3-10 -> 1,2
        let all = combined(&["1,2,3,4", "~3-10"], DEFAULT_WIDTH);
        assert_eq!(s(&all, OutputMode::List), "1,2\n");
        // bits --list 1,2,3,4 ^3-10 -> 1,2,5-10
        let all = combined(&["1,2,3,4", "^3-10"], DEFAULT_WIDTH);
        assert_eq!(s(&all, OutputMode::List), "1,2,5-10\n");
        // AND keeps the intersection.
        let all = combined(&["1,2,3,4", "&2,3,9"], DEFAULT_WIDTH);
        assert_eq!(s(&all, OutputMode::Expand), "2,3\n");
    }

    #[test]
    fn width_truncates_and_fail_width_errors() {
        // --width 64 drops bit 74/79 silently by default.
        let all = combined(&["2,22,74,79"], 64);
        assert_eq!(s(&all, OutputMode::List), "2,22\n");
        let mut narrow = BitSet::new(64);
        assert!(apply_group("2,22,74,79", &mut narrow, true).is_err());
        // Masks truncate without failing.
        let mut narrow = BitSet::new(64);
        assert!(apply_group("0xeec2", &mut narrow, true).is_ok());
    }

    #[test]
    fn empty_set_outputs() {
        let all = BitSet::new(DEFAULT_WIDTH);
        assert_eq!(s(&all, OutputMode::Mask), "0x0\n");
        assert_eq!(s(&all, OutputMode::GroupedMask), "0\n");
        assert_eq!(s(&all, OutputMode::Binary), "0b0\n");
        assert_eq!(render(&all, OutputMode::List), b"");
        assert_eq!(render(&all, OutputMode::Expand), b"");
    }

    #[test]
    fn list_runs_compress_only_length_three_plus() {
        let all = combined(&["1,3-5,7"], DEFAULT_WIDTH);
        assert_eq!(s(&all, OutputMode::Expand), "1,3,4,5,7\n");
        assert_eq!(s(&all, OutputMode::List), "1,3-5,7\n");
        let two = combined(&["6,7"], DEFAULT_WIDTH);
        assert_eq!(s(&two, OutputMode::List), "6,7\n");
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        let mut all = BitSet::new(DEFAULT_WIDTH);
        assert!(apply_group("abc", &mut all, false).is_err());
        assert!(apply_group("0xzz", &mut all, false).is_err());
        assert!(apply_group("5-2", &mut all, false).is_err());
    }

    #[test]
    fn version_line_matches_upstream_format() {
        assert_eq!(version_line(), "bits from util-linux 2.42\n");
    }

    #[test]
    fn help_mentions_every_option() {
        for needle in [
            "--width",
            "--fail-width",
            "--mask",
            "--grouped-mask",
            "--binary",
            "--expand",
            "--list",
        ] {
            assert!(HELP.contains(needle), "help is missing {:?}", needle);
        }
    }
}
