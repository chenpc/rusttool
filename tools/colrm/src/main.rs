//! `colrm` — remove columns from its input, like util-linux `colrm(1)`.
//!
//! This is the I/O half; the column arithmetic lives in the `colrm` library
//! crate so it can be unit-tested on the host without touching stdin.
//!
//! Behaviour copied from upstream `text-utils/colrm.c`:
//!
//! * input is always standard input — `colrm` takes no file operands,
//! * with no operands nothing is removed (upstream's `!first` test), so the
//!   tool degenerates to a verbatim copy,
//! * at most two operands are read (`first`, then `last`); any further operand
//!   is silently ignored, exactly like upstream's `if (argc > 1)` / `if (argc >
//!   2)` pair,
//! * both operands go through `strtoul_or_err()`, i.e. C `strtoul()` with base
//!   0: decimal, `0`-prefixed octal and `0x`-prefixed hex are all accepted and
//!   trailing garbage is an error,
//! * `-h`/`--help` and `-V`/`--version` print to stdout and exit 0,
//! * an unusable option or operand reports on stderr and exits
//!   `EXIT_FAILURE`.

use std::io::{self, Read, Write};
use std::process::ExitCode;

use colrm::{version_line, Range, HELP};

/// Where a command line stops after `-h`/`-V` short-circuits the rest.
#[derive(Debug)]
enum Action {
    Help,
    Version,
    Run(Range),
}

/// Why the command line was refused.
///
/// The two classes are not the same: an option this tool does not have is a usage
/// error and gets the `Try 'colrm --help'` line after it, while an operand that is
/// not a number is reported on its own, with no such line. Upstream's `usage()`
/// and its bare `error (EXIT_FAILURE, ...)` calls are what make the difference,
/// and it is visible.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Rejected {
    /// Print `Try 'colrm --help' for more information.` afterwards.
    Usage(String),
    /// Print the message and nothing else.
    Operand(String),
}

impl Rejected {
    fn message(&self) -> &str {
        match self {
            Rejected::Usage(message) | Rejected::Operand(message) => message,
        }
    }

    fn wants_try_help(&self) -> bool {
        matches!(self, Rejected::Usage(_))
    }
}

fn usage_hint() -> String {
    "Try 'colrm --help' for more information.".to_string()
}

/// `strtoul(str, &end, 0)` restricted to what `strtoul_or_err()` accepts: no
/// leading blanks, a valid base-0 numeral, and nothing but blanks after it.
fn parse_ulong(text: &str) -> Option<u64> {
    let trimmed = text.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let (digits, radix) = if let Some(rest) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
        (rest, 16)
    } else if trimmed.len() > 1 && trimmed.starts_with('0') {
        (&trimmed[1..], 8)
    } else {
        (trimmed, 10)
    };
    // A bare "0" is the only valid numeral whose body may be empty.
    if digits.is_empty() {
        return if radix == 10 { Some(0) } else { None };
    }
    if digits.chars().any(|c| !c.is_digit(radix)) {
        return None;
    }
    u64::from_str_radix(digits, radix).ok()
}

/// `getopt_long`-style parsing with GNU permutation (options may follow
/// operands). Upstream only knows `-h` and `-V`; `--help`/`--version` are the
/// long forms registered in its `longopts[]`.
fn parse_args(args: &[String]) -> Result<Action, Rejected> {
    let mut operands: Vec<String> = Vec::new();
    let mut operands_only = false;

    for arg in args {
        if operands_only || !arg.starts_with('-') || arg == "-" {
            operands.push(arg.clone());
            continue;
        }
        if arg == "--" {
            operands_only = true;
            continue;
        }
        if let Some(long) = arg.strip_prefix("--") {
            match long {
                "help" => return Ok(Action::Help),
                "version" => return Ok(Action::Version),
                other => {
                    return Err(Rejected::Usage(format!(
                        "unrecognized option '--{}'",
                        other
                    )))
                }
            }
        }
        for flag in arg.chars().skip(1) {
            match flag {
                'h' => return Ok(Action::Help),
                'V' => return Ok(Action::Version),
                other => {
                    return Err(Rejected::Usage(format!("invalid option -- '{}'", other)))
                }
            }
        }
    }

    // Upstream reads argv[1] and argv[2] only; extra operands are ignored.
    let mut range = Range::default();
    for (index, label) in [(0, "first"), (1, "second")] {
        let Some(text) = operands.get(index) else {
            break;
        };
        let value = parse_ulong(text)
            .ok_or_else(|| Rejected::Operand(format!("{} argument: '{}'", label, text)))?;
        if index == 0 {
            range.first = value;
        } else {
            range.last = value;
        }
    }
    Ok(Action::Run(range))
}

