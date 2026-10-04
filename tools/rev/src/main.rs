//! `rev` — reverse the characters of every line, like util-linux `rev(1)`.
//!
//! This is the I/O half; the reversal itself lives in the `rev` library crate
//! so it can be unit-tested on the host without touching files.
//!
//! Behaviour copied from upstream `text-utils/rev.c`:
//!
//! * with no file operand stdin is read, otherwise each file is processed in
//!   order and the output is concatenated,
//! * a file that cannot be opened is reported on stderr and processing
//!   continues with the next one; the exit status is then `EXIT_FAILURE`,
//! * `-0`/`--zero` uses NUL instead of newline as the line separator,
//! * `-h`/`--help` and `-V`/`--version` print to stdout and exit 0,
//! * an unusable option reports on stderr and exits `EXIT_FAILURE`.

use std::fs;
use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use rev::{reverse_chars, Separator};

/// Where a command line stops after `-h`/`-V` short-circuits the rest.
#[derive(Debug)]
enum Action {
    Help,
    Version,
    Run(Options),
}

#[derive(Debug, PartialEq, Eq)]
struct Options {
    sep: Separator,
    files: Vec<String>,
}

fn usage_hint() -> String {
    "try 'rev --help' for more information.".to_string()
}

/// `getopt_long`-style parsing with GNU permutation (options may follow file
/// operands), short option clustering and a `--` terminator. Upstream also
/// accepts unambiguous long-option abbreviations; we require the full name.
fn parse_args(args: &[String]) -> Result<Action, String> {
    let mut sep = Separator::Newline;
    let mut files: Vec<String> = Vec::new();
    let mut operands_only = false;

    for arg in args {
        // Upstream has no special meaning for a lone "-": fopen() gets it and
        // fails, so treat it as a (missing) file name too.
        if operands_only || !arg.starts_with('-') || arg == "-" {
            files.push(arg.clone());
            continue;
        }
        if arg == "--" {
            operands_only = true;
            continue;
        }
        if let Some(long) = arg.strip_prefix("--") {
            match long {
                "zero" => sep = Separator::Zero,
                "help" => return Ok(Action::Help),
                "version" => return Ok(Action::Version),
                other => return Err(format!("unrecognized option '--{}'", other)),
            }
            continue;
        }
        for flag in arg.chars().skip(1) {
            match flag {
                '0' => sep = Separator::Zero,
                'h' => return Ok(Action::Help),
                'V' => return Ok(Action::Version),
                other => return Err(format!("invalid option -- '{}'", other)),
            }
        }
    }

    Ok(Action::Run(Options { sep, files }))
}

/// Copy `input` to `out` with every line reversed.
///
/// `read_until` matches upstream's `read_line(sep, ...)`: the separator is
/// included in the buffer, and a zero-length read means end of input (which
/// upstream skips, so it emits nothing).
fn reverse_stream<R: BufRead, W: Write>(
    input: &mut R,
    out: &mut W,
    sep: Separator,
) -> io::Result<()> {
    let sep_byte = sep.byte();
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = input.read_until(sep_byte, &mut line)?;
        if read == 0 {
            return Ok(());
        }
        let has_sep = line[read - 1] == sep_byte;
        let body = if has_sep { read - 1 } else { read };
        out.write_all(&reverse_chars(&line[..body]))?;
        if has_sep {
            out.write_all(&line[body..])?;
        }
    }
}

