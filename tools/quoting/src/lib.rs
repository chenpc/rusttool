//! The two quoting styles GNU tools put in their diagnostics.
//!
//! There are two of them and they are not variations of one another:
//!
//! * `quote()` wraps the text in quotes and escapes only what would otherwise be
//!   unreadable. Which quotes it uses — and whether a byte the C locale cannot
//!   print is escaped at all — follows the **locale**, so the same argument can be
//!   reported as `‘a’` or `'a'` on the same machine.
//! * `quoteaf()` always produces something a shell would accept, splitting the
//!   string into runs when it has control characters in it.
//!
//! `quote()` is the one that follows the locale, so a program has to call
//! `setlocale(LC_ALL, "")` the way `env.c` and `seq.c` do and then report what
//! [`fancy_quotes`] says. A tool that never calls `setlocale` — `colrm`, for one —
//! stays in the C locale for its whole run and always gets the plain shape; pass
//! `false` explicitly in that case rather than leaving it to the process locale.
//!
//! This is the second crate to need these: `env` and `seq` each wanted them, and
//! every remaining coreutils tool prints at least one diagnostic.

use std::sync::atomic::{AtomicBool, Ordering};

/// Whether `quote` should use the fancy U+2018/U+2019 quotes.
///
/// A UTF-8 codeset gets the fancy quotes and lets the bytes of a multi-byte
/// character through; anything else gets plain `'...'` and escapes every byte the
/// C locale cannot print.
static FANCY_QUOTES: AtomicBool = AtomicBool::new(false);

/// Record what `nl_langinfo(CODESET)` said about the locale.
pub fn set_fancy_quotes(fancy: bool) {
    FANCY_QUOTES.store(fancy, Ordering::Relaxed);
}

/// Whether the fancy quotes are in use.
pub fn fancy_quotes() -> bool {
    FANCY_QUOTES.load(Ordering::Relaxed)
}

/// Open the locale and record the quoting style that follows from it.
///
/// Returns the codeset, for a tool that wants to report it.
pub fn open_locale() -> String {
    // SAFETY: both calls take plain C strings, and `environ` is what the C
    // library is already using.
    unsafe {
        libc::setlocale(libc::LC_ALL, b"\0".as_ptr() as *const libc::c_char);
        let codeset = libc::nl_langinfo(libc::CODESET);
        let name = if codeset.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(codeset).to_string_lossy().into_owned()
        };
        set_fancy_quotes(name == "UTF-8");
        name
    }
}

/// Render `text` the way gnulib's `quote()` does for a diagnostic: always
/// wrapped in quotes, with only a backslash and the unprintable characters
/// escaped. The quote character itself is escaped only in the plain shape, which
/// is what makes `env: ‘a'b'` look broken and is what the system tool prints.
pub fn quote(text: &str) -> String {
    quote_for(text, fancy_quotes())
}

