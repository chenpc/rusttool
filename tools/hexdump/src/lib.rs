//! Pure, I/O-free core of the `hexdump(1)` clone.
//!
//! A faithful port of util-linux `text-utils/hexdump-parse.c` and
//! `hexdump-display.c` (BSD heritage; source-read, not guessed). Key
//! upstream semantics reproduced:
//!
//! * Unit syntax (add_fmt): `[reps][ws|/][/[size][ws]]"fmt"` — a repetition
//!   count must be followed by whitespace or `/`; an explicit byte count
//!   must be followed by whitespace; `8/2"x"` is a *bad format*.
//! * The conversion spec after `%` skips chars in `.#-+ 0123456789`, then
//!   takes an optional `.prec`, then the conversion letter. Flags/width are
//!   handed to a printf-style renderer (`%03o` zero-pads to 3, `%7x`
//!   space-pads to 7).
//! * `%_a`/`%_A` REQUIRE a base letter from `dox` (uppercase `X` rejected;
//!   a bare `%_a` is `bad conversion character %_a`).
//! * A unit with an explicit byte count may hold only ONE conversion
//!   (`byte count with multiple conversion characters`).
//! * Escapes (escape()): only `\a \b \f \n \r \t \v` are special; any other
//!   `\c` drops the backslash and copies `c` (`\0` is the digit `0`, not
//!   NUL).
//! * Auto-repeat (rewrite_rules): the LAST unit of a format, when it has no
//!   explicit repetition count and consumes data, is repeated to fill the
//!   block size (the max over all formats).
//! * nospace: a unit with `reps > 1` drops the last whitespace of its
//!   output on the final iteration.
//! * bpad: a conversion whose first byte lies at/past the end of data
//!   prints `width` spaces (missing *later* bytes read as zero).
//! * Units holding `%_A` render once at end of input (F_IGNORE), with the
//!   final absolute offset, only when at least one byte was consumed.
//! * NOT supported: float conversions (`%e %f %g` and friends) — they fail
//!   with `bad format {...}` like an unknown verb.

/// The util-linux release whose observable behaviour this clone tracks.
pub const UTIL_LINUX_VERSION: &str = "2.42";

/// `hexdump -h` output, shaped like the upstream `usage()` text.
pub const HELP: &str = concat!(
    "\nUsage:\n",
    " hexdump [options] <file>...\n",
    "\n",
    "Display file contents in hexadecimal, decimal, octal, or ascii.\n",
    "\n",
    "Options:\n",
    " -b, --one-byte-octal      one-byte octal display\n",
    " -X, --one-byte-hex        one-byte hexadecimal display\n",
    " -c, --one-byte-char       one-byte character display\n",
    " -C, --canonical           canonical hex+ASCII display\n",
    " -d, --two-bytes-decimal   two-byte decimal display\n",
    " -o, --two-bytes-octal     two-byte octal display\n",
    " -x, --two-bytes-hex       two-byte hexadecimal display\n",
    " -L, --color[=<mode>]      interpret color formatting specifiers\n",
    " -e, --format <format>     format string to be used for displaying data\n",
    " -f, --format-file <file>  file that contains format strings\n",
    " -n, --length <length>     interpret only length bytes of input\n",
    " -s, --skip <offset>       skip offset bytes from the beginning\n",
    " -v, --no-squeezing        output identical lines\n",
    " -h, --help                display this help\n",
    " -V, --version             display version\n",
    "\n",
    "See hexdump(1) for more details.\n",
);

/// The exact line `hexdump -V` prints.
pub fn version_line() -> String {
    format!("hexdump from util-linux {}\n", UTIL_LINUX_VERSION)
}

/// Numeric base of a conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    Dec,
    Oct,
    Hex,
}

impl Base {
    /// The base letters accepted for %_a/%_A: exactly `dox` (no `X`).
    fn from_base_letter(letter: u8) -> Option<Base> {
        match letter {
            b'd' => Some(Base::Dec),
            b'o' => Some(Base::Oct),
            b'x' => Some(Base::Hex),
            _ => None,
        }
    }

    fn digits(self, value: u64) -> String {
        match self {
            Base::Dec => format!("{}", value),
            Base::Oct => format!("{:o}", value),
            Base::Hex => format!("{:x}", value),
        }
    }
}

/// A data conversion (`%` verb that consumes input bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataConv {
    /// `%c`: the raw byte.
    Char,
    /// `%_c`: C escapes, graphic ASCII as-is, otherwise 3-digit octal.
    EscChar,
    /// `%_p`: graphic ASCII (and space) as-is, `.` otherwise.
    Printable,
    /// `%_u`: control names (nul/soh/...), octal otherwise.
    Unnamed,
    /// `%s`: up to `bcnt` bytes, stopping at NUL.
    Str,
    /// `%d`/`%i`: sign-extended value in decimal.
    Signed,
    /// `%e`/`%f`/`%g`: 8-byte little-endian double.
    Double,
    /// `%o`/`%u`/`%x`/`%X`: value in the given base.
    Unsigned(Base),
}

