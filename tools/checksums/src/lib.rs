//! The `*sum` family: `cksum`, `sum`, `md5sum`, `sha1sum`, `sha224sum`,
//! `sha256sum`, `sha384sum`, `sha512sum`, `b2sum`.
//!
//! Nine commands, one shape. They differ only in which digest they default to and
//! which options they accept, so the shape lives here once and each crate is a
//! `Tool` description plus a call to `run`.
//!
//! Everything below was read off the system binary rather than off the man page:
//! the exact wording of the diagnostics, which option combinations are rejected,
//! how a checksum line is parsed, and where BSD puts five spaces where SYSV puts
//! one.

pub mod check;
pub mod compute;
pub mod tools;

use hashes::Algorithm;
use quoting::quote_for;

/// Which options a given tool accepts. `sum` takes three, `cksum` takes sixteen,
/// and handing `sum` a `--check` would make it accept something the real one
/// rejects.
#[derive(Clone, Copy)]
pub struct Accepts {
    pub check: bool,
    pub tag: bool,
    pub untagged: bool,
    pub base64: bool,
    pub raw: bool,
    pub zero: bool,
    pub length: bool,
    pub binary: bool,
    pub text: bool,
    pub algorithm: bool,
    pub check_only: bool,
    pub debug: bool,
}

/// One tool's description.
pub struct Tool {
    pub name: &'static str,
    pub default_algorithm: Algorithm,
    /// `cksum` prints `CRC (file) = ...` unless told otherwise; the hash tools
    /// print the digest first and only tag it when asked.
    pub default_tag: bool,
    pub accepts: Accepts,
    pub short_opts: &'static str,
    pub long_opts: &'static [&'static str],
}

struct Options {
    algorithm: Algorithm,
    algorithm_given: bool,
    check: bool,
    tag: bool,
    untagged: bool,
    base64: bool,
    raw: bool,
    zero: bool,
    quiet: bool,
    status: bool,
    strict: bool,
    warn: bool,
    ignore_missing: bool,
    binary: bool,
    length: usize,
    debug: bool,
    files: Vec<String>,
}

const CHECK_ONLY: [&str; 5] = [
    "--ignore-missing", "--quiet", "--status", "--strict", "--warn",
];

pub fn prog(tool: &Tool) -> String {
    tool.name.to_string()
}

pub fn try_help(tool: &Tool) {
    eprintln!("Try '{} --help' for more information.", prog(tool));
}

/// The long options, in the order the tool lists them, which is the order an
/// ambiguity message prints them in.
fn long_names(tool: &Tool) -> Vec<&'static str> {
    tool.long_opts.iter().map(|s| *s).collect()
}

