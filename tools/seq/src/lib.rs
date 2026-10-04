//! Number generation of `seq(1)`.
//!
//! Two things about this tool are not visible from its interface:
//!
//! * The default output format is not one thing. It is `%.PRECf` where `PREC` is
//!   the most fractional digits among **FIRST and STEP only** — LAST is left out,
//!   which is why `seq 3.25` prints `1 2 3` and not `1.00 2.00 3.00`. If any of
//!   the three is not a plain decimal, the format becomes `%g` instead.
//! * The values are stepped as `first + i * step` and not by repeated addition, and
//!   the last one gets a second look: if the number past LAST renders as LAST but
//!   renders *differently* from the one before, it is printed after all. That is
//!   what stops `seq 0 0.000001 0.000003` from losing its last digit.
//!
//! The quoting in the diagnostics comes from the `quoting` crate. `seq` does call
//! `setlocale`, unlike `colrm` and a few others, so which shape a diagnostic gets
//! depends on the locale here too.

use quoting::fancy_quotes;

/// The sentinel `seq` uses for "this operand is not a plain decimal, so it has no
/// precision to speak of". It is `INT_MAX` in the original.
pub const NO_PRECISION: i32 = i32::MAX;

/// Whether an argument that starts with `-` is a negative number rather than a
/// bundle of options.
///
/// This is deliberately not "parses as a number": `seq -3w` treats the whole
/// `-3w` as an operand and then complains that it is not a number, so a leading
/// numeric prefix is enough. A leading `.` counts, as `seq -.5` shows, and hex
/// does too, as `seq -0x3` shows. `nan` and `inf` have no digits to offer, which
/// is why `seq -inf` is taken as options and complains about `-i`.
pub fn looks_like_number(arg: &str) -> bool {
    let rest = match arg.strip_prefix('-') {
        Some(rest) => rest,
        None => return false,
    };
    let bytes = rest.as_bytes();
    let mut at = 0usize;
    let hex = bytes.len() >= 2 && bytes[0] == b'0' && (bytes[1] == b'x' || bytes[1] == b'X');
    if hex {
        at = 2;
    }
    let digit = |byte: u8| if hex { byte.is_ascii_hexdigit() } else { byte.is_ascii_digit() };
    let before_point = {
        let start = at;
        while at < bytes.len() && digit(bytes[at]) {
            at += 1;
        }
        at > start
    };
    let mut after_point = 0usize;
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        let start = at;
        while at < bytes.len() && digit(bytes[at]) {
            at += 1;
        }
        after_point = at - start;
    }
    if !before_point && after_point == 0 {
        return false;
    }
    // An exponent is part of the number only when it is complete; `-3e` is still
    // an operand, because the numeric prefix `-3` is there either way.
    if let Some(&marker) = bytes.get(at) {
        if marker == b'e' || marker == b'E' {
            let mut next = at + 1;
            if matches!(bytes.get(next), Some(b'+') | Some(b'-')) {
                next += 1;
            }
            let start = next;
            while next < bytes.len() && bytes[next].is_ascii_digit() {
                next += 1;
            }
            if next > start {
                at = next;
            }
        }
    }
    // Anything left over is ignored: `-3w` is an operand whose text is not a
    // number, and the operand stage is what says so.
    at <= bytes.len()
}

/// One number from the command line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Operand {
    pub value: f64,
    /// Width of the number as it was written, needed by `-w`. Zero means "let the
    /// format decide", which is what an operand with an exponent gets.
    pub width: usize,
    /// Fractional digits, or [`NO_PRECISION`].
    pub precision: i32,
}