/// One item inside a quoted format part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    /// Literal bytes (already unescaped).
    Literal(Vec<u8>),
    /// `%_a` (current address) / `%_A` (end address) offset.
    Offset {
        end: bool,
        base: Base,
        width: Option<usize>,
        prec: Option<usize>,
        left: bool,
        zero: bool,
    },
    /// A conversion consuming `size` bytes per iteration.
    Data {
        conv: DataConv,
        width: Option<usize>,
        prec: Option<usize>,
        left: bool,
        zero: bool,
    },
}

/// One `[reps][/size]"..."` unit. `iterate` repetitions each consume
/// `size` bytes per data conversion inside `parts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub iterate: u64,
    pub size: usize,
    pub parts: Vec<Part>,
    /// The unit carried an explicit leading repetition count (F_SETREP).
    pub explicit_reps: bool,
    /// The unit's format ends in a blank, and `iterate > 1`, so the final
    /// iteration drops that blank (upstream `nospace`).
    ///
    /// Decided from the *format*, never from the rendered output: a data byte
    /// that happens to be a space must survive.
    pub nospace: bool,
}

impl Unit {
    /// Bytes consumed from the block per iteration (data parts only).
    fn bytes_per_iter(&self) -> usize {
        self.parts
            .iter()
            .filter(|p| matches!(p, Part::Data { .. }))
            .count()
            * self.size
    }

    /// Total bytes consumed per block.
    pub fn bytes_per_block(&self) -> usize {
        self.bytes_per_iter() * self.iterate as usize
    }

    /// Whether this unit renders at the end (holds an end-offset).
    pub fn is_end(&self) -> bool {
        self.parts
            .iter()
            .any(|p| matches!(p, Part::Offset { end: true, .. }))
    }
}

/// One `-e`/`-b`/... format string: an ordered list of units sharing one
/// cursor. Every unit starts where the previous one stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Format {
    pub units: Vec<Unit>,
}

impl Format {
    /// Bytes this format consumes per block (end units contribute 0).
    pub fn bytes_per_block(&self) -> usize {
        self.units
            .iter()
            .filter(|u| !u.is_end())
            .map(Unit::bytes_per_block)
            .sum()
    }
}

/// printf-ish integer rendering: `prec` is a minimum digit count
/// (zero-padded, after the sign), `width` pads with spaces -- or zeros when
/// `zero` holds and no precision was given -- and `left` pads on the right.
fn render_int(
    digits: &str,
    negative: bool,
    width: Option<usize>,
    prec: Option<usize>,
    left: bool,
    zero: bool,
) -> String {
    let mut body = String::new();
    if negative {
        body.push('-');
    }
    if let Some(prec) = prec {
        for _ in digits.len()..prec {
            body.push('0');
        }
    }
    body.push_str(digits);
    let width = width.unwrap_or(0);
    if body.len() >= width {
        return body;
    }
    let pad = width - body.len();
    if left {
        body.extend(std::iter::repeat(' ').take(pad));
        body
    } else if zero && prec.is_none() && !negative {
        format!("{}{}", "0".repeat(pad), body)
    } else if zero && prec.is_none() {
        // Zero padding goes after the sign, like printf.
        let mut out = String::with_capacity(width);
        out.push('-');
        out.extend(std::iter::repeat('0').take(pad));
        out.push_str(digits);
        out
    } else {
        format!("{}{}", " ".repeat(pad), body)
    }
}

/// escape(): upstream escape() handles only these; any other `\c` drops
/// the backslash and copies `c` literally.
fn unescape_quoted(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut chars = text.bytes();
    while let Some(b) = chars.next() {
        if b != b'\\' {
            out.push(b);
            continue;
        }
        match chars.next() {
            Some(b'a') => out.push(7),
            Some(b'b') => out.push(8),
            Some(b'f') => out.push(12),
            Some(b'n') => out.push(b'\n'),
            Some(b'r') => out.push(b'\r'),
            Some(b't') => out.push(b'\t'),
            Some(b'v') => out.push(11),
            Some(other) => out.push(other),
            None => break,
        }
    }
    out
}

