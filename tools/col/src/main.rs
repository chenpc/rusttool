//! `col` — filter reverse line feeds from its input, like util-linux `col(1)`.
//!
//! This is the I/O half; the line/column machine lives in the `col` library
//! crate so it can be unit-tested on the host without touching stdin.
//!
//! Behaviour copied from upstream `text-utils/col.c`:
//!
//! * input is always standard input — `col` takes no file operands and
//!   reports `bad usage` for one,
//! * `-b`/`--no-backspaces` emits only the last character of a column instead
//!   of the overwritten character followed by rubouts,
//! * `-f`/`--fine` keeps half lines (`ESC`-`\t`) instead of rounding them up to
//!   whole lines,
//! * `-p`/`--pass` passes unknown control sequences through instead of
//!   dropping them,
//! * `-h`/`--tabs` (the default) compresses runs of blanks into tabs and
//!   `-x`/`--spaces` turns that off; the two are mutually exclusive,
//! * `-l`/`--lines` takes a line count,
//! * note that `-h` is *tabs* here: help is `-H`/`--help`,
//! * `-H`/`--help` and `-V`/`--version` print to stdout and exit 0,
//! * an unusable option reports on stderr and exits `EXIT_FAILURE`.

use std::io::{self, Read, Write};
use std::process::ExitCode;

use col::{version_line, Options, HELP};

/// Where a command line stops after `-H`/`-V` short-circuits the rest.
#[derive(Debug)]
enum Action {
    Help,
    Version,
    Run(Options),
}

fn usage_hint() -> String {
    "Try 'col --help' for more information.".to_string()
}

/// `strtou32_or_err()` for `-l`: decimal digits and nothing else.
fn parse_lines(text: &str) -> Result<usize, String> {
    if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("bad -l argument: '{}'", text));
    }
    text.parse::<usize>()
        .map_err(|_| format!("bad -l argument: '{}'", text))
}

/// `getopt_long`-style parsing with GNU permutation (options may follow
/// operands) and short-option clustering.
///
/// The short option string is upstream's `"bfhl:pxVH"`, which is why `-h` means
/// "tabs" and help is `-H`.
fn parse_args(args: &[String]) -> Result<Action, String> {
    let mut opts = Options::default();
    let mut saw_tabs = false;
    let mut saw_spaces = false;
    let mut operands = 0usize;
    let mut operands_only = false;
    let mut index = 0usize;

    while index < args.len() {
        let arg = args[index].clone();
        index += 1;
        if operands_only || !arg.starts_with('-') || arg == "-" {
            operands += 1;
            continue;
        }
        if arg == "--" {
            operands_only = true;
            continue;
        }

        // Long options are whole arguments, so a clustered "-bx" only happens
        // on the short side; `-l`/`--lines` argument may be attached or separate.
        if let Some(long) = arg.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value.to_string())),
                None => (long, None),
            };
            let mut take_value = |flag: char| -> Result<String, String> {
                if let Some(value) = inline.clone() {
                    return Ok(value);
                }
                if index < args.len() {
                    let value = args[index].clone();
                    index += 1;
                    return Ok(value);
                }
                Err(format!("option '--{}' requires an argument", name_of(flag)))
            };
            match name {
                "no-backspaces" => opts.no_backspaces = true,
                "fine" => opts.fine = true,
                "pass" => opts.pass_unknown = true,
                "tabs" => {
                    opts.compress_spaces = true;
                    saw_tabs = true;
                }
                "spaces" => {
                    opts.compress_spaces = false;
                    saw_spaces = true;
                }
                "lines" => opts.buffer_lines = parse_lines(&take_value('l')?)?,
                "help" => return Ok(Action::Help),
                "version" => return Ok(Action::Version),
                other => return Err(format!("unrecognized option '--{}'", other)),
            }
            continue;
        }

        let flags: Vec<char> = arg.chars().skip(1).collect();
        let mut at = 0usize;
        while at < flags.len() {
            let flag = flags[at];
            at += 1;
            match flag {
                'b' => opts.no_backspaces = true,
                'f' => opts.fine = true,
                'p' => opts.pass_unknown = true,
                'h' => {
                    opts.compress_spaces = true;
                    saw_tabs = true;
                }
                'x' => {
                    opts.compress_spaces = false;
                    saw_spaces = true;
                }
                'l' => {
                    // Attached ("-l32") or the next argv entry ("-l 32").
                    let value: String = if at < flags.len() {
                        let value = flags[at..].iter().collect::<String>();
                        at = flags.len();
                        value
                    } else if index < args.len() {
                        let value = args[index].clone();
                        index += 1;
                        value
                    } else {
                        return Err("option requires an argument -- 'l'".to_string());
                    };
                    opts.buffer_lines = parse_lines(&value)?;
                }
                'V' => return Ok(Action::Version),
                'H' => return Ok(Action::Help),
                other => return Err(format!("invalid option -- '{}'", other)),
            }
        }
    }

    if saw_tabs && saw_spaces {
        return Err("option '--tabs' is mutually exclusive with '--spaces'".to_string());
    }
    if operands > 0 {
        return Err("bad usage".to_string());
    }
    Ok(Action::Run(opts))
}

/// Long option name of a short flag, for the "requires an argument" message.
fn name_of(flag: char) -> &'static str {
    match flag {
        'l' => "lines",
        _ => "",
    }
}