fn real_main() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse_args(&args) {
        Ok(Action::Help) => {
            print!("{}", rev::HELP);
            return 0;
        }
        Ok(Action::Version) => {
            print!("{}", rev::version_line());
            return 0;
        }
        Ok(Action::Run(options)) => options,
        Err(message) => {
            eprintln!("rev: {}", message);
            eprintln!("rev: {}", usage_hint());
            return 1;
        }
    };

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let mut status = 0;

    if options.files.is_empty() {
        let stdin = io::stdin();
        let mut input = stdin.lock();
        if let Err(error) = reverse_stream(&mut input, &mut out, options.sep) {
            eprintln!("rev: stdin: {}", error);
            status = 1;
        }
    } else {
        for path in &options.files {
            match fs::File::open(path) {
                Ok(handle) => {
                    let mut input = io::BufReader::new(handle);
                    if let Err(error) = reverse_stream(&mut input, &mut out, options.sep) {
                        eprintln!("rev: {}: {}", path, error);
                        status = 1;
                    }
                }
                Err(error) => {
                    // Upstream warns and keeps going with the next operand.
                    eprintln!("rev: cannot open {}: {}", path, error);
                    status = 1;
                }
            }
        }
    }

    if let Err(error) = out.flush() {
        eprintln!("rev: stdout: {}", error);
        status = 1;
    }
    status
}

fn main() -> ExitCode {
    // Exit codes are small ints here (0 ok, 1 failure), same as upstream's
    // EXIT_SUCCESS/EXIT_FAILURE; std::io::Error's 1..=255 mapping would also
    // work but says nothing about intent.
    let status = real_main();
    match status {
        0 => ExitCode::SUCCESS,
        other => ExitCode::from(other.clamp(1, 255) as u8),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rev::rev;

    fn parse(args: &[&str]) -> Result<Action, String> {
        parse_args(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>())
    }

    fn run_options(args: &[&str]) -> Options {
        match parse(args) {
            Ok(Action::Run(options)) => options,
            other => panic!("expected options for {:?}, got {:?}", args, other),
        }
    }

    #[test]
    fn no_operands_reads_stdin() {
        assert_eq!(
            run_options(&[]),
            Options { sep: Separator::Newline, files: vec![] }
        );
    }

    #[test]
    fn files_keep_their_order() {
        assert_eq!(
            run_options(&["a.txt", "b.txt", "c.txt"]).files,
            vec!["a.txt", "b.txt", "c.txt"]
        );
    }

    #[test]
    fn options_permute_around_operands() {
        let options = run_options(&["a.txt", "-0", "b.txt"]);
        assert_eq!(options.sep, Separator::Zero);
        assert_eq!(options.files, vec!["a.txt", "b.txt"]);
    }

    #[test]
    fn short_flags_cluster() {
        assert!(matches!(parse(&["-00"]), Ok(Action::Run(_))));
        // First flag wins and short-circuits, exactly like getopt.
        assert!(matches!(parse(&["-0h"]), Ok(Action::Help)));
        // Everything up to the first bad flag is still rejected.
        assert_eq!(parse(&["-0q"]).unwrap_err(), "invalid option -- 'q'".to_string());
    }

    #[test]
    fn long_flags_match() {
        assert_eq!(run_options(&["--zero"]).sep, Separator::Zero);
        assert!(matches!(parse(&["--help"]), Ok(Action::Help)));
        assert!(matches!(parse(&["--version"]), Ok(Action::Version)));
    }

    #[test]
    fn double_dash_ends_options() {
        let options = run_options(&["--", "-0", "--weird"]);
        assert_eq!(options.sep, Separator::Newline);
        assert_eq!(options.files, vec!["-0", "--weird"]);
    }

    #[test]
    fn bad_options_are_rejected_with_getopt_style_messages() {
        assert_eq!(
            parse(&["-q"]).unwrap_err(),
            "invalid option -- 'q'".to_string()
        );
        assert_eq!(
            parse(&["--bogus"]).unwrap_err(),
            "unrecognized option '--bogus'".to_string()
        );
    }

    #[test]
    fn reverse_stream_agrees_with_the_pure_function() {
        for sample in ["", "\n", "abc", "abc\n", "a\n\nb", "\u{4f60}\u{597d}\nx"] {
            for sep in [Separator::Newline, Separator::Zero] {
                let mut streamed = Vec::new();
                reverse_stream(&mut sample.as_bytes(), &mut streamed, sep).unwrap();
                assert_eq!(
                    streamed,
                    rev(sample.as_bytes(), sep),
                    "streamed output differs for {:?} with {:?}",
                    sample,
                    sep
                );
            }
        }
    }
}