/// The two shapes `quote` takes, split out so both can be tested without having to
/// change the process locale.
pub fn quote_for(text: &str, fancy: bool) -> String {
    let mut out: Vec<u8> = Vec::with_capacity(text.len() + 2);
    if fancy {
        out.extend_from_slice(&[0xe2, 0x80, 0x98]);
    } else {
        out.push(b'\'');
    }
    for &byte in text.as_bytes() {
        match byte {
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'\'' if !fancy => out.extend_from_slice(b"\\'"),
            0x07 => out.extend_from_slice(b"\\a"),
            0x08 => out.extend_from_slice(b"\\b"),
            0x0c => out.extend_from_slice(b"\\x0C"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\t' => out.extend_from_slice(b"\\t"),
            0x0b => out.extend_from_slice(b"\\v"),
            // In a UTF-8 locale a byte this high is part of a character and is left
            // alone; in the C locale it is unprintable, so it is escaped.
            0x80..=0xff if fancy => out.push(byte),
            0x20..=0x7e => out.push(byte),
            _ => out.extend_from_slice(format!("\\{:03o}", byte).as_bytes()),
        }
    }
    if fancy {
        out.extend_from_slice(&[0xe2, 0x80, 0x99]);
    } else {
        out.push(b'\'');
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Render `text` the way `quoteaf` does: `shell_escape_always_quoting_style`
/// through gnulib's quotearg, which is a different function from `quote` and does
/// not look at the locale.
///
/// The shape follows what a shell would accept:
///
/// * the whole text is wrapped in `'...'`;
/// * a `'` inside it becomes `'\''`, the only way to get one into single quotes;
/// * a character the C locale cannot print opens a `$'...'` group, in which it is
///   written `\n` or as three octal digits;
/// * that group is closed with `''` before ordinary text resumes.
///
/// The characters that rule out the shorter double-quoted form are the ones a
/// shell still acts on inside them: `!`, `"`, `$`, `&`, `(`, `)`, `*`, `;`, `<`,
/// `=`, `>`, `?`, `[`, `\`, `^`, a backtick, `{`, `|`, `}`, `~` and `#`.
pub fn quoteaf(text: &str) -> String {
    let bytes = text.as_bytes();
    // A text that has an apostrophe but nothing a shell acts on inside double
    // quotes is shorter wrapped in them, and quotearg takes that path.
    if bytes.contains(&b'\'')
        && bytes.iter().all(|byte| printable(*byte))
        && !bytes.iter().any(|byte| special_in_double_quotes(*byte))
    {
        return format!("\"{}\"", text);
    }
    let mut out: Vec<u8> = Vec::new();
    out.push(b'\'');
    // Whether a `$'...'` group is still open, carried across characters exactly
    // as quotearg carries `pending_shell_escape_end`.
    let mut group_open = false;
    let mut at = 0usize;
    while at < bytes.len() {
        let byte = bytes[at];
        if byte == b'\'' {
            // A quote character cannot appear inside single quotes, so the string
            // is closed, an escaped quote is written, and the string reopens.
            if group_open {
                out.extend_from_slice(b"''");
                group_open = false;
            }
            out.extend_from_slice(b"'\\''");
            at += 1;
            continue;
        }
        if printable(byte) {
            // Ordinary text: close an open group first, then the character.
            if group_open {
                out.extend_from_slice(b"''");
                group_open = false;
            }
            out.push(byte);
            at += 1;
            continue;
        }
        // An unprintable one: open a group unless one is already open, and escape
        // the character.
        if !group_open {
            out.extend_from_slice(b"'$'");
            group_open = true;
        }
        out.push(b'\\');
        out.extend_from_slice(escape_byte(byte).as_bytes());
        at += 1;
    }
    out.push(b'\'');
    String::from_utf8_lossy(&out).into_owned()
}

/// What a control byte is written as inside `$'...'`.
///
/// The backslash is not part of this: quotearg's `START_ESC` stores it once, and
/// this is only the remainder.
fn escape_byte(byte: u8) -> String {
    match byte {
        0x07 => "a".to_string(),
        0x08 => "b".to_string(),
        0x0b => "v".to_string(),
        0x0c => "f".to_string(),
        b'\n' => "n".to_string(),
        b'\r' => "r".to_string(),
        b'\t' => "t".to_string(),
        // A backslash is never escaped in the shell style: the surrounding single
        // quotes already protect it.
        b'\\' => "\\".to_string(),
        other => format!("{:03o}", other),
    }
}

/// Whether the C locale can print this byte on its own.
fn printable(byte: u8) -> bool {
    (0x20..0x7f).contains(&byte)
}

/// Whether a text with this byte in it has to stay in single quotes rather than
/// being wrapped in double quotes.
pub fn special_in_double_quotes(byte: u8) -> bool {
    matches!(
        byte,
        b'!' | b'"'
            | b'$'
            | b'&'
            | b'('
            | b')'
            | b'*'
            | b';'
            | b'<'
            | b'='
            | b'>'
            | b'?'
            | b'['
            | b'\\'
            | b'^'
            | b'`'
            | b'{'
            | b'|'
            | b'}'
            | b'~'
            | b'#'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_wraps_and_escapes_only_what_would_be_unreadable() {
        assert_eq!(quote_for("abc", true), "\u{2018}abc\u{2019}");
        assert_eq!(quote_for("", true), "\u{2018}\u{2019}");
        assert_eq!(quote_for("a b", true), "\u{2018}a b\u{2019}");
        // A single quote is left alone in the fancy shape, which looks wrong and
        // is what the system tool prints.
        assert_eq!(quote_for("a'b", true), "\u{2018}a'b\u{2019}");
        assert_eq!(quote_for("a\\b", true), "\u{2018}a\\\\b\u{2019}");
        assert_eq!(quote_for("a\nb", true), "\u{2018}a\\nb\u{2019}");
        assert_eq!(quote_for("a\tb", true), "\u{2018}a\\tb\u{2019}");
        assert_eq!(quote_for("a\rb", true), "\u{2018}a\\rb\u{2019}");
        assert_eq!(quote_for("a\0b", true), "\u{2018}a\\000b\u{2019}");
        assert_eq!(quote_for("a\x1b[0m", true), "\u{2018}a\\033[0m\u{2019}");
        assert_eq!(quote_for("a\x7fb", true), "\u{2018}a\\177b\u{2019}");
        // The bytes of a multi-byte character are passed through, not mangled.
        assert_eq!(quote_for("\u{e9}", true), "\u{2018}\u{e9}\u{2019}");
    }

    #[test]
    fn quote_without_a_utf8_locale_uses_plain_quotes() {
        assert_eq!(quote_for("abc", false), "'abc'");
        assert_eq!(quote_for("", false), "''");
        assert_eq!(quote_for("a b", false), "'a b'");
        // A quote character is escaped here, but not in the fancy shape.
        assert_eq!(quote_for("a'b", false), "'a\\'b'");
        assert_eq!(quote_for("'", false), "'\\''");
        assert_eq!(quote_for("a'b\"c", false), "'a\\'b\"c'");
        assert_eq!(quote_for("a\\b", false), "'a\\\\b'");
        assert_eq!(quote_for("a\nb", false), "'a\\nb'");
        assert_eq!(quote_for("a\0b", false), "'a\\000b'");
        assert_eq!(quote_for("a\x7fb", false), "'a\\177b'");
        // A byte the C locale cannot print becomes octal here.
        assert_eq!(quote_for("\u{e9}", false), "'\\303\\251'");
    }

    #[test]
    fn quoteaf_produces_something_a_shell_would_accept() {
        // Every one of these was compared against `env -C`, which is the only
        // caller in this project, byte for byte.
        assert_eq!(quoteaf(""), "''");
        assert_eq!(quoteaf("abc"), "'abc'");
        assert_eq!(quoteaf("/nosuchdir"), "'/nosuchdir'");
        assert_eq!(quoteaf("a b"), "'a b'");
        assert_eq!(quoteaf("a\"b"), "'a\"b'");
        assert_eq!(quoteaf("\"q\""), "'\"q\"'");
        // An apostrophe with nothing a shell acts on inside double quotes is
        // shorter wrapped in them.
        assert_eq!(quoteaf("a'b"), "\"a'b\"");
        assert_eq!(quoteaf("'x"), "\"'x\"");
        assert_eq!(quoteaf("x'"), "\"x'\"");
        // A backslash is still an escape inside double quotes, so it rules them out.
        assert_eq!(quoteaf("a'b\\c"), "'a'\\''b\\c'");
        // Anything with a `"` in it stays in single quotes and escapes the quote.
        assert_eq!(quoteaf("a\"b'c"), "'a\"b'\\''c'");
        assert_eq!(quoteaf("a'b\"c"), "'a'\\''b\"c'");
    }

    #[test]
    fn quoteaf_groups_unprintable_characters() {
        // A byte the C locale cannot print opens a `$'...'` group, closed with
        // `''` once ordinary text resumes.
        assert_eq!(quoteaf("a\nb"), "'a'$'\\n''b'");
        assert_eq!(quoteaf("a\tb"), "'a'$'\\t''b'");
        assert_eq!(quoteaf("a\u{7}b"), "'a'$'\\a''b'");
        assert_eq!(quoteaf("a\u{1}b"), "'a'$'\\001''b'");
        assert_eq!(quoteaf("a\u{7f}b"), "'a'$'\\177''b'");
        assert_eq!(quoteaf("a'b\nc"), "'a'\\''b'$'\\n''c'");
        // An empty run before the group shows as an empty quoted run.
        assert_eq!(quoteaf("\na"), "''$'\\n''a'");
        assert_eq!(quoteaf("\n"), "''$'\\n'");
        assert_eq!(quoteaf("a\n"), "'a'$'\\n'");
    }

    #[test]
    fn quoteaf_characters_special_in_double_quotes_are_the_measured_set() {
        // `env -C` was asked about `a'b<c>` for every printable ASCII character.
        // These keep the text in single quotes; everything else allows double
        // quotes, which is only visible when there is also an apostrophe.
        for byte in b"!\"$&()*;<=>?[\\^`{|}~#" {
            let text = format!("a'b{}c", *byte as char);
            assert!(
                quoteaf(&text).starts_with('\''),
                "{text:?} should stay in single quotes"
            );
        }
        for byte in b" %'+,-./0123456789:@ABCDEFGHIJKLMNOPQRSTUVWXYZ]_abcdefghijklmnopqrstuvwxyz" {
            let text = format!("a'b{}c", *byte as char);
            assert!(
                quoteaf(&text).starts_with('"'),
                "{text:?} should be wrapped in double quotes"
            );
        }
    }

    #[test]
    fn quoteaf_diverges_from_the_system_tool_on_one_shape() {
        // Recorded rather than papered over: an apostrophe together with a run of
        // two or more unprintable characters gets one extra quote character from
        // the system tool, and where that comes from in quotearg's read-only
        // rescan is not something the source makes obvious. It needs a directory
        // name holding both an apostrophe and a run of control characters, so it
        // is left as a known difference rather than guessed at.
        assert_eq!(quoteaf("a'b\n\n"), "'a'\\''b'$'\\n\\n'");
        assert_eq!(quoteaf("a'b\n"), "'a'\\''b'$'\\n'");
    }

    #[test]
    fn the_flag_defaults_to_the_plain_shape() {
        // Nothing here opens a locale, so the plain shape is what comes out; the
        // two tests above cover the fancy one directly.
        set_fancy_quotes(false);
        assert!(!fancy_quotes());
        assert_eq!(quote("x"), "'x'");
    }
}
/// The third quoting style: what a shell would need to quote a word safely.
/// Unlike `quote`, this one leaves a plain word alone - `md5sum: missing.txt` is
/// printed bare, while `md5sum: 'a b.txt'` is not. Control characters become
/// `$'\n'` segments rather than escapes inside the quotes.
pub fn shell_quote_meta(text: &str) -> String {
    if !needs_meta(text) {
        return text.to_string();
    }
    let mut out = String::new();
    let mut run = String::new();
    for &byte in text.as_bytes() {
        if byte == b'\'' {
            // A single quote cannot go inside single quotes, so the whole thing
            // takes double quotes instead.
            return double_quote_meta(text);
        }
        if is_control(byte) {
            if !run.is_empty() {
                out.push('\'');
                out.push_str(&run);
                out.push('\'');
                run.clear();
            }
            out.push_str(&format!("$'{}'", control_name(byte)));
            continue;
        }
        run.push(char::from(byte));
    }
    if !run.is_empty() {
        out.push('\'');
        out.push_str(&run);
        out.push('\'');
    }
    out
}

fn double_quote_meta(text: &str) -> String {
    let open = if fancy_quotes() { "\u{2018}" } else { "\"" };
    let close = if fancy_quotes() { "\u{2019}" } else { "\"" };
    let mut out = String::from(open);
    for &byte in text.as_bytes() {
        if is_control(byte) {
            out.push_str(&format!("$'{}'", control_name(byte)));
        } else {
            out.push(char::from(byte));
        }
    }
    out.push_str(close);
    out
}

/// The characters that make a shell word unsafe to print bare.
fn needs_meta(text: &str) -> bool {
    if text.is_empty() {
        return true;
    }
    text.bytes().any(|b| {
        is_control(b)
            || matches!(
                b,
                b' ' | b'!' | b'"' | b'$' | b'\'' | b'&' | b'(' | b')' | b'*' | b'<' | b'>' | b'?'
                    | b'[' | b']' | b'`' | b'^' | b'\\' | b';' | b'|'
            )
    })
}

fn is_control(byte: u8) -> bool {
    byte < 0x20 || byte == 0x7f
}

fn control_name(byte: u8) -> String {
    match byte {
        b'\n' => "\\n".to_string(),
        b'\t' => "\\t".to_string(),
        b'\r' => "\\r".to_string(),
        b'\x0C' => "\\x0C".to_string(),
        _ => format!("\\{:03o}", byte),
    }
}