fn real_main() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let range = match parse_args(&args) {
        Ok(Action::Help) => {
            print!("{}", HELP);
            return 0;
        }
        Ok(Action::Version) => {
            print!("{}", version_line());
            return 0;
        }
        Ok(Action::Run(range)) => range,
        Err(rejected) => {
            eprintln!("colrm: {}", rejected.message());
            if rejected.wants_try_help() {
                eprintln!("{}", usage_hint());
            }
            return 1;
        }
    };

    let mut input = String::new();
    if let Err(error) = io::stdin().read_to_string(&mut input) {
        eprintln!("colrm: stdin: {}", error);
        return 1;
    }

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if let Err(error) = out
        .write_all(colrm::remove_columns(&input, range).as_bytes())
        .and_then(|()| out.flush())
    {
        eprintln!("colrm: stdout: {}", error);
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

    fn parse(args: &[&str]) -> Result<Action, Rejected> {
        parse_args(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>())
    }

    fn run_range(args: &[&str]) -> Range {
        match parse(args) {
            Ok(Action::Run(range)) => range,
            other => panic!("expected a range for {:?}, got {:?}", args, other),
        }
    }

    #[test]
    fn no_operands_selects_the_pass_through_range() {
        assert_eq!(run_range(&[]), Range { first: 0, last: 0 });
    }

    #[test]
    fn one_operand_is_the_start_column() {
        assert_eq!(run_range(&["3"]), Range { first: 3, last: 0 });
    }

    #[test]
    fn two_operands_are_start_and_stop() {
        assert_eq!(run_range(&["3", "5"]), Range { first: 3, last: 5 });
    }

    #[test]
    fn extra_operands_are_ignored_like_upstream() {
        assert_eq!(run_range(&["3", "5", "9", "x"]), Range { first: 3, last: 5 });
    }

    #[test]
    fn options_permute_around_operands() {
        // GNU permutation would interleave operands with options, but -V
        // short-circuits like getopt: the first option action wins and the
        // range is never built.
        assert!(matches!(parse(&["3", "-V", "5", "-V"]), Ok(Action::Version)));
        assert!(matches!(parse(&["3", "-V"]), Ok(Action::Version)));
        // A lone "-" is an operand for getopt purposes, and then fails the
        // strtoul_or_err() check like any other non-numeral.
        assert_eq!(
            parse(&["3", "-"]).unwrap_err().message(),
            "second argument: '-'"
        );
    }

    #[test]
    fn operand_errors_and_option_errors_are_two_classes() {
        // An operand that is not a number is reported on its own, while an option
        // this tool does not have is a usage error and gets the Try line. Upstream
        // has a bare `error()` for the first and `usage()` for the second, and the
        // difference is visible.
        let operand = parse(&["zz"]).unwrap_err();
        assert!(!operand.wants_try_help());
        assert_eq!(operand, Rejected::Operand("first argument: 'zz'".to_string()));

        let option = parse(&["-x"]).unwrap_err();
        assert!(option.wants_try_help());
        assert_eq!(option, Rejected::Usage("invalid option -- 'x'".to_string()));

        let unknown = parse(&["--bogus"]).unwrap_err();
        assert!(unknown.wants_try_help());
    }

    #[test]
    fn help_and_version_short_circuit() {
        assert!(matches!(parse(&["-h"]), Ok(Action::Help)));
        assert!(matches!(parse(&["--help"]), Ok(Action::Help)));
        assert!(matches!(parse(&["-V"]), Ok(Action::Version)));
        assert!(matches!(parse(&["--version"]), Ok(Action::Version)));
        // Like getopt, the first action wins and the rest is not examined.
        assert!(matches!(parse(&["-Vh"]), Ok(Action::Version)));
        assert!(matches!(parse(&["-hV"]), Ok(Action::Help)));
    }

    #[test]
    fn bad_options_are_rejected_with_getopt_style_messages() {
        assert_eq!(parse(&["-x"]).unwrap_err().message(), "invalid option -- 'x'");
        assert_eq!(
            parse(&["--bogus"]).unwrap_err().message(),
            "unrecognized option '--bogus'"
        );
    }

    #[test]
    fn bad_operands_report_which_one_failed() {
        assert_eq!(
            parse(&["zz"]).unwrap_err().message(),
            "first argument: 'zz'"
        );
        assert_eq!(
            parse(&["1", "3x"]).unwrap_err().message(),
            "second argument: '3x'"
        );
        assert_eq!(parse(&[""]).unwrap_err().message(), "first argument: ''");
        assert_eq!(
            parse(&["-3"]).unwrap_err().message(),
            "invalid option -- '3'"
        );
    }

    #[test]
    fn double_dash_ends_options() {
        assert_eq!(run_range(&["--", "2", "4"]), Range { first: 2, last: 4 });
        // Without "--", "-3" would be taken for a cluster of short options.
        assert_eq!(
            parse(&["--", "-3"]).unwrap_err().message(),
            "first argument: '-3'"
        );
    }

    #[test]
    fn strtoul_or_err_accepts_base_zero_numerals() {
        assert_eq!(parse_ulong("0"), Some(0));
        assert_eq!(parse_ulong("12"), Some(12));
        assert_eq!(parse_ulong(" 12"), Some(12));
        assert_eq!(parse_ulong("010"), Some(8));
        assert_eq!(parse_ulong("0x1f"), Some(31));
        assert_eq!(parse_ulong("0X10"), Some(16));
        assert_eq!(parse_ulong("0x"), None);
        assert_eq!(parse_ulong("1 2"), None);
        assert_eq!(parse_ulong("-1"), None);
        assert_eq!(parse_ulong(""), None);
    }

    #[test]
    fn parse_ulong_rejects_the_empty_octal_body() {
        // "0" alone is decimal zero; "0x" and "0b" are not numerals.
        assert_eq!(parse_ulong("0"), Some(0));
        assert_eq!(parse_ulong("0x"), None);
        assert_eq!(parse_ulong("0g"), None);
    }
}
