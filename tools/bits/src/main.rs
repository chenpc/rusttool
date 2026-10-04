//! `bits` — convert bit masks from/to various formats, like util-linux `bits(1)`.
//!
//! I/O half; the mask/list engine lives in the `bits` library crate.

use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use bits::{BitSet, OutputMode, DEFAULT_WIDTH, MAX_WIDTH};

#[derive(Debug)]
enum Action {
    Help,
    Version,
    Run(Options),
}

#[derive(Debug)]
struct Options {
    width: usize,
    fail_width: bool,
    mode: OutputMode,
    groups: Vec<String>,
}

fn usage_hint() -> &'static str {
    "try 'bits --help' for more information."
}

fn parse_width(s: &str) -> Result<usize, String> {
    match s.parse::<usize>() {
        Ok(n) if n >= 1 && n <= MAX_WIDTH => Ok(n),
        _ => Err("invalid --width".to_string()),
    }
}

fn parse_args(args: &[String]) -> Result<Action, String> {
    let mut width = DEFAULT_WIDTH;
    let mut fail_width = false;
    let mut mode = OutputMode::Mask;
    let mut groups: Vec<String> = Vec::new();
    let mut operands_only = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if operands_only || !arg.starts_with('-') || arg == "-" {
            groups.push(arg.clone());
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
                "width" => {
                    let v = match value {
                        Some(v) => v,
                        None => {
                            i += 1;
                            if i >= args.len() {
                                return Err("option '--width' requires an argument".to_string());
                            }
                            args[i].clone()
                        }
                    };
                    width = parse_width(&v)?;
                }
                "fail-width" => {
                    if value.is_some() {
                        return Err("option '--fail-width' doesn't allow an argument".to_string());
                    }
                    fail_width = true;
                }
                "mask" => mode = OutputMode::Mask,
                "grouped-mask" => mode = OutputMode::GroupedMask,
                "binary" => mode = OutputMode::Binary,
                "expand" => mode = OutputMode::Expand,
                "list" => mode = OutputMode::List,
                "help" => return Ok(Action::Help),
                "version" => return Ok(Action::Version),
                other => return Err(format!("unrecognized option '--{}'", other)),
            }
            i += 1;
            continue;
        }
        // Short cluster; `w` consumes the rest of the arg or the next one.
        let chars: Vec<char> = arg.chars().skip(1).collect();
        let mut j = 0;
        while j < chars.len() {
            match chars[j] {
                'w' => {
                    let rest: String = chars[j + 1..].iter().collect();
                    let v = if rest.is_empty() {
                        i += 1;
                        if i >= args.len() {
                            return Err("option '-w' requires an argument".to_string());
                        }
                        args[i].clone()
                    } else {
                        rest
                    };
                    width = parse_width(&v)?;
                    break;
                }
                'f' => fail_width = true,
                'm' => mode = OutputMode::Mask,
                'g' => mode = OutputMode::GroupedMask,
                'b' => mode = OutputMode::Binary,
                'e' => mode = OutputMode::Expand,
                'l' => mode = OutputMode::List,
                'h' => return Ok(Action::Help),
                'V' => return Ok(Action::Version),
                other => return Err(format!("invalid option -- '{}'", other)),
            }
            j += 1;
        }
        i += 1;
    }
    Ok(Action::Run(Options { width, fail_width, mode, groups }))
}

fn real_main() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse_args(&args) {
        Ok(Action::Help) => {
            print!("{}", bits::HELP);
            return 0;
        }
        Ok(Action::Version) => {
            print!("{}", bits::version_line());
            return 0;
        }
        Ok(Action::Run(o)) => o,
        Err(message) => {
            eprintln!("bits: {}", message);
            eprintln!("bits: {}", usage_hint());
            return 1;
        }
    };

    // With no operands, groups come from stdin, one per line (trailing
    // whitespace stripped, like upstream's rtrim).
    let groups: Vec<String> = if options.groups.is_empty() {
        let stdin = io::stdin();
        let mut lines = Vec::new();
        for line in stdin.lock().lines() {
            match line {
                Ok(l) => lines.push(l.trim_end().to_string()),
                Err(e) => {
                    eprintln!("bits: stdin: {}", e);
                    return 1;
                }
            }
        }
        lines
    } else {
        options.groups
    };

    let mut all = BitSet::new(options.width);
    for g in &groups {
        if let Err(e) = bits::apply_group(g, &mut all, options.fail_width) {
            eprintln!("bits: error: {}", e);
            return 1;
        }
    }

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let rendered = bits::render(&all, options.mode);
    if let Err(e) = out.write_all(&rendered).and_then(|()| out.flush()) {
        eprintln!("bits: stdout: {}", e);
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
    fn defaults_are_mask_width_8192() {
        let o = opts(&[]);
        assert_eq!(o.width, DEFAULT_WIDTH);
        assert!(!o.fail_width);
        assert_eq!(o.mode, OutputMode::Mask);
        assert!(o.groups.is_empty());
    }

    #[test]
    fn last_mode_wins() {
        assert_eq!(opts(&["-l", "-m"]).mode, OutputMode::Mask);
        assert_eq!(opts(&["--mask", "--list"]).mode, OutputMode::List);
        assert_eq!(opts(&["-e"]).mode, OutputMode::Expand);
        assert_eq!(opts(&["-b"]).mode, OutputMode::Binary);
        assert_eq!(opts(&["-g"]).mode, OutputMode::GroupedMask);
    }

    #[test]
    fn width_forms() {
        assert_eq!(opts(&["-w", "64"]).width, 64);
        assert_eq!(opts(&["-w64"]).width, 64);
        assert_eq!(opts(&["--width", "64"]).width, 64);
        assert_eq!(opts(&["--width=64"]).width, 64);
        assert!(parse(&["--width", "0"]).is_err());
        assert!(parse(&["-w", "abc"]).is_err());
    }

    #[test]
    fn operands_survive_option_permutation() {
        let o = opts(&["1,2", "-l", "0xeec2"]);
        assert_eq!(o.groups, vec!["1,2", "0xeec2"]);
        let o = opts(&["--", "-l"]);
        assert_eq!(o.groups, vec!["-l"]);
    }

    #[test]
    fn bad_options_report_getopt_style() {
        assert_eq!(parse(&["-q"]).unwrap_err(), "invalid option -- 'q'".to_string());
        assert!(parse(&["--bogus"]).unwrap_err().contains("unrecognized option"));
    }
}
