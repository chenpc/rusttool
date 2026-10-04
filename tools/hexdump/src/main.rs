//! `hexdump(1)` clone CLI: argument parsing, input handling, output.
//!
//! Option set and error wording follow util-linux hexdump.c `parse_args()`:
//! getopt string `bXcCde:f:L::n:os:vxhV`, with `-L` taking an optional
//! argument and long options for every display mode.

use std::io::{Read, Write};

use hexdump::{default_formats, display, parse_format, version_line, Format, FormatError, HELP};

/// util-linux `errtryhelp()`: message, then the "Try ... --help" hint.
fn errtryhelp(message: impl std::fmt::Display) -> i32 {
    eprintln!("hexdump: {}", message);
    eprintln!("Try 'hexdump --help' for more information.");
    1
}

/// Strip Rust's ` (os error N)` suffix so messages match C strerror output.
fn errno_str(err: &std::io::Error) -> String {
    let text = err.to_string();
    match text.find(" (os error ") {
        Some(idx) => text[..idx].to_string(),
        None => text,
    }
}

/// `strtosize_or_err()`: decimal/hex/octal with optional b/k/m/g multiplier.
fn strtosize(text: &str) -> Option<u64> {
    let text = text.strip_prefix('+').unwrap_or(text);
    let (digits, mult) = match text.as_bytes().last() {
        Some(b'b') | Some(b'B') => (&text[..text.len() - 1], 512u64),
        Some(b'k') | Some(b'K') => (&text[..text.len() - 1], 1024),
        Some(b'm') | Some(b'M') => (&text[..text.len() - 1], 1024 * 1024),
        Some(b'g') | Some(b'G') => (&text[..text.len() - 1], 1024 * 1024 * 1024),
        _ => (text, 1),
    };
    if digits.is_empty() {
        return None;
    }
    let value = if let Some(hex) = digits.strip_prefix("0x").or_else(|| digits.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()?
    } else if digits.len() > 1 && digits.starts_with('0') {
        u64::from_str_radix(digits, 8).ok()?
    } else {
        digits.parse::<u64>().ok()?
    };
    value.checked_mul(mult)
}

struct Config {
    formats: Vec<Format>,
    skip: u64,
    length: Option<u64>,
    squeeze: bool,
    files: Vec<String>,
}

/// A diagnostic plus whether it is an option-parsing error (getopt family,
/// which prints the program name and the `--help` hint) or a format parser
/// error (errx family: plain `hexdump: ...`).
struct Diagnostic {
    text: String,
    hint: bool,
}

impl Diagnostic {
    fn getopt(text: impl Into<String>) -> Diagnostic {
        Diagnostic { text: text.into(), hint: true }
    }
    fn plain(text: impl Into<String>) -> Diagnostic {
        Diagnostic { text: text.into(), hint: false }
    }
}

enum Action {
    Run(Config),
    Help,
    Version,
    Error(Diagnostic),
}

/// A `-f` format file: one format per line, blank lines and `#` comments
/// skipped, leading whitespace trimmed (util-linux `addfile()`).
fn read_format_file(path: &str) -> Result<Vec<Format>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("{}: {}", path, errno_str(&e)))?;
    let mut formats = Vec::new();
    for line in text.lines() {
        let spec = line.trim_start();
        if spec.is_empty() || spec.starts_with('#') {
            continue;
        }
        match parse_format(spec) {
            Ok(units) => formats.push(Format { units }),
            Err(e) => return Err(e.message),
        }
    }
    Ok(formats)
}

fn push_format(formats: &mut Vec<Format>, spec: &str) -> Result<(), String> {
    match parse_format(spec) {
        Ok(units) => {
            formats.push(Format { units });
            Ok(())
        }
        Err(FormatError { message, .. }) => Err(message),
    }
}