/// `%_c` body: escapes, printable ASCII as-is, otherwise 3-digit octal.
///
/// Upstream tests `isprint()`, so a space prints as a space; only
/// non-printables become octal.
fn esc_char(byte: u8) -> String {
    match byte {
        0 => "\\0".to_string(),
        7 => "\\a".to_string(),
        8 => "\\b".to_string(),
        b'\t' => "\\t".to_string(),
        b'\n' => "\\n".to_string(),
        11 => "\\v".to_string(),
        12 => "\\f".to_string(),
        b'\r' => "\\r".to_string(),
        0x20..=0x7e => (byte as char).to_string(),
        _ => format!("{:03o}", byte),
    }
}

/// `%_u` body: control names like cat -v's neighbours; octal otherwise.
fn unnamed_char(byte: u8) -> String {
    const NAMES: [&str; 32] = [
        "nul", "soh", "stx", "etx", "eot", "enq", "ack", "bel", "bs", "ht", "nl", "vt", "ff",
        "cr", "so", "si", "dle", "dc1", "dc2", "dc3", "dc4", "nak", "syn", "etb", "can", "em",
        "sub", "esc", "fs", "gs", "rs", "us",
    ];
    match byte {
        0..=31 => NAMES[byte as usize].to_string(),
        0x7f => "del".to_string(),
        0x21..=0x7e => (byte as char).to_string(),
        _ => format!("{:03o}", byte),
    }
}

fn is_print(byte: u8) -> bool {
    (0x20..=0x7e).contains(&byte)
}

/// Parse failure carrying the exact upstream message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatError {
    pub message: String,
    pub generic: bool,
}

fn generic_err() -> FormatError {
    FormatError { message: String::new(), generic: true }
}

fn specific_err(message: impl Into<String>) -> FormatError {
    FormatError { message: message.into(), generic: false }
}

