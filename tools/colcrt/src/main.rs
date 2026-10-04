//! `colcrt` — filter nroff output for CRT previewing, like util-linux `colcrt(1)`.
//!
//! I/O half; the filter itself lives in the `colcrt` library crate.

use std::fs;
use std::io::{self, Write};
use std::process::ExitCode;

#[derive(Debug)]
enum Action {
    Help,
    Version,
    Run(Options),
}

#[derive(Debug, Clone)]
struct Options {
    no_underlining: bool,
    half_lines: bool,
    files: Vec<String>,
}

fn usage_hint() -> &'static str {
    "try 'colcrt --help' for more information."
}

/// Parse argv: a lone `-` anywhere means `--no-underlining` (upstream
/// pre-scan), then getopt_long over `-2Vh` plus the two long flags.
fn parse_args(args: &[String]) -> Result<Action, String> {
    let mut no_underlining = false;
    let mut rest: Vec<String> = Vec::with_capacity(args.len());
    for a in args {
        if a == "-" {
            no_underlining = true;
        } else {
            rest.push(a.clone());
        }
    }
    let mut half_lines = false;
    let mut files: Vec<String> = Vec::new();
    let mut operands_only = false;
    let mut i = 0;
    while i < rest.len() {
        let arg = &rest[i];
        if operands_only || !arg.starts_with('-') || arg == "-" {
            files.push(arg.clone());
            i += 1;
            continue;
        }
        if arg == "--" {
            operands_only = true;
            i += 1;
            continue;
        }
        if let Some(long) = arg.strip_prefix("--") {
            match long {
                "no-underlining" => no_underlining = true,
                "half-lines" => half_lines = true,
                "help" => return Ok(Action::Help),
                "version" => return Ok(Action::Version),
                other => return Err(format!("unrecognized option '--{}'", other)),
            }
            i += 1;
            continue;
        }
        for flag in arg.chars().skip(1) {
            match flag {
                '2' => half_lines = true,
                'V' => return Ok(Action::Version),
                'h' => return Ok(Action::Help),
                other => return Err(format!("invalid option -- '{}'", other)),
            }
        }
        i += 1;
    }
    Ok(Action::Run(Options { no_underlining, half_lines, files }))
}

fn read_input(path: Option<&str>) -> io::Result<String> {
    let bytes = match path {
        Some(p) => fs::read(p)?,
        None => {
            let mut buf = Vec::new();
            use std::io::Read;
            io::stdin().lock().read_to_end(&mut buf)?;
            buf
        }
    };
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn real_main() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse_args(&args) {
        Ok(Action::Help) => {
            print!("{}", colcrt::HELP);
            return 0;
        }
        Ok(Action::Version) => {
            print!("{}", colcrt::version_line());
            return 0;
        }
        Ok(Action::Run(o)) => o,
        Err(message) => {
            eprintln!("colcrt: {}", message);
            eprintln!("colcrt: {}", usage_hint());
            return 1;
        }
    };

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    // Upstream processes each file with a fresh line state but keeps the
    // half-lines leading newline per file; filtering file-by-file matches.
    if options.files.is_empty() {
        let text = match read_input(None) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("colcrt: stdin: {}", e);
                return 1;
            }
        };
        let rendered = colcrt::colcrt_filter(&text, options.no_underlining, options.half_lines);
        if let Err(e) = out.write_all(rendered.as_bytes()).and_then(|()| out.flush()) {
            eprintln!("colcrt: stdout: {}", e);
            return 1;
        }
        return 0;
    }
    for path in &options.files {
        let text = match read_input(Some(path)) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("colcrt: cannot open {}: {}", path, e);
                return 1;
            }
        };
        let rendered = colcrt::colcrt_filter(&text, options.no_underlining, options.half_lines);
        if let Err(e) = out.write_all(rendered.as_bytes()) {
            eprintln!("colcrt: stdout: {}", e);
            return 1;
        }
    }
    if let Err(e) = out.flush() {
        eprintln!("colcrt: stdout: {}", e);
        return 1;
    }
    0
}

fn main() -> ExitCode {
    match real_main() {
        0 => ExitCode::SUCCESS,
        other => ExitCode::from(other.clamp(1, 255) as u8),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Action, String> {
        parse_args(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>())
    }

    fn opts(args: &[&str]) -> Options {
        match parse(args) {
            Ok(Action::Run(o)) => o,
            other => panic!("expected run for {:?}, got {:?}", args, other),
        }
    }

    #[test]
    fn lone_dash_means_no_underlining() {
        let o = opts(&["-"]);
        assert!(o.no_underlining);
        assert!(o.files.is_empty());
        let o = opts(&["-", "a.txt"]);
        assert!(o.no_underlining);
        assert_eq!(o.files, vec!["a.txt"]);
    }

    #[test]
    fn long_and_short_flags() {
        assert!(opts(&["--no-underlining"]).no_underlining);
        assert!(opts(&["-2"]).half_lines);
        assert!(opts(&["--half-lines"]).half_lines);
        assert!(matches!(parse(&["--help"]), Ok(Action::Help)));
        assert!(matches!(parse(&["-V"]), Ok(Action::Version)));
    }

    #[test]
    fn double_dash_ends_options() {
        let o = opts(&["--", "-2"]);
        assert!(!o.half_lines);
        assert_eq!(o.files, vec!["-2"]);
    }

    #[test]
    fn bad_options_rejected() {
        assert!(parse(&["-q"]).is_err());
        assert!(parse(&["--bogus"]).is_err());
    }
}