/// Parse a number the way `xstrtold` does.
///
/// The standard library's parser has no hexadecimal form, and `seq` takes one:
/// `seq -0x3` is an operand with the value -3, not a bundle of options.
fn parse_number(text: &str) -> Option<f64> {
    let (sign, rest) = match text.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, text.strip_prefix('+').unwrap_or(text)),
    };
    let hex = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X"));
    let rest = match hex {
        Some(rest) => rest,
        None => return text.trim().parse().ok(),
    };
    let (mantissa, exponent) = match rest.find(['p', 'P']) {
        Some(at) => (&rest[..at], rest[at + 1..].parse::<i32>().ok()?),
        None => (rest, 0),
    };
    let (whole, fraction) = match mantissa.find('.') {
        Some(at) => (&mantissa[..at], &mantissa[at + 1..]),
        None => (mantissa, ""),
    };
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    let mut value = 0.0f64;
    for byte in whole.bytes() {
        value = value * 16.0 + f64::from((byte as char).to_digit(16)? as u8);
    }
    let mut scale = 1.0f64 / 16.0;
    for byte in fraction.bytes() {
        value += f64::from((byte as char).to_digit(16)? as u8) * scale;
        scale /= 16.0;
    }
    Some(sign * value * 2.0f64.powi(exponent))
}

/// What `scan_arg` makes of one argument.
///
/// The width bookkeeping is fiddly and only `-w` ever looks at it, but it is what
/// decides whether `seq -w 8 10` pads to two columns and `seq -w 08 10` pads to
/// three.
pub fn scan_arg(text: &str) -> Option<Operand> {
    let leading = text.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '+');
    let value = parse_number(text.trim())?;
    if value.is_nan() {
        // `seq nan` is refused with its own wording, which the caller reports; a
        // NaN cannot reach the value here without being caught first.
        return None;
    }
    let mut operand = Operand { value, width: 0, precision: NO_PRECISION };

    let decimal_point = leading.find('.');
    let is_hex = leading.contains(['x', 'X']);
    // Anything with a point or a binary exponent keeps the sentinel until the
    // adjustments below work out how many digits it really needs; a plain integer
    // needs none, which is also the case that lets the tool generate the numbers
    // as text.
    if decimal_point.is_none() && !leading.contains(['p', 'P']) {
        operand.precision = 0;
    }
    if !is_hex && value.is_finite() {
        let mut fraction_len = 0usize;
        operand.width = leading.len();
        if let Some(point) = decimal_point {
            fraction_len = leading[point + 1..]
                .find(['e', 'E'])
                .unwrap_or(leading.len() - point - 1);
            if fraction_len <= NO_PRECISION as usize {
                operand.precision = fraction_len as i32;
            }
            // `#.` and `-.#` both lose the point, and a leading `.` gains a zero.
            operand.width += if fraction_len == 0 {
                -1
            } else if {
                point == 0 || !leading.as_bytes()[point - 1].is_ascii_digit()
            } {
                1
            } else {
                0
            } as isize as usize;
        }
        let exponent_at = leading.find(['e', 'E']);
        if let Some(at) = exponent_at {
            let exponent: i64 = leading[at + 1..].parse().unwrap_or(i64::MIN);
            let exponent = exponent.max(-(i64::MAX / 2));
            if exponent < 0 {
                operand.precision += (-exponent) as i32;
            } else {
                operand.precision -= (operand.precision as i64).min(exponent) as i32;
            }
            // The exponent itself is never printed.
            operand.width = operand.width.saturating_sub(leading.len() - at);
            if exponent < 0 {
                if decimal_point.is_some() {
                    if at == decimal_point.unwrap() + 1 {
                        operand.width += 1;
                    }
                } else {
                    operand.width += 1;
                }
            } else if decimal_point.is_some()
                && operand.precision == 0
                && fraction_len > 0
            {
                operand.width = operand.width.saturating_sub(1);
            }
            if exponent < 0 {
                operand.width += (-exponent) as usize;
            } else {
                operand.width += (exponent - fraction_len.min(exponent as usize) as i64) as usize;
            }
        }
    }
    Some(operand)
}

/// How one number is put together on the page: what comes before the number, and
/// what comes after it but still belongs to the same item.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    pub prefix_len: usize,
    pub suffix_len: usize,
}

