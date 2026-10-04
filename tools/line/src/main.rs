//! `line` — read exactly one line from stdin, like util-linux `line(1)`.
//!
//! Legacy utility kept for backward compatibility: it prints the first line
//! of stdin (plus a newline) and exits 0, or exits 1 on EOF.

use std::io::{self, Read, Write};
use std::process::ExitCode;

#[derive(Debug)]
enum Action {
    Help,
    Version,
    Run,
}

fn parse_args(args: &[String]) -> Result<Action, String> {
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            i += 1;
            continue;
        }
        if let Some(long) = arg.strip_prefix("--") {
            match long {
                "help" => return Ok(Action::Help),
                "version" => return Ok(Action::Version),
                other => return Err(format!("unrecognized option '--{}'", other)),
            }
            i += 1;
            continue;
        }
        if arg.starts_with('-') && arg != "-" {
            for flag in arg.chars().skip(1) {
                match flag {
                    'h' => return Ok(Action::Help),
                    'V' => return Ok(Action::Version),
                    other => return Err(format!("invalid option -- '{}'", other)),
                }
            }
            i += 1;
            continue;
        }
        // Upstream ignores non-option operands; keep accepting them.
        i += 1;
    }
    Ok(Action::Run)
}

fn real_main() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_args(&args) {
        Ok(Action::Help) => {
            print!("{}", line::HELP);
            return 0;
        }
        Ok(Action::Version) => {
            print!("{}", line::version_line());
            return 0;
        }
        Ok(Action::Run) => {}
        Err(message) => {
            eprintln!("line: {}", message);
            eprintln!("line: try 'line --help' for more information.");
            return 1;
        }
    }

    let mut input = Vec::new();
    if let Err(e) = io::stdin().lock().read_to_end(&mut input) {
        eprintln!("line: stdin: {}", e);
        return 1;
    }
    let (out, ok) = line::run_line(&input);
    let stdout = io::stdout();
    if let Err(e) = stdout.lock().write_all(&out).and_then(|()| stdout.lock().flush()) {
        let _ = e;
        eprintln!("line: stdout: broken pipe");
        return 1;
    }
    if ok { 0 } else { 1 }
}

fn main() -> ExitCode {
    match real_main() {
        0 => ExitCode::SUCCESS,
        _ => ExitCode::from(1),
    }
}
