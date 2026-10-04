//! `column` — format input into columns, like util-linux `column(1)`.
//!
//! The I/O half; [`column::columnate`] holds the layout rules so they can be
//! unit-tested on the host.
//!
//! Behaviour copied from upstream `text-utils/column.c` (2.39.3):
//!
//! * files are read in order, or standard input when none are given, and an
//!   unreadable file is a warning that makes the exit status non-zero,
//! * the display width comes from `-c`/`--output-width`, defaults to 80 when
//!   standard output is not a terminal, and `-c unlimited` or `-c 0` means
//!   "no limit",
//! * `-t` builds a table, `-s`/`--separator` gives the input separator
//!   characters and switches off greedy splitting, `-o` the string placed
//!   between table columns,
//! * `-x` fills rows instead of columns, and `-l`/`--table-columns-limit` caps
//!   the number of input *columns*,
//! * `-N` names the table columns and `-d` suppresses that header,
//! * `-L` keeps empty input lines, `-n` names a JSON table,
//! * `-t` and `-x` are mutually exclusive, as are `-N` and the table-less use
//!   of any `--table-*` option,
//! * `-h`/`-V` print to stdout and exit 0, and an unusable option reports on
//!   stderr with a hint and exits `EXIT_FAILURE`.
//!
//! Limitation: there is no JSON output, so `-n` has no effect beyond validation.

use std::fs::File;
use std::io::{self, Read, Write};
use std::process::ExitCode;

use column::{version_line, Config, Mode, DEFAULT_OUTPUT_SEPARATOR, DEFAULT_SEPARATOR, HELP};

/// Where a command line stops after `-h`/`-V` short-circuits the rest.
#[derive(Debug)]
enum Action {
    Help,
    Version,
    /// The resolved config plus the non-option arguments, i.e. the file
    /// names, which `getopt_long()` hands back through `argv[optind..]`.
    Run(Config, Vec<String>),
}

/// `strtou32_or_err()`: optional sign, then decimal digits and nothing else.
fn parse_width(text: &str) -> Result<usize, String> {
    if text == "unlimited" {
        return Ok(0);
    }
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err("invalid columns argument".to_string());
    }
    let value: i64 = text.parse().map_err(|_| "invalid columns argument".to_string())?;
    Ok(if value <= 0 { 0 } else { value as usize })
}

fn usage_hint() -> String {
    "Try 'column --help' for more information.".to_string()
}

/// Build the input separator set from `-s`, dropping duplicates.
fn separators(text: &str) -> Vec<char> {
    let mut set: Vec<char> = Vec::new();
    for c in text.chars() {
        if !set.contains(&c) {
            set.push(c);
        }
    }
    set
}