fn real_main() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = match parse_args(&args) {
        Ok(Action::Help) => {
            print!("{}", HELP);
            return 0;
        }
        Ok(Action::Version) => {
            print!("{}", version_line());
            return 0;
        }
        Ok(Action::Run(opts)) => opts,
        Err(message) => {
            eprintln!("col: {}", message);
            eprintln!("{}", usage_hint());
            return 1;
        }
    };

    let mut input = String::new();
    if let Err(error) = io::stdin().read_to_string(&mut input) {
        eprintln!("col: stdin: {}", error);
        return 1;
    }

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if let Err(error) = out
        .write_all(col::filter(&input, &opts).as_bytes())
        .and_then(|()| out.flush())
    {
        eprintln!("col: stdout: {}", error);
        return 1;
    }
    0
}

fn main() -> ExitCode {
    // Same small-int exit codes as upstream's EXIT_SUCCESS/EXIT_FAILURE.
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

    fn run_opts(args: &[&str]) -> Options {
        match parse(args) {
            Ok(Action::Run(opts)) => opts,
            other => panic!("expected options for {:?}, got {:?}", args, other),
        }
    }

    #[test]
    fn no_options_means_the_upstream_defaults() {
        assert_eq!(run_opts(&[]), Options::default());
        // Upstream initialises compress_spaces to 1, i.e. spaces become tabs.
        assert!(run_opts(&[]).compress_spaces);
        assert!(!run_opts(&[]).fine);
        assert!(!run_opts(&[]).no_backspaces);
        assert!(!run_opts(&[]).pass_unknown);
    }

    #[test]
    fn the_documented_flags_map_to_their_meanings() {
        assert!(run_opts(&["-b"]).no_backspaces);
        assert!(run_opts(&["-f"]).fine);
        assert!(run_opts(&["-p"]).pass_unknown);
        assert!(run_opts(&["-x"]).compress_spaces == false);
        assert!(run_opts(&["-h"]).compress_spaces);
        assert_eq!(run_opts(&["-l", "5"]).buffer_lines, 5);
        assert_eq!(run_opts(&["-l7"]).buffer_lines, 7);
        assert_eq!(run_opts(&["--lines=9"]).buffer_lines, 9);
    }

    #[test]
    fn short_flags_cluster() {
        let opts = run_opts(&["-bfp"]);
        assert!(opts.no_backspaces && opts.fine && opts.pass_unknown);
        // First action wins and short-circuits, exactly like getopt. Note -h
        // is tabs here (help is -H), so both orders reach -V as Version.
        assert!(matches!(parse(&["-Vh"]), Ok(Action::Version)));
        assert!(matches!(parse(&["-hV"]), Ok(Action::Version)));
        assert_eq!(parse(&["-bq"]).unwrap_err(), "invalid option -- 'q'".to_string());
    }

    #[test]
    fn long_flags_match() {
        assert!(run_opts(&["--no-backspaces"]).no_backspaces);
        assert!(run_opts(&["--fine"]).fine);
        assert!(run_opts(&["--pass"]).pass_unknown);
        assert!(!run_opts(&["--spaces"]).compress_spaces);
        assert!(run_opts(&["--tabs"]).compress_spaces);
        assert!(matches!(parse(&["--help"]), Ok(Action::Help)));
        assert!(matches!(parse(&["--version"]), Ok(Action::Version)));
    }

    #[test]
    fn tabs_and_spaces_are_mutually_exclusive() {
        assert!(parse(&["-h", "-x"]).unwrap_err().contains("mutually exclusive"));
        assert!(parse(&["--tabs", "--spaces"]).is_err());
        // Either order is an error, matching upstream's
        // "mutually exclusive arguments: --tabs --spaces" (exit 1).
        assert!(parse(&["-x", "-h"]).is_err());
        assert!(parse(&["-h", "-x"]).is_err());
    }

    #[test]
    fn options_permute_around_operands_but_operands_are_rejected() {
        // Upstream's getopt permutes, so the option is seen first...
        match parse(&["-b", "junk"]) {
            Err(message) => assert_eq!(message, "bad usage"),
            other => panic!("expected an error, got {:?}", other),
        }
        // ...and then the leftover operand is reported.
        assert_eq!(parse(&["junk"]).unwrap_err(), "bad usage".to_string());
        assert_eq!(parse(&["-b", "--", "junk"]).unwrap_err(), "bad usage".to_string());
    }

    #[test]
    fn double_dash_ends_options() {
        assert_eq!(parse(&["--", "-b"]).unwrap_err(), "bad usage".to_string());
    }

    #[test]
    fn bad_options_are_rejected_with_getopt_style_messages() {
        assert_eq!(parse(&["-Q"]).unwrap_err(), "invalid option -- 'Q'".to_string());
        assert_eq!(
            parse(&["--bogus"]).unwrap_err(),
            "unrecognized option '--bogus'".to_string()
        );
        assert_eq!(
            parse(&["-l", "zz"]).unwrap_err(),
            "bad -l argument: 'zz'".to_string()
        );
        assert_eq!(
            parse(&["-l"]).unwrap_err(),
            "option requires an argument -- 'l'".to_string()
        );
        assert_eq!(
            parse(&["--lines"]).unwrap_err(),
            "option '--lines' requires an argument".to_string()
        );
    }

    #[test]
    fn parse_lines_rejects_non_numerals() {
        assert_eq!(parse_lines("0"), Ok(0));
        assert_eq!(parse_lines("32"), Ok(32));
        assert!(parse_lines("").is_err());
        assert!(parse_lines("3x").is_err());
        assert!(parse_lines("-1").is_err());
    }
}
