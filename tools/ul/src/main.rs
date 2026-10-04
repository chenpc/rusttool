//! `ul` — do underlining, like util-linux `ul(1)`.
//!
//! I/O half; the overstrike engine lives in the `ul` library crate.

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
    terminal: Option<String>,
    indicated: bool,
    files: Vec<String>,
}

fn usage_hint() -> &'static str {
    "try 'ul --help' for more information."
}

fn parse_args(args: &[String]) -> Result<Action, String> {
    let mut terminal: Option<String> = None;
    let mut indicated = false;
    let mut files: Vec<String> = Vec::new();
    let mut operands_only = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
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
            let (name, value) = match long.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (long, None),
            };
            match name {
                "terminal" => {
                    let v = match value {
                        Some(v) => v,
                        None => {
                            i += 1;
                            if i >= args.len() {
                                return Err("option '--terminal' requires an argument".to_string());
                            }
                            args[i].clone()
                        }
                    };
                    terminal = Some(v);
                }
                "indicated" => indicated = true,
                "help" => return Ok(Action::Help),
                "version" => return Ok(Action::Version),
                other => return Err(format!("unrecognized option '--{}'", other)),
            }
            i += 1;
            continue;
        }
        let chars: Vec<char> = arg.chars().skip(1).collect();
        let mut j = 0;
        while j < chars.len() {
            match chars[j] {
                't' | 'T' => {
                    let rest: String = chars[j + 1..].iter().collect();
                    let v = if rest.is_empty() {
                        i += 1;
                        if i >= args.len() {
                            return Err("option '-t' requires an argument".to_string());
                        }
                        args[i].clone()
                    } else {
                        rest
                    };
                    terminal = Some(v);
                    break;
                }
                'i' => indicated = true,
                'h' => return Ok(Action::Help),
                'V' => return Ok(Action::Version),
                other => return Err(format!("invalid option -- '{}'", other)),
            }
            j += 1;
        }
        i += 1;
    }
    Ok(Action::Run(Options { terminal, indicated, files }))
}

/// Resolve whether to use the plain (`dumb`) renderer.
///
/// Upstream consults terminfo and falls back to `dumb` with a warning; here
/// `dumb` is selected when the effective terminal name is `dumb`, `unknown`,
/// empty or unset (documented deviation: no terminfo database is consulted).
fn is_dumb(explicit: Option<&str>) -> bool {
    let name = match explicit {
        Some(n) => n.to_string(),
        None => std::env::var("TERM").unwrap_or_default(),
    };
    name.is_empty() || name == "dumb" || name == "unknown"
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
            print!("{}", ul::HELP);
            return 0;
        }
        Ok(Action::Version) => {
            print!("{}", ul::version_line());
            return 0;
        }
        Ok(Action::Run(o)) => o,
        Err(message) => {
            eprintln!("ul: {}", message);
            eprintln!("ul: {}", usage_hint());
            return 1;
        }
    };
    let dumb = is_dumb(options.terminal.as_deref());

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let mut render_and_emit = |text: &str| -> Result<(), String> {
        let rendered = ul::ul_filter(text, dumb, options.indicated)?;
        out.write_all(rendered.as_bytes())
            .map_err(|e| format!("stdout: {}", e))?;
        Ok(())
    };

    if options.files.is_empty() {
        let text = match read_input(None) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("ul: stdin: {}", e);
                return 1;
            }
        };
        if let Err(e) = render_and_emit(&text) {
            eprintln!("ul: {}", e);
            return 1;
        }
    } else {
        for path in &options.files {
            let text = match read_input(Some(path)) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("ul: cannot open {}: {}", path, e);
                    return 1;
                }
            };
            if let Err(e) = render_and_emit(&text) {
                eprintln!("ul: {}", e);
                return 1;
            }
        }
    }
    if let Err(e) = out.flush() {
        eprintln!("ul: stdout: {}", e);
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
    fn terminal_forms() {
        assert_eq!(opts(&["-t", "dumb"]).terminal.as_deref(), Some("dumb"));
        assert_eq!(opts(&["-tdumb"]).terminal.as_deref(), Some("dumb"));
        assert_eq!(opts(&["-T", "xterm"]).terminal.as_deref(), Some("xterm"));
        assert_eq!(opts(&["--terminal", "xterm"]).terminal.as_deref(), Some("xterm"));
        assert_eq!(opts(&["--terminal=xterm"]).terminal.as_deref(), Some("xterm"));
        assert!(opts(&["-i"]).indicated);
        assert!(opts(&["--indicated"]).indicated);
    }

    #[test]
    fn dumb_resolution() {
        assert!(is_dumb(Some("dumb")));
        assert!(is_dumb(Some("unknown")));
        assert!(is_dumb(Some("")));
        assert!(!is_dumb(Some("xterm")));
    }

    #[test]
    fn bad_options_rejected() {
        assert!(parse(&["-q"]).is_err());
        assert!(parse(&["--bogus"]).is_err());
    }
}
