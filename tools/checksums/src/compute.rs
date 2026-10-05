use std::io::{self, Read, Write};

use hashes::{to_hex, Algorithm, BsdSum, Crc32, SysvSum, Hasher};
use libc::c_int;

/// strerror, not Rust's "(os error 2)", because the message is what is compared.
pub fn strerror(code: i32) -> String {
    unsafe {
        let p = libc::strerror(code as c_int);
        if p.is_null() {
            return format!("os error {}", code);
        }
        String::from_utf8_lossy(std::ffi::CStr::from_ptr(p).to_bytes()).to_string()
    }
}

use crate::{prog, Options, Tool};

/// Read a whole file, or standard input for `-`.
pub fn read_all(path: &str) -> Result<Vec<u8>, String> {
    if path == "-" {
        let mut buf = Vec::new();
        if let Err(e) = io::stdin().read_to_end(&mut buf) {
            return Err(strerror(e.raw_os_error().unwrap_or(0)));
        }
        return Ok(buf);
    }
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => return Err(strerror(e.raw_os_error().unwrap_or(0))),
    };
    let mut buf = Vec::new();
    if let Err(e) = file.read_to_end(&mut buf) {
        return Err(strerror(e.raw_os_error().unwrap_or(0)));
    }
    Ok(buf)
}

/// The digest bytes as raw mode writes them.
fn raw_bytes(tool: &Tool, opts: &Options, bytes: &[u8]) -> Vec<u8> {
    if opts.algorithm.is_numeric() {
        let mut hasher = opts.algorithm.hasher();
        hasher.update(bytes);
        return hasher.finish_with_length(bytes.len() as u64);
    }
    opts.algorithm.digest_of(bytes)
}

/// BSD counts 1024-byte blocks, SYSV counts 512-byte ones. That is the whole
/// difference between `sum -r` and `sum -s` besides the sum itself.
fn blocks(bytes: usize, block: usize) -> usize {
    if block == 0 {
        return 0;
    }
    bytes.div_ceil(block)
}

/// The number a numeric algorithm prints, and the block count that follows it.
fn numeric(tool: &Tool, opts: &Options, bytes: &[u8]) -> (u64, usize) {
    match opts.algorithm {
        Algorithm::Bsd => {
            let mut h = BsdSum::default();
            h.update(bytes);
            (h.value() as u64, blocks(bytes.len(), 1024))
        }
        Algorithm::Sysv => {
            let mut h = SysvSum::default();
            h.update(bytes);
            (h.value() as u64, blocks(bytes.len(), 512))
        }
        _ => {
            let mut h = Crc32::default();
            h.update(bytes);
            let d = h.finish_with_length(bytes.len() as u64);
            let value = d.iter().fold(0u64, |acc, b| (acc << 8) | *b as u64);
            let _ = tool;
            (value, bytes.len())
        }
    }
}

/// One line of numeric output.
///
/// coreutils prints BSD as `"%05d %5s"` and the others as `"%d %s"`: the checksum is
/// zero-padded to five digits and the block count is right-aligned in five. The gap
/// that makes `sum` look like it uses five spaces is a consequence of those two
/// widths, not a separator, so a value or a count that fills its field closes it.
fn numeric_line(value: u64, count: usize, name: &str, bsd: bool) -> String {
    if bsd {
        return format!("{:05} {:>5} {}", value, count, name);
    }
    format!("{} {} {}", value, count, name)
}

/// One output line in the shape this tool prints.
pub fn format_line(tool: &Tool, opts: &Options, name: &str, bytes: &[u8]) -> String {
    if opts.algorithm.is_numeric() {
        let (value, count) = numeric(tool, opts, bytes);
        return numeric_line(value, count, name, opts.algorithm == Algorithm::Bsd);
    }

    let algorithm = algorithm_with_length(opts);
    let digest = algorithm.digest_of(bytes);
    let shown = if opts.raw {
        String::from_utf8_lossy(&digest).to_string()
    } else if opts.base64 {
        base64(&digest)
    } else {
        to_hex(&digest)
    };

    let escaped = needs_escape(name);
    let name = escape(name);
    let marker = if escaped { "\\" } else { "" };
    if opts.tag && !opts.untagged {
        let tag = match algorithm {
            Algorithm::Blake2b(bits) if bits != 512 => format!("BLAKE2b-{}", bits),
            other => other.tag().to_string(),
        };
        return format!("{}{} ({}) = {}", marker, tag, name, shown);
    }
    let mode = if opts.binary { "*" } else { " " };
    format!("{}{} {}{}", marker, shown, mode, name)
}