/// Why a `-f` format string was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatError {
    /// No `%` directive at all. `%%` on its own counts as none, because it is the
    /// literal percent rather than a directive.
    NoDirective,
    /// The string stops in the middle of a directive.
    EndsWithPercent,
    /// A conversion `seq` does not pass through to printf.
    UnknownDirective { conversion: char },
    /// A second directive, which `seq` will not format.
    TooManyDirectives,
}

impl FormatError {
    pub fn message(&self, format: &str) -> String {
        self.message_in(format, fancy_quotes())
    }

    /// The message with the quoting shape spelled out, so a caller that knows the
    /// locale need not set it first.
    pub fn message_in(&self, format: &str, fancy: bool) -> String {
        let quoted = quoting::quote_for(format, fancy);
        match self {
            FormatError::NoDirective => format!("seq: format {} has no % directive", quoted),
            FormatError::EndsWithPercent => format!("seq: format {} ends in %", quoted),
            FormatError::UnknownDirective { conversion } => {
                format!("seq: format {} has unknown %{} directive", quoted, conversion)
            }
            FormatError::TooManyDirectives => {
                format!("seq: format {} has too many %% directives", quoted)
            }
        }
    }
}

/// The floating-point conversions `seq` lets through, before and after the `L`
/// length modifier it inserts.
const CONVERSIONS: &[u8] = b"efgaEFGA";

/// Check a `-f` format string and describe how one number fits into it.
///
/// The format is not simply handed to printf: `seq` rewrites it into the
/// `long double` form, which means finding the one directive, counting what is
/// before and after it, and refusing anything that would not survive the rewrite.
pub fn check_format(format: &str) -> Result<Layout, FormatError> {
    let bytes = format.as_bytes();
    let mut at = 0usize;
    let mut prefix_len = 0usize;
    // The first `%` that is not the first half of a `%%`. The count is of
    // characters as they end up in the output, so `a%%b` is three of them, not
    // four: `%%` prints as one `%`.
    while at < bytes.len() {
        if bytes[at] == b'%' && bytes.get(at + 1) != Some(&b'%') {
            break;
        }
        prefix_len += 1;
        at += usize::from(bytes[at] == b'%') + 1;
    }
    if at >= bytes.len() {
        return Err(FormatError::NoDirective);
    }
    at += 1;
    // Flags, in any order and any number of times.
    while matches!(bytes.get(at), Some(b'-') | Some(b'+') | Some(b'#') | Some(b'0')
                         | Some(b' ') | Some(b'\''))
    {
        at += 1;
    }
    // Width, then precision. Either may be absent.
    while matches!(bytes.get(at), Some(byte) if byte.is_ascii_digit()) {
        at += 1;
    }
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        while matches!(bytes.get(at), Some(byte) if byte.is_ascii_digit()) {
            at += 1;
        }
    }
    let has_long = bytes.get(at) == Some(&b'L');
    if has_long {
        at += 1;
    }
    let conversion = match bytes.get(at) {
        None => return Err(FormatError::EndsWithPercent),
        Some(byte) => *byte,
    };
    if !CONVERSIONS.contains(&conversion) {
        return Err(FormatError::UnknownDirective {
            // A second `.` lands here as a "conversion", which is why
            // `seq -f '%5.3.2g'` complains about `%.'.
            conversion: conversion as char,
        });
    }
    at += 1;
    // Whatever is left is the suffix, but a second directive is refused: only one
    // number is passed to the format.
    let mut suffix_len = 0usize;
    loop {
        if bytes.get(at) == Some(&b'%') && bytes.get(at + 1) != Some(&b'%') {
            return Err(FormatError::TooManyDirectives);
        }
        match bytes.get(at) {
            None => break,
            Some(_) => {
                suffix_len += 1;
                at += usize::from(bytes[at] == b'%') + 1;
            }
        }
    }
    Ok(Layout { prefix_len, suffix_len })
}