/// Parser for one `-e` format string (or one line of a `-f` file), ported
/// from add_fmt()/rewrite_rules().
struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Parser { bytes: text.as_bytes(), pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    /// strtol-style decimal number; leaves pos on the first non-digit.
    fn take_number(&mut self) -> Option<u64> {
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        if self.pos == start {
            return None;
        }
        std::str::from_utf8(&self.bytes[start..self.pos]).ok()?.parse().ok()
    }

    fn parse_format(&mut self) -> Result<Vec<Unit>, FormatError> {
        let mut units = Vec::new();
        loop {
            self.skip_space();
            if self.pos >= self.bytes.len() {
                break;
            }
            units.push(self.parse_unit()?);
        }
        if units.is_empty() {
            return Err(generic_err());
        }
        Ok(units)
    }

    /// add_fmt(): reps must be followed by space or `/`; size must be
    /// followed by space; then the quoted format.
    fn parse_unit(&mut self) -> Result<Unit, FormatError> {
        let mut explicit_reps = false;
        let mut iterate = 1u64;
        if matches!(self.peek(), Some(b'0'..=b'9')) {
            iterate = self.take_number().unwrap_or(1);
            explicit_reps = true;
            match self.peek() {
                Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'/') => {
                    self.pos += 1;
                    self.skip_space();
                }
                _ => return Err(generic_err()),
            }
        }
        if self.peek() == Some(b'/') {
            self.pos += 1;
            self.skip_space();
        }
        let mut explicit_size = false;
        let mut size = 0usize;
        if matches!(self.peek(), Some(b'0'..=b'9')) {
            size = self.take_number().unwrap_or(0) as usize;
            explicit_size = size > 0;
            match self.peek() {
                Some(b' ') | Some(b'\t') | Some(b'\n') => {
                    self.pos += 1;
                    self.skip_space();
                }
                _ => return Err(generic_err()),
            }
        }
        if self.peek() != Some(b'"') {
            return Err(generic_err());
        }
        self.pos += 1;
        let close = self
            .bytes[self.pos..]
            .iter()
            .position(|&b| b == b'"')
            .ok_or_else(generic_err)?;
        let quoted = std::str::from_utf8(&self.bytes[self.pos..self.pos + close])
            .map_err(|_| generic_err())?;
        self.pos += close + 1;

        let text = unescape_quoted(quoted);
        let parts = self.parse_quoted_parts(&text)?;

        // Derive the byte count when not given explicitly (block_size()).
        let mut derived = 0usize;
        for p in &parts {
            if let Part::Data { conv, prec, .. } = p {
                derived += match conv {
                    DataConv::Char | DataConv::EscChar | DataConv::Printable
                    | DataConv::Unnamed => 1,
                    DataConv::Str => prec.unwrap_or(0),
                    DataConv::Double => 8,
                    DataConv::Signed | DataConv::Unsigned(_) => 4,
                };
            }
        }
        let size = if explicit_size { size } else { derived };
        // An explicit byte count must be one the kernel conversions accept;
        // upstream reports the conversion character, not the count.
        if explicit_size && !matches!(size, 1 | 2 | 4 | 8) {
            // Name the conversion the way upstream does: the letter after the
            // flags (and the underscore for the %_x forms).
            let name = first_conversion_name(&quoted);
            return Err(specific_err(format!(
                "bad byte count for conversion character {}",
                name
            )));
        }
        // A %s without any byte count is rejected (badsfmt).
        if !explicit_size && size == 0 {
            for p in &parts {
                if matches!(p, Part::Data { conv: DataConv::Str, .. }) {
                    return Err(specific_err(
                        "%s requires a precision or a byte count",
                    ));
                }
            }
        }
        // An explicit byte count allows only one conversion.
        let data_count = parts
            .iter()
            .filter(|p| matches!(p, Part::Data { .. }))
            .count();
        if explicit_size && data_count > 1 {
            return Err(specific_err("byte count with multiple conversion characters"));
        }
        // nospace: upstream looks for the last whitespace inside the last
        // print record of a repeated unit, which is exactly a literal blank at
        // the end of this unit's format.
        let nospace = iterate > 1
            && matches!(
                parts.last(),
                Some(Part::Literal(bytes))
                    if matches!(bytes.last(), Some(b' ') | Some(b'\t'))
            );
        Ok(Unit { iterate, size, parts, explicit_reps, nospace })
    }

    /// Split the unescaped quoted text into literal/conversion parts.
    fn parse_quoted_parts(&mut self, text: &[u8]) -> Result<Vec<Part>, FormatError> {
        let mut parts = Vec::new();
        let mut literal: Vec<u8> = Vec::new();
        let mut idx = 0usize;
        while idx < text.len() {
            if text[idx] != b'%' {
                literal.push(text[idx]);
                idx += 1;
                continue;
            }
            if !literal.is_empty() {
                parts.push(Part::Literal(std::mem::take(&mut literal)));
            }
            let (part, next) = self.parse_conversion(&text[idx..])?;
            parts.push(part);
            idx += next;
        }
        if !literal.is_empty() {
            parts.push(Part::Literal(literal));
        }
        Ok(parts)
    }

    /// Parse one conversion from the start of `text` (which begins with
    /// `%`); returns the part and how many bytes were consumed.
    fn parse_conversion(&self, text: &[u8]) -> Result<(Part, usize), FormatError> {
        let conv_start = 0usize; // text[0] == b'%'
        let mut pos = 1usize;
        if text.get(pos) == Some(&b'%') {
            return Ok((Part::Literal(vec![b'%']), 2));
        }
        let mut left = false;
        let mut zero = false;
        // skip set: ".#-+ 0123456789" minus the leading '.', i.e. the
        // rewrite loop skips "#", "-", "+", " " and digits.
        while matches!(
            text.get(pos),
            Some(b'#') | Some(b'-') | Some(b'+') | Some(b' ') | Some(b'0'..=b'9')
        ) {
            match text[pos] {
                b'-' => left = true,
                b'0' => zero = true,
                _ => {}
            }
            pos += 1;
        }
        let mut width: Option<usize> = None;
        let mut prec: Option<usize> = None;
        {
            // The skip loop above consumed the field width digits too;
            // recover them by re-reading the numeric prefix.
            let start = 1usize;
            let mut p = start;
            while matches!(text.get(p), Some(b'0'..=b'9')) {
                p += 1;
            }
            if p > start {
                width = Some(
                    std::str::from_utf8(&text[start..p]).unwrap().parse::<usize>().unwrap_or(0),
                );
            }
            if text.get(p) == Some(&b'.') {
                let ds = p + 1;
                let mut d = ds;
                while matches!(text.get(d), Some(b'0'..=b'9')) {
                    d += 1;
                }
                prec = Some(
                    std::str::from_utf8(&text[ds..d]).unwrap().parse::<usize>().unwrap_or(0),
                );
                pos = d;
            }
        }
        let verb = match text.get(pos) {
            Some(&b) => b,
            None => return Err(generic_err()),
        };
        // badconv() names the conversion from here on (flags and width skipped).
        let verb_start = pos;
        pos += 1;

        if verb == b'_' {
            let sub = match text.get(pos) {
                Some(&b) => b,
                None => return Err(generic_err()),
            };
            pos += 1;
            // upstream badconv() names the conversion starting at the letter
            // after the flags/width, plus one more character when one follows.
            let name = |upto: usize| -> String {
                String::from_utf8_lossy(&text[verb_start..upto]).into_owned()
            };
            match sub {
                b'a' | b'A' => {
                    // A base letter from "dox" is REQUIRED.
                    match text.get(pos).copied().and_then(Base::from_base_letter) {
                        Some(base) => {
                            pos += 1;
                            Ok((
                                Part::Offset {
                                    end: sub == b'A',
                                    base,
                                    width,
                                    prec,
                                    left,
                                    zero,
                                },
                                pos,
                            ))
                        }
                        None => {
                            // badconv over "%_a"/"%_A" plus the offending
                            // letter when one follows.
                            let upto = if text.get(pos).is_some() { pos + 1 } else { pos };
                            Err(specific_err(format!(
                                "bad conversion character %{}",
                                name(upto)
                            )))
                        }
                    }
                }
                b'c' | b'p' | b'u' => {
                    let conv = match sub {
                        b'c' => DataConv::EscChar,
                        b'p' => DataConv::Printable,
                        _ => DataConv::Unnamed,
                    };
                    Ok((Part::Data { conv, width, prec, left, zero }, pos))
                }
                _ => {
                    let upto = if text.get(pos).is_some() { pos + 1 } else { pos };
                    Err(specific_err(format!(
                        "bad conversion character %{}",
                        name(upto)
                    )))
                }
            }
        } else {
            let conv = match verb {
                b'c' => DataConv::Char,
                b'd' | b'i' => DataConv::Signed,
                b'o' => DataConv::Unsigned(Base::Oct),
                b'u' => DataConv::Unsigned(Base::Dec),
                b'x' => DataConv::Unsigned(Base::Hex),
                b'X' => DataConv::Unsigned(Base::Hex),
                b's' => DataConv::Str,
                b'e' | b'f' | b'g' => DataConv::Double,
                b'E' | b'G' => {
                    // Upstream has no uppercase float verb; it names the
                    // character it did not recognise.
                    return Err(specific_err(format!(
                        "bad conversion character %{}",
                        String::from_utf8_lossy(&text[verb_start..pos])
                    )));
                }
                _ => {
                    return Err(specific_err(format!(
                        "bad conversion character %{}",
                        String::from_utf8_lossy(&text[verb_start..pos])
                    )));
                }
            };
            Ok((Part::Data { conv, width, prec, left, zero }, pos))
        }
    }
}