/// getopt-style parse: clustered short flags, `--long[=value]`, `--`.
fn parse_args(args: &[String]) -> Action {
    let mut formats: Vec<Format> = Vec::new();
    let mut skip = 0u64;
    let mut length: Option<u64> = None;
    let mut squeeze = true;
    let mut files: Vec<String> = Vec::new();
    let mut no_more_options = false;

    // Display modes are additive, exactly like upstream: every -b/-c/-C/-d/
    // -o/-x/-X appends its format(s), and the block size is the largest.
    let mut add_mode = |ch: char, formats: &mut Vec<Format>| {
        if let Some(mut fs) = hexdump::builtin_formats(ch) {
            formats.append(&mut fs);
        }
    };

    let mut it = args.iter().peekable();
    while let Some(arg) = it.next() {
        if no_more_options || arg == "-" || !arg.starts_with('-') {
            files.push(arg.clone());
            continue;
        }
        if arg == "--" {
            no_more_options = true;
            continue;
        }
        if let Some(long) = arg.strip_prefix("--") {
            let (name, value) = match long.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (long, None),
            };
            // Options taking a value, with getopt's `--opt=value` or
            // `--opt value` forms.
            let take_value = |it: &mut std::iter::Peekable<std::slice::Iter<'_, String>>| -> Option<String> {
                match value.clone() {
                    Some(v) => Some(v),
                    None => it.next().cloned(),
                }
            };
            match name {
                "one-byte-octal" => add_mode('b', &mut formats),
                "one-byte-char" => add_mode('c', &mut formats),
                "canonical" => add_mode('C', &mut formats),
                "two-bytes-decimal" => add_mode('d', &mut formats),
                "two-bytes-octal" => add_mode('o', &mut formats),
                "two-bytes-hex" => add_mode('x', &mut formats),
                "one-byte-hex" => add_mode('X', &mut formats),
                "no-squeezing" => squeeze = false,
                "color" => { /* accepted; colours are not rendered */ }
                "help" => return Action::Help,
                "version" => return Action::Version,
                "format" => match take_value(&mut it) {
                    Some(v) => {
                        if let Err(e) = push_format(&mut formats, &v) {
                            return Action::Error(Diagnostic::plain(e));
                        }
                    }
                    None => return Action::Error(Diagnostic::getopt(format!("option '--{}' requires an argument", name))),
                },
                "format-file" => match take_value(&mut it) {
                    Some(v) => match read_format_file(&v) {
                        Ok(mut fs) => formats.append(&mut fs),
                        Err(e) => return Action::Error(Diagnostic::plain(e)),
                    },
                    None => return Action::Error(Diagnostic::getopt(format!("option '--{}' requires an argument", name))),
                },
                "length" => match take_value(&mut it) {
                    Some(v) => match strtosize(&v) {
                        Some(n) => length = Some(n),
                        None => return Action::Error(Diagnostic::plain(format!("failed to parse length: '{}': Invalid argument", v))),
                    },
                    None => return Action::Error(Diagnostic::getopt(format!("option '--{}' requires an argument", name))),
                },
                "skip" => match take_value(&mut it) {
                    Some(v) => match strtosize(&v) {
                        Some(n) => skip = n,
                        None => return Action::Error(Diagnostic::plain(format!("failed to parse offset: '{}': Invalid argument", v))),
                    },
                    None => return Action::Error(Diagnostic::getopt(format!("option '--{}' requires an argument", name))),
                },
                _ => return Action::Error(Diagnostic::getopt(format!("unrecognized option '--{}'", name))),
            }
            continue;
        }

        // Short cluster, e.g. -Cv or -n5.
        let chars: Vec<char> = arg[1..].chars().collect();
        let mut idx = 0usize;
        while idx < chars.len() {
            let ch = chars[idx];
            idx += 1;
            match ch {
                'b' | 'c' | 'C' | 'd' | 'o' | 'x' | 'X' => add_mode(ch, &mut formats),
                'v' => squeeze = false,
                'h' => return Action::Help,
                'V' => return Action::Version,
                'L' => {
                    // Optional argument: `-L`, `-L<mode>` or `--color=<mode>`.
                    if idx < chars.len() {
                        idx = chars.len();
                    }
                }
                'e' | 'f' | 'n' | 's' => {
                    let value: String = if idx < chars.len() {
                        let v: String = chars[idx..].iter().collect();
                        idx = chars.len();
                        v
                    } else {
                        match it.next() {
                            Some(v) => v.clone(),
                            None => {
                                return Action::Error(Diagnostic::getopt(format!(
                                    "option requires an argument -- '{}'",
                                    ch
                                )))
                            }
                        }
                    };
                    match ch {
                        'e' => {
                            if let Err(e) = push_format(&mut formats, &value) {
                                return Action::Error(Diagnostic::plain(e));
                            }
                        }
                        'f' => match read_format_file(&value) {
                            Ok(mut fs) => formats.append(&mut fs),
                            Err(e) => return Action::Error(Diagnostic::plain(e)),
                        },
                        'n' => match strtosize(&value) {
                            Some(n) => length = Some(n),
                            None => return Action::Error(Diagnostic::plain(format!("failed to parse length: '{}': Invalid argument", value))),
                        },
                        's' => match strtosize(&value) {
                            Some(n) => skip = n,
                            None => return Action::Error(Diagnostic::plain(format!("failed to parse offset: '{}': Invalid argument", value))),
                        },
                        _ => unreachable!(),
                    }
                }
                _ => return Action::Error(Diagnostic::getopt(format!("invalid option -- '{}'", ch))),
            }
        }
    }

    if formats.is_empty() {
        formats = default_formats();
    }

    Action::Run(Config { formats, skip, length, squeeze, files })
}