/// Drop the `L` length modifier from a format, if it has one.
///
/// `seq` rewrites the user's format into its `long double` form and passes a
/// `long double`. Rust cannot name that type, so the value goes across as a
/// `double` and the `L` has to go with it: leaving it in place would have printf
/// read the argument as a `long double` out of a slot holding something else, and
/// print `nan`. See the handover note on `seq` for what this costs.
pub fn strip_long_double(format: &str) -> String {
    let bytes = format.as_bytes();
    let mut out = String::with_capacity(format.len());
    let mut at = 0usize;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            out.push('%');
            at += 1;
            while matches!(bytes.get(at), Some(b'-') | Some(b'+') | Some(b'#') | Some(b'0')
                               | Some(b' ') | Some(b'\''))
            {
                out.push(bytes[at] as char);
                at += 1;
            }
            while matches!(bytes.get(at), Some(byte) if byte.is_ascii_digit()) {
                out.push(bytes[at] as char);
                at += 1;
            }
            if bytes.get(at) == Some(&b'.') {
                out.push('.');
                at += 1;
                while matches!(bytes.get(at), Some(byte) if byte.is_ascii_digit()) {
                    out.push(bytes[at] as char);
                    at += 1;
                }
            }
            // The modifier itself goes: what follows is the conversion.
            while matches!(bytes.get(at), Some(b'l') | Some(b'L') | Some(b'h')
                               | Some(b'j') | Some(b'z') | Some(b't'))
            {
                at += 1;
            }
            if let Some(byte) = bytes.get(at) {
                out.push(*byte as char);
                at += 1;
            }
            continue;
        }
        out.push(bytes[at] as char);
        at += 1;
    }
    out
}

