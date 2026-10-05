use std::io::{self, Read, Write};

use hashes::Algorithm;

use crate::{prog, try_help, Options, Tool};
use crate::compute::{read_all, unescape};
use crate::compute::quote_name;

/// A parsed checksum line: the digest bytes and the name they belong to.
struct Entry {
    digest: Vec<u8>,
    name: String,
}

/// Which shape a line is, and whether it is one this tool can check.
///
/// `hex name` and `TAG (name) = hex` are both accepted. The tag has to name the
/// algorithm in effect, and the hex has to be exactly as long as that algorithm's
/// digest - which is why a 31-character md5 line is "improperly formatted" rather
/// than a mismatch.
fn parse_line(tool: &Tool, opts: &Options, line: &str) -> Option<Entry> {
    if line.starts_with('\\') {
        // The whole line was escaped, so the name has to be unescaped back.
        let entry = parse_line_inner(tool, opts, &line[1..])?;
        return Some(Entry { digest: entry.digest, name: unescape(&entry.name) });
    }
    parse_line_inner(tool, opts, line)
}

fn parse_line_inner(tool: &Tool, opts: &Options, line: &str) -> Option<Entry> {
    if line.is_empty() {
        return None;
    }
    let bytes = line.as_bytes();

    // TAG (name) = hex
    if let Some(open) = bytes.iter().position(|b| *b == b'(') {
        let tag = line[..open].trim();
        if tag != opts.algorithm.tag() && !tag.starts_with(opts.algorithm.tag()) {
            return None;
        }
        let close = bytes.iter().position(|b| *b == b')')?;
        let eq = bytes[close..].iter().position(|b| *b == b'=')?;
        let name = line[open + 1..close].to_string();
        let hex = line[close + 1 + eq + 1..].trim();
        if name.is_empty() || hex.is_empty() {
            return None;
        }
        let digest = hex_to_bytes(hex)?;
        if digest.len() != opts.algorithm.bits() / 8 {
            return None;
        }
        return Some(Entry { digest, name });
    }

    // hex [mode]name, where the separator is exactly one space
    let space = bytes.iter().position(|b| *b == b' ')?;
    let hex = &line[..space];
    let rest = &line[space + 1..];
    if hex.is_empty() || rest.is_empty() {
        return None;
    }
    let digest = hex_to_bytes(hex)?;
    // BLAKE2b has a variable width, so a line for it can be any length.
    if digest.len() != opts.algorithm.bits() / 8 && !matches!(opts.algorithm, Algorithm::Blake2b(_)) {
        return None;
    }
    // The character after the separator is the mode marker: a space for text, an
    // asterisk for binary. It is not part of the name, unless it is the whole
    // remainder of the line.
    let name = if rest.starts_with('*') || rest.starts_with(' ') {
        let trimmed = rest[1..].to_string();
        if trimmed.is_empty() { rest.to_string() } else { trimmed }
    } else {
        rest.to_string()
    };
    Some(Entry { digest, name })
}

fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = (bytes[i] as char).to_digit(16)?;
        let lo = (bytes[i + 1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
        i += 2;
    }
    Some(out)
}

pub fn check(tool: &Tool, opts: &Options) -> i32 {
    let out = io::stdout();
    let mut out = out.lock();
    let files = if opts.files.is_empty() { vec!["-".to_string()] } else { opts.files.clone() };

    let mut improperly = 0usize;
    let mut mismatch = 0usize;
    let mut unreadable = 0usize;
    let mut verified = 0usize;
    let mut empty_files = 0usize;
    let mut any_file_had_lines = false;

    for file in &files {
        let content = match read_lines(file) {
            Ok(c) => c,
            Err(message) => {
                if opts.ignore_missing {
                    continue;
                }
                eprintln!("{}: {}: {}", prog(tool), quote_name(file), message);
                unreadable += 1;
                continue;
            }
        };

        let mut parsed_here = 0usize;
        let mut improperly_here = 0usize;
        for (number, line) in content.iter().enumerate() {
            if line.is_empty() {
                continue;
            }
            match parse_line(tool, opts, line) {
                Some(entry) => {
                    parsed_here += 1;
                    if opts.status || opts.quiet {
                        // Nothing is printed, but the file still has to be read.
                        match read_all(&entry.name) {
                            Ok(bytes) => {
                                if opts.algorithm.digest_of(&bytes) != entry.digest {
                                    mismatch += 1;
                                }
                            }
                            Ok(bytes) => {
                                verified += 1;
                                if opts.algorithm.digest_of(&bytes) != entry.digest {
                                    mismatch += 1;
                                }
                            }
                            Err(_) => {
                                if opts.ignore_missing {
                                    continue;
                                }
                                unreadable += 1;
                            }
                        }
                        continue;
                    }
                    match read_all(&entry.name) {
                        Ok(bytes) => {
                            if opts.algorithm.digest_of(&bytes) == entry.digest {
                                let _ = writeln!(out, "{}: OK", entry.name);
                            } else {
                                let _ = writeln!(out, "{}: FAILED", entry.name);
                                mismatch += 1;
                            }
                        }
                        Err(message) => {
                            if opts.ignore_missing {
                                continue;
                            }
                            eprintln!("{}: {}: {}", prog(tool), quote_name(&entry.name), message);
                            let _ = writeln!(out, "{}: FAILED open or read", entry.name);
                            unreadable += 1;
                        }
                    }
                }
                None => {
                    improperly_here += 1;
                    if opts.warn {
                        eprintln!(
                            "{}: {}: {}: improperly formatted {} checksum line",
                            prog(tool),
                            quote_name(file),
                            number + 1,
                            opts.algorithm.tag()
                        );
                    }
                }
            }
        }

        if parsed_here == 0 {
            // A file with nothing usable in it is reported on its own, and its
            // bad lines do not join the summary at the end.
            eprintln!("{}: {}: no properly formatted checksum lines found", prog(tool), quote_name(file));
            empty_files += 1;
        } else {
            improperly += improperly_here;
            any_file_had_lines = true;
        }
    }

    if !opts.status && improperly > 0 {
        if improperly > 0 {
            eprintln!(
                "{}: WARNING: {} {} improperly formatted",
                prog(tool),
                improperly,
                if improperly == 1 { "line is" } else { "lines are" }
            );
        }
        if mismatch > 0 {
            eprintln!(
                "{}: WARNING: {} computed {} did NOT match",
                prog(tool),
                mismatch,
                if mismatch == 1 { "checksum" } else { "checksums" }
            );
        }
        if unreadable > 0 {
            eprintln!(
                "{}: WARNING: {} listed {} could not be read",
                prog(tool),
                unreadable,
                if unreadable == 1 { "file" } else { "files" }
            );
        }
    }
    if opts.ignore_missing && verified == 0 && !opts.status {
        for file in &files {
            eprintln!("{}: {}: no file was verified", prog(tool), quote_name(file));
        }
    }

    if improperly > 0 && opts.strict {
        return 1;
    }
    if mismatch > 0 || unreadable > 0 {
        return 1;
    }
    if empty_files > 0 {
        return 1;
    }
    if opts.ignore_missing && verified == 0 {
        return 1;
    }
    0
}

fn read_lines(path: &str) -> Result<Vec<String>, String> {
    let bytes = crate::compute::read_all(path)?;
    let text = String::from_utf8_lossy(&bytes).to_string();
    Ok(text.split('\n').map(|s| s.trim_end_matches('\r').to_string()).collect())
}