/// The name upstream uses when it rejects a byte count: the conversion letter
/// after any flags/width, with the underscore kept for the `%_x` forms.
fn first_conversion_name(quoted: &str) -> String {
    let bytes = quoted.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && bytes[i] != b'%' {
        i += 1;
    }
    if i >= bytes.len() {
        return "?".to_string();
    }
    i += 1; // the '%'
    while i < bytes.len() && matches!(bytes[i], b'#' | b'-' | b'+' | b' ' | b'0'..=b'9') {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    let mut name = String::new();
    if i < bytes.len() && bytes[i] == b'_' {
        name.push('_');
        i += 1;
    }
    match bytes.get(i) {
        Some(&b) => name.push(b as char),
        None => {}
    }
    name
}

/// Parse one `-e` format string into units. Generic errors come back
/// already wrapped as `bad format {<text>}`.
pub fn parse_format(text: &str) -> Result<Vec<Unit>, FormatError> {
    let mut p = Parser::new(text);
    p.parse_format().map_err(|e| {
        if e.generic {
            FormatError { message: format!("bad format {{{}}}", text), generic: false }
        } else {
            e
        }
    })
}

/// Render one offset conversion at `address`.
fn render_offset(part: &Part, address: u64) -> Vec<u8> {
    let Part::Offset { base, width, prec, left, zero, .. } = part else {
        unreachable!("render_offset on non-offset part")
    };
    render_int(&base.digits(address), false, *width, *prec, *left, *zero).into_bytes()
}

/// Render one data conversion over `window` (the unit's byte window inside
/// the block) where `valid` of those bytes actually exist in the block.
fn render_data(
    conv: DataConv,
    window: &[u8],
    valid: usize,
    width: Option<usize>,
    prec: Option<usize>,
    left: bool,
    zero: bool,
) -> Vec<u8> {
    let width_v = width.unwrap_or(0);
    if valid == 0 {
        // bpad: the conversion prints `width` spaces.
        return vec![b' '; width_v];
    }
    let body: String = match conv {
        DataConv::Char => {
            let ch = window[0] as char;
            if width_v <= 1 {
                ch.to_string()
            } else {
                let pad = " ".repeat(width_v - 1);
                if left {
                    format!("{}{}", ch, pad)
                } else {
                    format!("{}{}", pad, ch)
                }
            }
        }
        DataConv::EscChar => {
            let text = esc_char(window[0]);
            if text.len() >= width_v {
                text
            } else {
                let pad = " ".repeat(width_v - text.len());
                if left {
                    format!("{}{}", text, pad)
                } else {
                    format!("{}{}", pad, text)
                }
            }
        }
        DataConv::Printable => {
            let ch = if is_print(window[0]) { window[0] as char } else { '.' };
            ch.to_string()
        }
        DataConv::Unnamed => unnamed_char(window[0]),
        DataConv::Str => {
            let end = window.iter().position(|&b| b == 0).unwrap_or(valid);
            let mut text = String::from_utf8_lossy(&window[..end]).into_owned();
            if let Some(prec) = prec {
                text.truncate(prec);
            }
            if text.len() >= width_v {
                text
            } else {
                let pad = " ".repeat(width_v - text.len());
                if left {
                    format!("{}{}", text, pad)
                } else {
                    format!("{}{}", pad, text)
                }
            }
        }
        DataConv::Double => {
            let bits = window
                .iter()
                .take(valid)
                .fold(0u64, |acc, &b| (acc << 8) | b as u64);
            let value = f64::from_bits(bits);
            let mut text = format!("{}", value);
            if let Some(prec) = prec {
                text = format!("{:.*}", prec, value);
            }
            let mut padded = text;
            let width_v = width.unwrap_or(0);
            if padded.len() < width_v {
                let pad = " ".repeat(width_v - padded.len());
                padded = if left { format!("{}{}", padded, pad) } else { format!("{}{}", pad, padded) };
            }
            padded
        }
        DataConv::Signed | DataConv::Unsigned(_) => {
            // Little-endian; missing bytes read as zero.
            let mut value: u64 = 0;
            for (i, &b) in window.iter().take(valid).enumerate() {
                value |= (b as u64) << (8 * i);
            }
            match conv {
                DataConv::Signed => {
                    let bits = window.len() * 8;
                    let signed = (value as i64) << (64 - bits) >> (64 - bits);
                    let digits = format!("{}", signed.unsigned_abs());
                    render_int(&digits, signed < 0, width, prec, left, zero)
                }
                _ => {
                    let base = match conv {
                        DataConv::Unsigned(b) => b,
                        _ => Base::Dec,
                    };
                    render_int(&base.digits(value), false, width, prec, left, zero)
                }
            }
        }
    };
    body.into_bytes()
}

/// Render one unit iteration; returns the bytes emitted and the new cursor.
fn render_unit_iter(
    unit: &Unit,
    block: &[u8],
    cursor: usize,
    address: u64,
    final_iter: bool,
) -> (Vec<u8>, usize) {
    let mut out = Vec::new();
    let mut cur = cursor;
    for part in &unit.parts {
        match part {
            Part::Literal(bytes) => out.extend_from_slice(bytes),
            Part::Offset { .. } => out.extend(render_offset(part, address + cur as u64)),
            Part::Data { conv, width, prec, left, zero } => {
                let have = block.len().saturating_sub(cur);
                let window = &block[cur.min(block.len())..(cur + unit.size).min(block.len())];
                let valid = have.min(unit.size);
                out.extend(render_data(*conv, window, valid, *width, *prec, *left, *zero));
                cur += unit.size;
            }
        }
    }
    if final_iter && unit.nospace {
        // nospace: drop the format's trailing blank on the last iteration.
        if let Some(idx) = out.iter().rposition(|&b| b == b' ' || b == b'\t') {
            out.remove(idx);
        }
    }
    (out, cur)
}

/// Render one format over one block, appending to `out`.
fn render_format(format: &Format, block: &[u8], address: u64, out: &mut Vec<u8>) {
    let mut cursor = 0usize;
    for unit in &format.units {
        if unit.is_end() {
            continue; // F_IGNORE: rendered once at end of input
        }
        for it in 0..unit.iterate {
            let (bytes, next) =
                render_unit_iter(unit, block, cursor, address, it + 1 == unit.iterate);
            out.extend(bytes);
            cursor = next;
        }
    }
}

/// Render the end-of-input line(s).
///
/// Upstream keeps a single `endfu` pointer, so when several formats carry a
/// `%_A` unit only the LAST one prints its end line.
fn render_end_lines(formats: &[Format], address: u64, out: &mut Vec<u8>) {
    let last = formats
        .iter()
        .rposition(|f| f.units.iter().any(Unit::is_end));
    let Some(index) = last else { return };
    for unit in &formats[index].units {
        if !unit.is_end() {
            continue;
        }
        for _ in 0..unit.iterate {
            let (bytes, _) = render_unit_iter(unit, &[], 0, address, false);
            out.extend(bytes);
        }
    }
}

/// Expand the last unit of each format to fill `blocksize`
/// (rewrite_rules auto-repeat), returning the adjusted formats.
fn auto_repeat(mut formats: Vec<Format>, blocksize: usize) -> Vec<Format> {
    for format in &mut formats {
        let consumed = format.bytes_per_block();
        if consumed >= blocksize || format.units.is_empty() {
            continue;
        }
        let last = format.units.last_mut().unwrap();
        if last.is_end() || last.explicit_reps || last.size == 0 || last.bytes_per_iter() == 0 {
            continue;
        }
        let per = last.bytes_per_iter();
        let extra = (blocksize - consumed) / per;
        last.iterate += extra as u64;
    }
    formats
}

/// Full display pass over `data`: blocks, squeezing (`*`), end line.
///
/// `base` is the absolute offset of `data[0]`, i.e. the bytes skipped with
/// `-s`: upstream keeps counting addresses across the skipped region, so a
/// dump with `-s 8` starts at `00000008`. Returns everything `hexdump` would
/// write to stdout.
pub fn display(data: &[u8], formats: &[Format], squeeze: bool, base: u64) -> Vec<u8> {
    let mut out = Vec::new();
    let end_address = base + data.len() as u64;
    // The end line appears once any input byte was consumed, including bytes
    // only skipped over (`hexdump -C -s 1k small-file` still prints it).
    if data.is_empty() && base == 0 {
        return out;
    }
    let blocksize = formats.iter().map(Format::bytes_per_block).max().unwrap_or(0);
    if blocksize == 0 {
        return out;
    }
    let formats = auto_repeat(formats.to_vec(), blocksize);
    let mut prev: Option<&[u8]> = None;
    let mut starred = false;
    let mut pos = 0usize;
    while pos < data.len() {
        let end = (pos + blocksize).min(data.len());
        let block = &data[pos..end];
        if squeeze && prev == Some(block) {
            if !starred {
                out.extend(b"*");
                out.push(b'\n');
                starred = true;
            }
        } else {
            for format in &formats {
                render_format(format, block, base + pos as u64, &mut out);
            }
            starred = false;
        }
        prev = Some(block);
        pos += blocksize;
    }
    render_end_lines(&formats, end_address, &mut out);
    out
}

/// The builtin display modes as `-e` format strings, verbatim from
/// util-linux master hexdump.c (parse_args()).
const HEX_OFFT: &str = "\"%07.7_Ax\\n\"";

pub fn builtin_formats(mode: char) -> Option<Vec<Format>> {
    let specs: Vec<&str> = match mode {
        'b' => vec!["\"%07.7_ax \" 16/1 \"%03o \" \"\\n\"", HEX_OFFT],
        'X' => vec!["\"%07.7_ax \" 16/1 \" %02x \" \"\\n\"", HEX_OFFT],
        'c' => vec!["\"%07.7_ax \" 16/1 \"%3_c \" \"\\n\"", HEX_OFFT],
        'd' => vec!["\"%07.7_ax \" 8/2 \"  %05u \" \"\\n\"", HEX_OFFT],
        'o' => vec!["\"%07.7_ax \" 8/2 \" %06o \" \"\\n\"", HEX_OFFT],
        'x' => vec!["\"%07.7_ax \" 8/2 \"   %04x \" \"\\n\"", HEX_OFFT],
        'C' => vec![
            "\"%08.8_Ax\\n\"",
            "\"%08.8_ax  \" 8/1 \"%02x \" \"  \" 8/1 \"%02x \" ",
            "\"  |\" 16/1 \"%_p\" \"|\\n\"",
        ],
        _ => return None,
    };
    Some(
        specs
            .iter()
            .map(|s| Format { units: parse_format(s).expect("builtin format must parse") })
            .collect(),
    )
}

/// The format used when no display flag is given (util-linux hexdump.c):
/// two-byte hex like `-x`, but without the three leading blanks per item.
pub fn default_formats() -> Vec<Format> {
    let specs = ["\"%07.7_ax \" 8/2 \"%04x \" \"\\n\"", HEX_OFFT];
    specs
        .iter()
        .map(|s| Format { units: parse_format(s).expect("builtin format must parse") })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dump(data: &[u8], specs: &[&str]) -> String {
        let formats: Vec<Format> = specs
            .iter()
            .map(|s| Format { units: parse_format(s).unwrap() })
            .collect();
        String::from_utf8(display(data, &formats, true, 0)).unwrap()
    }

    /// Dump through the real builtin mode table, so the tests cover the same
    /// path the CLI uses.
    fn dump_mode(data: &[u8], mode: char) -> String {
        let formats = builtin_formats(mode).expect("builtin mode");
        String::from_utf8(display(data, &formats, true, 0)).unwrap()
    }

    #[test]
    fn parses_iteration_specs() {
        let units = parse_format("16/1 \"%02x \" \"\\n\"").unwrap();
        assert_eq!(units.len(), 2);
        assert_eq!(units[0].iterate, 16);
        assert_eq!(units[0].size, 1);
        assert!(units[0].explicit_reps);
        assert_eq!(units[1].bytes_per_block(), 0);
    }

    #[test]
    fn rejects_bad_unit_syntax() {
        // reps must be followed by whitespace or '/'
        assert!(parse_format("16\"x\"").is_err());
        // explicit size must be followed by whitespace
        assert!(parse_format("8/2\"x\"").is_err());
        assert!(parse_format("bogus").is_err());
        // `0 "x"` is accepted upstream (add_fmt has no zero check): a unit with
        // zero iterations renders nothing and contributes no bytes.
        assert!(parse_format("0 \"x\"").is_ok());
    }

    #[test]
    fn rejects_bad_conversions() {
        assert!(parse_format("\"%y\"").unwrap_err().message.contains("%y"));
        // %_a needs a base letter; bare %_a names itself
        assert_eq!(
            parse_format("\"%_a\"").unwrap_err().message,
            "bad conversion character %_a"
        );
        // uppercase X is not an address base
        assert_eq!(
            parse_format("\"%_aX\"").unwrap_err().message,
            "bad conversion character %_aX"
        );
        assert_eq!(
            parse_format("\"%_Ap\"").unwrap_err().message,
            "bad conversion character %_Ap"
        );
    }

    #[test]
    fn one_byte_octal_matches_oracle() {
        assert_eq!(
            dump_mode(b"0123456789abcdef", 'b'),
            "0000000 060 061 062 063 064 065 066 067 070 071 141 142 143 144 145 146\n0000010\n"
        );
        // A short final block is padded with blanks so the end-of-data address
        // still lines up: every display mode pads to the same 71-column line
        // (verified against util-linux 2.39.3).
        let short = dump_mode(b"abc", 'b');
        let first = short.lines().next().unwrap();
        assert_eq!(first.len(), 71);
        assert!(first.starts_with("0000000 141 142 143"));
        assert!(short.ends_with("\n0000003\n"));
    }

    #[test]
    fn two_byte_modes_match_oracle() {
        assert_eq!(
            dump_mode(b"0123456789abcdef", 'x'),
            "0000000    3130    3332    3534    3736    3938    6261    6463    6665\n0000010\n"
        );
        assert_eq!(
            dump_mode(b"abc", 'x'),
            "0000000    6261    0063                                                \n0000003\n"
        );
        assert_eq!(
            dump_mode(b"abc", 'd'),
            "0000000   25185   00099                                                \n0000003\n"
        );
        assert_eq!(
            dump_mode(b"abc", 'o'),
            "0000000  061141  000143                                                \n0000003\n"
        );
    }

    #[test]
    fn canonical_matches_oracle() {
        assert_eq!(
            dump_mode(b"0123456789abcdef", 'C'),
            "00000000  30 31 32 33 34 35 36 37  38 39 61 62 63 64 65 66  |0123456789abcdef|\n00000010\n"
        );
        assert_eq!(
            dump_mode(b"abc", 'C'),
            "00000000  61 62 63                                          |abc|\n00000003\n"
        );
    }

    #[test]
    fn squeeze_marks_identical_blocks() {
        let data = vec![b'a'; 32];
        let out = dump_mode(&data, 'b');
        assert_eq!(
            out,
            "0000000 141 141 141 141 141 141 141 141 141 141 141 141 141 141 141 141\n*\n0000020\n"
        );
    }

    #[test]
    fn nospace_drops_final_blank() {
        // A quoted section after a repetition count is a text-only unit of its
        // own, printed once per block: with a 4-byte block, 8 bytes of input
        // give two lines, each "four numbers, blank, newline".
        assert_eq!(
            dump(b"01234567", &["4/1 \"%u\" \" \\n\""]),
            "48495051 \n52535455 \n"
        );
    }

    #[test]
    fn signed_two_byte_negative() {
        // Two iterations over a 2-byte signed value: one real, one padded.
        assert_eq!(dump(&[0xff, 0xff], &["2/2 \"%d\\n\""]), "-1\n\n");
    }

    #[test]
    fn esc_char_table() {
        assert_eq!(esc_char(0), "\\0");
        assert_eq!(esc_char(b'\n'), "\\n");
        assert_eq!(esc_char(0x41), "A");
        assert_eq!(esc_char(0x20), " "); // isprint(): a space stays a space
        assert_eq!(esc_char(0xff), "377");
    }

    #[test]
    fn empty_input_produces_nothing() {
        assert_eq!(dump_mode(b"", 'C'), "");
    }

    #[test]
    fn auto_repeat_fills_block() {
        // A unit with an explicit repetition count is never auto-repeated, and
        // a text-only last unit contributes no bytes, so this format has a
        // 4-byte block: two lines of two 16-bit hex values.
        assert_eq!(dump(b"abcdefgh", &["2/2 \"%04x \" \"\\n\""]), "6261 6463\n6665 6867\n");
    }
}