/// Parse argv. Returns None when the tool has already diagnosed and decided to
/// exit, with the exit status in the second element.
fn parse(tool: &Tool, argv: &[String]) -> (Option<Options>, i32) {
    let mut opts = Options {
        algorithm: tool.default_algorithm,
        algorithm_given: false,
        check: false,
        tag: tool.default_tag,
        untagged: false,
        base64: false,
        raw: false,
        zero: false,
        quiet: false,
        status: false,
        strict: false,
        warn: false,
        ignore_missing: false,
        binary: false,
        length: 512,
        debug: false,
        files: Vec::new(),
    };

    // POSIXLY_CORRECT stops option parsing at the first non-option, so a later
    // --check becomes a file name. Without it getopt_long permutes, which is the
    // behaviour the differential has to reproduce.
    let posixly = std::env::var("POSIXLY_CORRECT").is_ok();
    let mut i = 0;
    let mut no_more_options = false;

    while i < argv.len() {
        let arg = &argv[i];
        if no_more_options || arg.is_empty() {
            opts.files.push(arg.clone());
            i += 1;
            continue;
        }
        if arg == "--" {
            no_more_options = true;
            i += 1;
            continue;
        }
        if posixly && !opts.files.is_empty() {
            opts.files.push(arg.clone());
            i += 1;
            continue;
        }

        if arg.starts_with("--") {
            let name = match arg.find('=') {
                Some(pos) => &arg[..pos],
                None => arg,
            };
            let value = match arg.find('=') {
                Some(pos) => Some(arg[pos + 1..].to_string()),
                None => None,
            };
            let matched = match_long(tool, name);
            match matched {
                Ok(found) => {
                    if takes_argument(found.as_str()) {
                        let value = match value {
                            Some(v) => v,
                            None => {
                                i += 1;
                                if i >= argv.len() {
                                    eprintln!(
                                        "{}: option '{}' requires an argument",
                                        prog(tool),
                                        found
                                    );
                                    try_help(tool);
                                    return (None, 1);
                                }
                                argv[i].clone()
                            }
                        };
                        if !apply_long(tool, &mut opts, found.as_str(), &value) {
                            return (None, 1);
                        }
                    } else {
                        apply_long(tool, &mut opts, found.as_str(), "");
                    }
                }
                Err(candidates) => {
                    if candidates.len() == 1 {
                        eprintln!("{}: unrecognized option '{}'", prog(tool), arg);
                    } else {
                        let list: Vec<String> = candidates
                            .iter()
                            .map(|c| format!("'{}'", c))
                            .collect();
                        eprintln!(
                            "{}: option '{}' is ambiguous; possibilities: {}",
                            prog(tool),
                            arg,
                            list.join(" ")
                        );
                    }
                    try_help(tool);
                    return (None, 1);
                }
            }
            i += 1;
            continue;
        }

        if arg.starts_with('-') {
            let chars: Vec<char> = arg[1..].chars().collect();
            let mut ci = 0;
            while ci < chars.len() {
                let c = chars[ci];
                if !tool.short_opts.contains(c) {
                    eprintln!("{}: invalid option -- '{}'", prog(tool), c);
                    try_help(tool);
                    return (None, 1);
                }
                let takes = short_takes_argument(tool, c);
                if takes {
                    let value = if ci + 1 < chars.len() {
                        chars[ci + 1..].iter().collect()
                    } else {
                        i += 1;
                        if i >= argv.len() {
                            eprintln!("{}: option requires an argument -- '{}'", prog(tool), c);
                            try_help(tool);
                            return (None, 1);
                        }
                        argv[i].clone()
                    };
                    if !apply_short(tool, &mut opts, c, &value) {
                        return (None, 1);
                    }
                    break;
                }
                apply_short(tool, &mut opts, c, "");
                ci += 1;
            }
            i += 1;
            continue;
        }

        opts.files.push(arg.clone());
        i += 1;
    }

    (Some(opts), 0)
}

fn takes_argument(long: &str) -> bool {
    long == "--algorithm" || long == "--length"
}

fn short_takes_argument(tool: &Tool, c: char) -> bool {
    match c {
        'a' => tool.accepts.algorithm,
        'l' => tool.accepts.length,
        _ => false,
    }
}

/// getopt_long's prefix matching, including the ambiguity report.
fn match_long(tool: &Tool, name: &str) -> Result<String, Vec<String>> {
    let names = long_names(tool);
    if names.iter().any(|n| *n == name) {
        return Ok(name.to_string());
    }
    let hits: Vec<String> = names.iter().filter(|n| n.starts_with(name)).map(|n| n.to_string()).collect();
    match hits.len() {
        1 => Ok(hits[0].clone()),
        0 => Err(vec![name.to_string()]),
        _ => Err(hits),
    }
}