/// `getopt_long()`-style parsing with GNU permutation (options may follow
/// operands), short-option clustering, and arguments that are either attached
/// to the flag or given as the next argument.
///
/// The short option string is upstream's `"C:c:dE:eH:hi:Jl:LN:n:mO:o:p:R:r:s:T:tVW:x"`.
fn parse_args(args: &[String]) -> Result<Action, String> {
    let mut cfg = Config::default();
    let mut table_name = false;
    let mut saw_table = false;
    let mut saw_fillrows = false;
    let mut files: Vec<String> = Vec::new();
    let mut index = 0usize;
    let mut operands_only = false;

    while index < args.len() {
        let arg = args[index].clone();
        index += 1;
        if operands_only || !arg.starts_with('-') || arg == "-" {
            files.push(arg);
            continue;
        }
        if arg == "--" {
            operands_only = true;
            continue;
        }

        if let Some(long) = arg.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((name, value)) => (name.to_string(), Some(value.to_string())),
                None => (long.to_string(), None),
            };
            // Required arguments may be attached with '=' or given next.
            let mut take = |expected: &str| -> Result<String, String> {
                if let Some(value) = inline.clone() {
                    return Ok(value);
                }
                if index < args.len() {
                    let value = args[index].clone();
                    index += 1;
                    return Ok(value);
                }
                Err(format!("option '--{}' requires an argument", expected))
            };
            match name.as_str() {
                "help" => return Ok(Action::Help),
                "version" => return Ok(Action::Version),
                "table" => {
                    cfg.mode = Mode::Table;
                    saw_table = true;
                }
                "fillrows" => {
                    cfg.mode = Mode::FillRows;
                    saw_fillrows = true;
                }
                "table-noheadings" => cfg.no_headings = true,
                "keep-empty-lines" | "table-empty-lines" | "empty-lines" => {
                    cfg.keep_empty_lines = true;
                }
                // `required_argument`
                "output-width" | "width" | "columns" => cfg.width = parse_width(&take("output-width")?)?,
                "output-separator" => cfg.output_separator = take("output-separator")?,
                "separator" | "separators" => {
                    cfg.separator = separators(&take("separator")?);
                    cfg.greedy = false;
                }
                "table-name" => {
                    take("table-name")?;
                    table_name = true;
                }
                // `required_argument`, comma separated
                "table-columns" | "table-columnnames" => {
                    cfg.names = Some(take("table-columns")?.split(',').map(str::to_string).collect());
                }
                "table-columns-limit" | "last-column" => {
                    let value = take("table-columns-limit")?;
                    let limit: usize = value.parse().map_err(|_| "invalid columns limit argument")?;
                    if limit == 0 {
                        return Err("columns limit must be greater than zero".to_string());
                    }
                    cfg.max_columns = Some(limit);
                }
                other => return Err(format!("unrecognized option '--{}'", other)),
            }
            continue;
        }

        let flags: Vec<char> = arg.chars().skip(1).collect();
        let mut at = 0usize;
        while at < flags.len() {
            let flag = flags[at];
            at += 1;
            // The argument is the rest of the cluster, else the next argv entry.
            let mut take = |flag: char| -> Result<String, String> {
                let attached: String = flags[at..].iter().collect();
                if !attached.is_empty() {
                    at = flags.len();
                    return Ok(attached);
                }
                if index < args.len() {
                    let value = args[index].clone();
                    index += 1;
                    return Ok(value);
                }
                Err(format!("option requires an argument -- '{}'", flag))
            };
            match flag {
                'h' => return Ok(Action::Help),
                'V' => return Ok(Action::Version),
                'c' => cfg.width = parse_width(&take('c')?)?,
                'o' => cfg.output_separator = take('o')?,
                's' => {
                    cfg.separator = separators(&take('s')?);
                    cfg.greedy = false;
                }
                'n' => {
                    take('n')?;
                    table_name = true;
                }
                'N' => cfg.names = Some(take('N')?.split(',').map(str::to_string).collect()),
                'l' => {
                    let value = take('l')?;
                    let limit: usize = value.parse().map_err(|_| "invalid columns limit argument")?;
                    if limit == 0 {
                        return Err("columns limit must be greater than zero".to_string());
                    }
                    cfg.max_columns = Some(limit);
                }
                't' => {
                    cfg.mode = Mode::Table;
                    saw_table = true;
                }
                'x' => {
                    cfg.mode = Mode::FillRows;
                    saw_fillrows = true;
                }
                'd' => cfg.no_headings = true,
                'L' => cfg.keep_empty_lines = true,
                other => return Err(format!("invalid option -- '{}'", other)),
            }
        }
    }

    if saw_table && saw_fillrows {
        return Err("options '--table' and '--fillrows' are mutually exclusive".to_string());
    }
    // Upstream refuses any --table-* option unless the output really is a
    // table.
    if cfg.mode != Mode::Table && (cfg.names.is_some() || table_name) {
        return Err("option --table required for all --table-*".to_string());
    }
    Ok(Action::Run(cfg, files))
}