/// The escaping a name with a newline or a backslash gets: a leading backslash on
/// the line, and each special byte written as two.
pub fn needs_escape(name: &str) -> bool {
    name.bytes().any(|b| SPECIAL.contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bsd_pads_both_fields_to_five() {
        // What the system tool prints for a 1-byte-block file whose BSD sum is 3762.
        assert_eq!(numeric_line(3762, 1, "a.txt", true), "03762     1 a.txt");
        // A count that fills its field closes the gap.
        assert_eq!(numeric_line(0, 1024, "big.bin", true), "00000  1024 big.bin");
        assert_eq!(numeric_line(0, 104857600, "sparse.bin", true), "00000 104857600 sparse.bin");
    }

    #[test]
    fn other_numeric_algorithms_pad_nothing() {
        assert_eq!(numeric_line(1126, 1, "a.txt", false), "1126 1 a.txt");
        assert_eq!(numeric_line(2155303065, 200, "r1.bin", false), "2155303065 200 r1.bin");
    }
}

const SPECIAL: &[u8] = &[b'\\', b'\n', b'\r', b'\t', b'\x0C', b'\x0B', b'\x08'];

pub fn escape(name: &str) -> String {
    if !needs_escape(name) {
        return name.to_string();
    }
    let mut out = String::new();
    for b in name.bytes() {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            b'\x0C' => out.push_str("\\f"),
            b'\x0B' => out.push_str("\\v"),
            b'\x08' => out.push_str("\\b"),
            _ => out.push(char::from(b)),
        }
    }
    out
}

pub fn unescape(name: &str) -> String {
    let bytes = name.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'n' => { out.push(b'\n'); i += 2; continue; }
                b'r' => { out.push(b'\r'); i += 2; continue; }
                b't' => { out.push(b'\t'); i += 2; continue; }
                b'f' => { out.push(b'\x0C'); i += 2; continue; }
                b'v' => { out.push(b'\x0B'); i += 2; continue; }
                b'b' => { out.push(b'\x08'); i += 2; continue; }
                b'\\' => { out.push(b'\\'); i += 2; continue; }
                _ => {}
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// The algorithm as `-l` would change it. Only BLAKE2b has a variable width.
pub fn algorithm_with_length(opts: &Options) -> Algorithm {
    if matches!(opts.algorithm, Algorithm::Blake2b(_)) {
        Algorithm::Blake2b(opts.length)
    } else {
        opts.algorithm
    }
}

fn base64(bytes: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(CHARS[(n >> 18) as usize & 0x3f] as char);
        out.push(CHARS[(n >> 12) as usize & 0x3f] as char);
        if chunk.len() == 1 {
            out.push('=');
            out.push('=');
        } else {
            out.push(CHARS[(n >> 6) as usize & 0x3f] as char);
            if chunk.len() == 2 {
                out.push('=');
            }
        }
    }
    out
}

pub fn compute(tool: &Tool, opts: &Options) -> i32 {
    let out = io::stdout();
    let mut out = out.lock();
    let files = if opts.files.is_empty() { vec!["-".to_string()] } else { opts.files.clone() };
    let mut status = 0;
    for file in files {
        match read_all(&file) {
            Ok(bytes) => {
                if opts.raw {
                    // Raw mode prints the digest bytes and nothing else, not even
                    // a newline.
                    let _ = out.write_all(&raw_bytes(tool, opts, &bytes));
                    continue;
                }
                let line = format_line(tool, opts, &file, &bytes);
                if opts.zero {
                    let _ = out.write_all(line.as_bytes());
                    let _ = out.write_all(&[0]);
                } else {
                    let _ = out.write_all(line.as_bytes());
                    let _ = out.write_all(&[b'\n']);
                }
            }
            Err(message) => {
                eprintln!("{}: {}: {}", prog(tool), quote_name(&file), message);
                status = 1;
            }
        }
    }
    status
}

/// GNU quotes a name only when something in it needs quoting.
pub fn quote_name(name: &str) -> String {
    if name.is_empty() {
        return "''".to_string();
    }
    quoting::shell_quote_meta(name)
}