/// Read the whole input (files in order, or stdin) and apply skip/length.
///
/// Returns the data plus the number of bytes actually skipped: `-s` past the
/// end of the input consumes what is there and the addresses keep counting
/// from there, which is why upstream still prints an end address for
/// `hexdump -C -s 1k small-file`.
type Skipped = u64;

fn read_input(
    files: &[String],
    skip: u64,
    length: Option<u64>,
) -> Result<(Vec<u8>, Skipped, Vec<String>), Vec<String>> {
    let mut data = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    let mut opened = 0usize;

    let sources: Vec<Option<&str>> = if files.is_empty() {
        vec![None]
    } else {
        files.iter().map(|f| if f == "-" { None } else { Some(f.as_str()) }).collect()
    };

    for source in sources {
        match source {
            None => {
                if let Err(e) = std::io::stdin().read_to_end(&mut data) {
                    failures.push(format!("<stdin>: {}", errno_str(&e)));
                } else {
                    opened += 1;
                }
            }
            Some(path) => match std::fs::File::open(path) {
                Ok(mut handle) => {
                    if let Err(e) = handle.read_to_end(&mut data) {
                        failures.push(format!("{}: {}", path, errno_str(&e)));
                    } else {
                        opened += 1;
                    }
                }
                Err(e) => failures.push(format!("{}: {}", path, errno_str(&e))),
            },
        }
    }

    if opened == 0 {
        return Err(failures);
    }

    let skipped = skip.min(data.len() as u64);
    let mut data = data[skipped as usize..].to_vec();
    if let Some(len) = length {
        data.truncate(len.min(data.len() as u64) as usize);
    }
    Ok((data, skipped, failures))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // getopt-style messages name the program as invoked.
    let program = std::env::args().next().unwrap_or_else(|| "hexdump".to_string());
    let code = match parse_args(&args) {
        Action::Help => {
            print!("{}", HELP);
            0
        }
        Action::Version => {
            print!("{}", version_line());
            0
        }
        Action::Error(message) => {
            // errtryhelp(): the option-parsing errors carry the hint, while
            // errx() diagnostics from the format parser do not.
            if message.hint {
                eprintln!("{}: {}", program, message.text);
                eprintln!("Try 'hexdump --help' for more information.");
            } else {
                eprintln!("hexdump: {}", message.text);
            }
            1
        }
        Action::Run(config) => {
            let (data, skipped, failures) = match read_input(&config.files, config.skip, config.length)
            {
                Ok(v) => v,
                Err(failures) => {
                    for message in &failures {
                        eprintln!("hexdump: {}", message);
                    }
                    eprintln!("hexdump: all input file arguments failed");
                    std::process::exit(1);
                }
            };
            // Unreadable files alongside readable ones are warnings only: the
            // data still gets dumped.
            for message in &failures {
                eprintln!("hexdump: {}", message);
            }
            let out = display(&data, &config.formats, config.squeeze, skipped);
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            let _ = lock.write_all(&out);
            let _ = lock.flush();
            if failures.is_empty() { 0 } else { 1 }
        }
    };
    std::process::exit(code);
}