fn real_main() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (cfg, files) = match parse_args(&args) {
        Ok(Action::Help) => {
            print!("{}", HELP);
            return 0;
        }
        Ok(Action::Version) => {
            print!("{}", version_line());
            return 0;
        }
        Ok(Action::Run(cfg, files)) => (cfg, files),
        Err(message) => {
            eprintln!("column: {}", message);
            eprintln!("{}", usage_hint());
            return 1;
        }
    };

    let mut input = String::new();
    if files.is_empty() {
        if let Err(error) = io::stdin().read_to_string(&mut input) {
            eprintln!("column: stdin: {}", error);
            return 1;
        }
    } else {
        // Upstream keeps going and only sets the exit status.
        let mut status = 0;
        for name in &files {
            match File::open(name).and_then(|mut file| file.read_to_string(&mut input)) {
                Ok(_) => {}
                Err(error) => {
                    eprintln!("column: {}: {}", name, error);
                    status = 1;
                }
            }
        }
        if status != 0 {
            return status;
        }
    }

    let entries = column::read_entries(&input, cfg.keep_empty_lines);
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if let Err(error) = out
        .write_all(column::columnate(&entries, &cfg).as_bytes())
        .and_then(|()| out.flush())
    {
        eprintln!("column: stdout: {}", error);
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

    fn run_cfg(args: &[&str]) -> Config {
        match parse(args) {
            Ok(Action::Run(cfg, _)) => cfg,
            other => panic!("expected a config for {:?}, got {:?}", args, other),
        }
    }

    fn run_files(args: &[&str]) -> Vec<String> {
        match parse(args) {
            Ok(Action::Run(_, files)) => files,
            other => panic!("expected files for {:?}, got {:?}", args, other),
        }
    }

    #[test]
    fn no_options_matches_the_upstream_default() {
        let cfg = run_cfg(&[]);
        assert_eq!(cfg.mode, Mode::FillCols);
        assert_eq!(cfg.width, column::DEFAULT_WIDTH);
        assert_eq!(cfg.separator, DEFAULT_SEPARATOR.chars().collect::<Vec<_>>());
        assert_eq!(cfg.output_separator, DEFAULT_OUTPUT_SEPARATOR);
        assert!(cfg.greedy);
        assert!(!cfg.keep_empty_lines);
        assert!(!cfg.no_headings);
        assert_eq!(cfg.names, None);
        assert_eq!(cfg.max_columns, None);
    }

    #[test]
    fn flags_select_the_mode() {
        assert_eq!(run_cfg(&["-t"]).mode, Mode::Table);
        assert_eq!(run_cfg(&["--table"]).mode, Mode::Table);
        assert_eq!(run_cfg(&["-x"]).mode, Mode::FillRows);
        assert_eq!(run_cfg(&["--fillrows"]).mode, Mode::FillRows);
    }

    #[test]
    fn table_and_fillrows_conflict() {
        assert_eq!(
            parse(&["-t", "-x"]).unwrap_err(),
            "options '--table' and '--fillrows' are mutually exclusive".to_string()
        );
        assert_eq!(
            parse(&["--table", "--fillrows"]).unwrap_err(),
            "options '--table' and '--fillrows' are mutually exclusive".to_string()
        );
        // Order does not matter.
        assert!(parse(&["-x", "-t"]).is_err());
    }

    #[test]
    fn table_only_options_require_a_table() {
        assert_eq!(
            parse(&["-N", "x,y"]).unwrap_err(),
            "option --table required for all --table-*".to_string()
        );
        assert_eq!(
            parse(&["-n", "tbl"]).unwrap_err(),
            "option --table required for all --table-*".to_string()
        );
        // -d and -L are harmless without -t.
        assert!(matches!(parse(&["-d"]), Ok(Action::Run(..))));
        assert!(matches!(parse(&["-L"]), Ok(Action::Run(..))));
        // With -t they are fine.
        assert!(matches!(parse(&["-t", "-N", "x"]), Ok(Action::Run(..))));
    }

    #[test]
    fn width_option_is_validated() {
        assert_eq!(run_cfg(&["-c20"]).width, 20);
        assert_eq!(run_cfg(&["-c", "20"]).width, 20);
        assert_eq!(run_cfg(&["--output-width=20"]).width, 20);
        assert_eq!(run_cfg(&["--output-width", "20"]).width, 20);
        // 0 and "unlimited" both mean no limit.
        assert_eq!(run_cfg(&["-c0"]).width, 0);
        assert_eq!(run_cfg(&["-c", "unlimited"]).width, 0);
        assert_eq!(run_cfg(&["-c", "-5"]).width, 0);
        assert_eq!(parse(&["-c", "xx"]).unwrap_err(), "invalid columns argument".to_string());
    }

    #[test]
    fn separators_option_replaces_the_default_set_and_stops_greedy() {
        assert_eq!(run_cfg(&["-s,"]).separator, vec![',']);
        // Duplicates collapse.
        assert_eq!(run_cfg(&["-s,,,"]).separator, vec![',']);
        assert_eq!(run_cfg(&["-s", ",;"]).separator, vec![',', ';']);
        assert_eq!(run_cfg(&["--separator=|"]).separator, vec!['|']);
        assert!(!run_cfg(&["-s,"]).greedy);
        assert!(run_cfg(&[]).greedy);
    }

    #[test]
    fn output_separator_option() {
        assert_eq!(run_cfg(&["-o", " | "]).output_separator, " | ");
        assert_eq!(run_cfg(&["-o|"]).output_separator, "|");
        assert_eq!(run_cfg(&["--output-separator=::"]).output_separator, "::");
    }

    #[test]
    fn names_and_heading_flags() {
        let cfg = run_cfg(&["-t", "-N", "x,y"]);
        assert_eq!(cfg.names, Some(vec!["x".to_string(), "y".to_string()]));
        assert_eq!(run_cfg(&["-t", "-Nx,y,z"]).names.as_ref().unwrap().len(), 3);
        assert_eq!(
            run_cfg(&["-t", "--table-columns=x,y"]).names,
            Some(vec!["x".to_string(), "y".to_string()])
        );
        assert!(run_cfg(&["-d"]).no_headings);
        assert!(run_cfg(&["--table-noheadings"]).no_headings);
    }

    #[test]
    fn empty_lines_flag() {
        assert!(run_cfg(&["-L"]).keep_empty_lines);
        assert!(run_cfg(&["--keep-empty-lines"]).keep_empty_lines);
    }

    #[test]
    fn columns_limit_is_validated() {
        assert_eq!(run_cfg(&["-t", "-l3"]).max_columns, Some(3));
        assert_eq!(run_cfg(&["-t", "-l", "3"]).max_columns, Some(3));
        assert_eq!(run_cfg(&["-t", "--table-columns-limit=4"]).max_columns, Some(4));
        assert_eq!(
            parse(&["-l", "0"]).unwrap_err(),
            "columns limit must be greater than zero".to_string()
        );
        assert_eq!(parse(&["-l", "zz"]).unwrap_err(), "invalid columns limit argument".to_string());
    }

    #[test]
    fn table_name_needs_a_table_but_is_then_accepted() {
        assert!(matches!(parse(&["-t", "-n", "tbl"]), Ok(Action::Run(..))));
        assert!(matches!(parse(&["-t", "--table-name=tbl"]), Ok(Action::Run(..))));
    }

    #[test]
    fn help_and_version_short_circuit() {
        assert!(matches!(parse(&["-h"]), Ok(Action::Help)));
        assert!(matches!(parse(&["--help"]), Ok(Action::Help)));
        assert!(matches!(parse(&["-V"]), Ok(Action::Version)));
        assert!(matches!(parse(&["--version"]), Ok(Action::Version)));
        assert!(matches!(parse(&["-Vh"]), Ok(Action::Version)));
        // Even after other options.
        assert!(matches!(parse(&["-t", "-V"]), Ok(Action::Version)));
    }

    #[test]
    fn bad_options_are_rejected() {
        assert_eq!(parse(&["-Q"]).unwrap_err(), "invalid option -- 'Q'".to_string());
        assert_eq!(
            parse(&["--bogus"]).unwrap_err(),
            "unrecognized option '--bogus'".to_string()
        );
        assert_eq!(
            parse(&["-c"]).unwrap_err(),
            "option requires an argument -- 'c'".to_string()
        );
        assert_eq!(
            parse(&["--output-width"]).unwrap_err(),
            "option '--output-width' requires an argument".to_string()
        );
        assert_eq!(
            parse(&["-N"]).unwrap_err(),
            "option requires an argument -- 'N'".to_string()
        );
    }

    #[test]
    fn parse_width_matches_upstream() {
        assert_eq!(parse_width("80"), Ok(80));
        assert_eq!(parse_width("+8"), Ok(8));
        assert_eq!(parse_width("0"), Ok(0));
        assert_eq!(parse_width("unlimited"), Ok(0));
        assert_eq!(parse_width(""), Err("invalid columns argument".to_string()));
        assert_eq!(parse_width("8x"), Err("invalid columns argument".to_string()));
    }

    #[test]
    fn option_arguments_are_not_mistaken_for_file_names() {
        assert_eq!(run_files(&["-c", "20", "data.txt"]), vec!["data.txt"]);
        assert_eq!(run_files(&["-t", "-N", "x,y"]), Vec::<String>::new());
        assert_eq!(run_files(&["-t", "-o", "|", "a", "b"]), vec!["a", "b"]);
        // GNU permutation: the file may come first.
        assert_eq!(run_files(&["a", "-c", "20", "b"]), vec!["a", "b"]);
        // A double dash ends the options.
        assert_eq!(run_files(&["--", "-c"]), vec!["-c"]);
    }
}