/// The default format for the three operands, as a printf format string.
///
/// `PREC` comes from FIRST and STEP alone, and any operand that is not a plain
/// decimal turns the whole thing into `%g`.
pub fn default_format(
    first: Operand,
    step: Operand,
    last: Operand,
    equal_width: bool,
) -> String {
    let prec = first.precision.max(step.precision);
    if prec == NO_PRECISION || last.precision == NO_PRECISION {
        return "%g".to_string();
    }
    if !equal_width {
        return format!("%.{}f", prec);
    }
    // With -w the width is the widest number either end will print as, adjusted
    // for the precision the two ends share.
    let mut first_width = first.width as i64 + (prec - first.precision) as i64;
    let mut last_width = last.width as i64 + (prec - last.precision) as i64;
    if last.precision != 0 && prec == 0 {
        // A last operand with a point but no digits stops needing room for it.
        last_width -= 1;
    }
    if last.precision == 0 && prec != 0 {
        // ...and one without a point starts needing it.
        last_width += 1;
    }
    if first.precision == 0 && prec != 0 {
        first_width += 1;
    }
    let width = first_width.max(last_width).max(0);
    format!("%0{}.{}f", width, prec)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operand(text: &str) -> Operand {
        scan_arg(text).unwrap_or(Operand { value: 0.0, width: 0, precision: NO_PRECISION })
    }

    #[test]
    fn a_leading_dash_only_makes_an_operand_when_a_number_follows() {
        // Negative numbers are operands, not option bundles.
        for good in ["-1", "-0", "-.5", "-3.5", "-1e-3", "-3e2", "-0x3", "-3e", "-3w"] {
            assert!(looks_like_number(good), "{good} should look like a number");
        }
        // These are option bundles, and `seq -inf` says so by complaining about
        // the `-i`.
        for bad in ["-w", "-x", "-inf", "-nan", "-Infinity", "-", "-e5"] {
            assert!(!looks_like_number(bad), "{bad} should not look like a number");
        }
        assert!(!looks_like_number("1"), "a positive number needs no help");
        assert!(!looks_like_number("-f%g"));
    }

    #[test]
    fn an_integer_needs_no_precision() {
        assert_eq!(operand("5").precision, 0);
        assert_eq!(operand("0.25").precision, 2);
        assert_eq!(operand("1.5").precision, 1);
        assert_eq!(operand("3.25").precision, 2);
        // An exponent shifts the precision rather than removing it: `seq 1e-3 0.1`
        // prints `0.001`, which is three places.
        assert_eq!(operand("1e-3").precision, 3);
        assert_eq!(operand("1e3").precision, 0);
        assert_eq!(operand("1.5e2").precision, 0);
        // A hexadecimal operand with a point or a binary exponent keeps the
        // sentinel, because nothing about it says how many digits were meant.
        // One written as a plain integer is just an integer.
        assert_eq!(operand("-0x3").precision, 0);
        assert_eq!(operand("0x10").precision, 0);
        assert_eq!(operand("0x1.8p1").precision, NO_PRECISION);
        assert_eq!(operand("5").value, 5.0);
        assert_eq!(operand("-0.5").value, -0.5);
        // Hexadecimal floats are numbers here, not option bundles.
        assert_eq!(operand("-0x3").value, -3.0);
        assert_eq!(operand("0x1.8p1").value, 3.0);
        assert_eq!(operand("0x10").value, 16.0);
    }

    #[test]
    fn the_default_precision_ignores_the_last_operand() {
        // `seq 3.25` prints 1 2 3, so LAST's two fractional digits do not count.
        assert_eq!(default_format(operand("1"), operand("1"), operand("3.25"), false), "%.0f");
        assert_eq!(default_format(operand("1.5"), operand("1"), operand("3"), false), "%.1f");
        assert_eq!(default_format(operand("0"), operand("0.25"), operand("1"), false), "%.2f");
        // An exponent widens the precision instead of forcing %g.
        assert_eq!(default_format(operand("1e-3"), operand("0.1"), operand("0.2"), false), "%.3f");
        assert_eq!(default_format(operand("1e3"), operand("1"), operand("9"), false), "%.0f");
        // A hexadecimal operand with a point has no precision to speak of, so
        // that is what falls back to %g; one written as a plain integer does not.
        assert_eq!(default_format(operand("0x10"), operand("1"), operand("0x20"), false), "%.0f");
        assert_eq!(default_format(operand("0x1.8p1"), operand("0x1"), operand("0x4"), false), "%g");
    }

    #[test]
    fn equal_width_pads_to_the_widest_end() {
        assert_eq!(default_format(operand("8"), operand("1"), operand("10"), true), "%02.0f");
        // A leading zero is part of the width, so `08` and `10` still agree on two.
        assert_eq!(default_format(operand("08"), operand("1"), operand("10"), true), "%02.0f");
        assert_eq!(default_format(operand("0"), operand("1"), operand("100"), true), "%03.0f");
        assert_eq!(default_format(operand("-8"), operand("1"), operand("10"), true), "%02.0f");
        assert_eq!(default_format(operand("8.5"), operand("1"), operand("10"), true), "%04.1f");
        assert_eq!(default_format(operand("0.5"), operand("1"), operand("10"), true), "%04.1f");
        // Both ends need room for the point the shared precision introduces.
        assert_eq!(default_format(operand("1"), operand("0.5"), operand("10"), true), "%04.1f");
        // The shared precision makes room for the point on both ends.
        assert_eq!(default_format(operand("1"), operand("0.5"), operand("2"), true), "%03.1f");
        assert_eq!(default_format(operand("1"), operand("0.25"), operand("2"), true), "%04.2f");
        // A number already as wide as the format is not padded further.
        assert_eq!(default_format(operand("1"), operand("1"), operand("10"), true), "%02.0f");
    }

    #[test]
    fn format_check_accepts_what_seq_will_pass_on() {
        for good in ["%g", "%f", "%e", "%a", "%E", "%G", "%F", "%A", "%Lg", "%.2Lf",
                     "%10.3g", "%-10.3g|", "%010.3g", "%#g", "% g", "%+g", "%0g", "%.g",
                     "%--g", "%-+g", "x%gy", "%.2f", "%%%g", "%g%%", "a%%b%gc"] {
            assert_eq!(check_format(good).is_ok(), true, "{good} should be accepted");
        }
    }

    #[test]
    fn format_check_refuses_what_seq_will_not() {
        assert_eq!(check_format(""), Err(FormatError::NoDirective));
        // `%%` is the literal percent, so on its own it is no directive at all.
        assert_eq!(check_format("%%"), Err(FormatError::NoDirective));
        assert_eq!(check_format("x%%y"), Err(FormatError::NoDirective));
        assert_eq!(check_format("%%g"), Err(FormatError::NoDirective));
        assert_eq!(check_format("%"), Err(FormatError::EndsWithPercent));
        assert_eq!(check_format("%5"), Err(FormatError::EndsWithPercent));
        assert_eq!(check_format("%L"), Err(FormatError::EndsWithPercent));
        // A second `.` is not a flag, so it is read as the conversion.
        assert_eq!(
            check_format("%5.3.2g"),
            Err(FormatError::UnknownDirective { conversion: '.' })
        );
        assert_eq!(check_format("%lf"), Err(FormatError::UnknownDirective { conversion: 'l' }));
        assert_eq!(check_format("%d"), Err(FormatError::UnknownDirective { conversion: 'd' }));
        // Only one number is passed to the format, so only one directive is allowed.
        assert_eq!(check_format("%g %g"), Err(FormatError::TooManyDirectives));
        assert_eq!(check_format("x%gy%gz"), Err(FormatError::TooManyDirectives));
    }

    #[test]
    fn the_long_double_modifier_is_dropped_for_the_value_we_pass() {
        assert_eq!(strip_long_double("%Lg"), "%g");
        assert_eq!(strip_long_double("%.2Lf"), "%.2f");
        assert_eq!(strip_long_double("%10.3Lg|"), "%10.3g|");
        assert_eq!(strip_long_double("%-8Lg"), "%-8g");
        // Nothing to strip leaves the format exactly as it was.
        assert_eq!(strip_long_double("%g"), "%g");
        assert_eq!(strip_long_double("x%gy"), "x%gy");
        assert_eq!(strip_long_double("%%"), "%%");
        assert_eq!(strip_long_double(""), "");
    }

    #[test]
    fn the_layout_measures_what_belongs_to_one_number() {
        assert_eq!(check_format("%g").unwrap(), Layout { prefix_len: 0, suffix_len: 0 });
        assert_eq!(check_format("x%gy").unwrap(), Layout { prefix_len: 1, suffix_len: 1 });
        assert_eq!(check_format("%10.3g|").unwrap(), Layout { prefix_len: 0, suffix_len: 1 });
        // `a%%b` prints as `a%b`, so the prefix is three characters wide.
        assert_eq!(check_format("a%%b%gc").unwrap(), Layout { prefix_len: 3, suffix_len: 1 });
        assert_eq!(check_format("%%g%gc").unwrap(), Layout { prefix_len: 2, suffix_len: 1 });
        assert_eq!(check_format("%.2Lf").unwrap(), Layout { prefix_len: 0, suffix_len: 0 });
    }

    #[test]
    fn format_error_messages_name_the_format_and_the_directive() {
        // Nothing here opens a locale, so the plain quoting shape is what the
        // one-argument form gives; the fancy shape is spelled out separately.
        assert_eq!(
            FormatError::NoDirective.message("x"),
            "seq: format 'x' has no % directive"
        );
        assert_eq!(
            FormatError::NoDirective.message_in("x", true),
            "seq: format ‘x’ has no % directive"
        );
        assert_eq!(FormatError::EndsWithPercent.message("%5"), "seq: format '%5' ends in %");
        assert_eq!(
            FormatError::UnknownDirective { conversion: 'd' }.message("%d"),
            "seq: format '%d' has unknown %d directive"
        );
        assert_eq!(
            FormatError::TooManyDirectives.message("%g %g"),
            "seq: format '%g %g' has too many %% directives"
        );
    }
}