fn apply_long(tool: &Tool, opts: &mut Options, name: &str, value: &str) -> bool {
    match name {
        "--algorithm" => {
            match Algorithm::from_name(value) {
                Some(a) => {
                    opts.algorithm = a;
                    opts.algorithm_given = true;
                }
                None => {
                    eprintln!(
                        "{}: invalid argument {} for '{}'",
                        prog(tool),
                        quote_for(value, true),
                        "--algorithm"
                    );
                    eprintln!("Valid arguments are:");
                    for n in hashes::ALGORITHM_NAMES {
                        eprintln!("  - {}", quote_for(n, true));
                    }
                    try_help(tool);
                    return false;
                }
            }
        }
        "--length" => {
            let Ok(bits) = value.parse::<usize>() else {
                eprintln!("{}: invalid length: {}", prog(tool), quote_for(value, true));
                return false;
            };
            if bits % 8 != 0 {
                eprintln!("{}: invalid length: {}", prog(tool), quote_for(value, true));
                eprintln!("{}: length is not a multiple of 8", prog(tool));
                return false;
            }
            if bits > 512 {
                eprintln!("{}: invalid length: {}", prog(tool), quote_for(value, true));
                eprintln!(
                    "{}: maximum digest length for {} is 512 bits",
                    prog(tool),
                    quote_for("BLAKE2b", true)
                );
                return false;
            }
            opts.length = bits;
        }
        "--base64" => opts.base64 = true,
        "--raw" => opts.raw = true,
        "--check" => opts.check = true,
        "--tag" => opts.tag = true,
        "--untagged" => opts.untagged = true,
        "--zero" => opts.zero = true,
        "--ignore-missing" => opts.ignore_missing = true,
        "--quiet" => opts.quiet = true,
        "--status" => opts.status = true,
        "--strict" => opts.strict = true,
        "--warn" => opts.warn = true,
        "--binary" => opts.binary = true,
        "--text" => opts.binary = false,
        "--debug" => opts.debug = true,
        "--sysv" => opts.algorithm = Algorithm::Sysv,
        _ => {}
    }
    true
}

fn apply_short(tool: &Tool, opts: &mut Options, c: char, value: &str) -> bool {
    match c {
        'a' => return apply_long(tool, opts, "--algorithm", value),
        'l' => return apply_long(tool, opts, "--length", value),
        'b' => opts.binary = true,
        't' => opts.binary = false,
        'c' => opts.check = true,
        'w' => opts.warn = true,
        'z' => opts.zero = true,
        'r' => opts.algorithm = Algorithm::Bsd,
        's' => opts.algorithm = Algorithm::Sysv,
        _ => {}
    }
    true
}

/// The option combinations the tool refuses, in the order it refuses them.
fn validate(tool: &Tool, opts: &Options) -> bool {
    if opts.check {
        for name in ["--tag", "--base64", "--raw", "--zero"] {
            let set = match name {
                "--tag" => opts.tag && !tool.default_tag,
                "--base64" => opts.base64,
                "--raw" => opts.raw,
                _ => opts.zero,
            };
            if set {
                eprintln!(
                    "{}: the {} option is {} when verifying checksums",
                    prog(tool),
                    name,
                    if name == "--zero" { "not supported" } else { "meaningless" }
                );
                try_help(tool);
                return false;
            }
        }
        if opts.algorithm_given && opts.algorithm.is_numeric() {
            eprintln!(
                "{}: --check is not supported with --algorithm={{bsd,sysv,crc}}",
                prog(tool)
            );
            return false;
        }
        return true;
    }

    for name in CHECK_ONLY {
        let set = match name {
            "--ignore-missing" => opts.ignore_missing,
            "--quiet" => opts.quiet,
            "--status" => opts.status,
            "--strict" => opts.strict,
            _ => opts.warn,
        };
        if set {
            eprintln!(
                "{}: the {} option is meaningful only when verifying checksums",
                prog(tool),
                name
            );
            try_help(tool);
            return false;
        }
    }
    true
}

pub fn run(tool: &Tool, argv: &[String]) -> i32 {
    // GNU's fancy quotes are on unless POSIXLY_CORRECT says otherwise.
    if argv.iter().any(|a| a == "--help") {
        print!("{}", help(tool));
        return 0;
    }
    if argv.iter().any(|a| a == "--version") {
        println!("{} (rusttool) 1.0", tool.name);
        return 0;
    }
    let (parsed, status) = parse(tool, argv);
    let Some(opts) = parsed else { return status };
    if !validate(tool, &opts) {
        return 1;
    }
    if opts.check {
        return check::check(tool, &opts);
    }
    compute::compute(tool, &opts)
}

fn help(tool: &Tool) -> String {
    format!("Usage: {} [OPTION]... [FILE]...\n", tool.name)
}
