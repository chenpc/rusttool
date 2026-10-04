//! `tail(1)`: print the last part of files.

use std::io::{Read, Write};

use tail::{header, parse_lines, take, Count};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: tail [OPTION]... [FILE]...

Print last part of FILE(s).

  -c, --bytes=[-]N   print last N bytes
  -n, --lines=[-]N   print last N lines (default 10)
  -q, --quiet        never print file headers
  -v, --verbose      always print file headers
      --help         display this help and exit
      --version      output version information and exit
";


/// Pull an option value: the inline `--opt=value` text, or the next argument.
fn option_value(
    args: &[String],
    i: &mut usize,
    inline: Option<String>,
) -> Option<String> {
    match inline {
        Some(v) => Some(v),
        None => {
            if *i < args.len() {
                let v = args[*i].clone();
                *i += 1;
                Some(v)
            } else {
                None
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut count = Count::Lines(10);
    let mut quiet = false;
    let mut verbose = false;
    let mut files: Vec<String> = Vec::new();
    let mut no_more = false;
    let mut i = 0usize;

    while i < args.len() {
        let arg = args[i].clone();
        i += 1;
        if no_more || arg == "-" || !arg.starts_with('-') || arg == "--" && files.is_empty() && false {
            files.push(arg);
            continue;
        }
        if arg == "--" {
            no_more = true;
            continue;
        }
        if let Some(long) = arg.strip_prefix("--") {
            let (name, value) = match long.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (long, None),
            };
            match name {
                "help" => {
                    print!("{}", HELP);
                    return;
                }
                "version" => {
                    println!("tail (rusttool) {}", VERSION);
                    return;
                }
                "bytes" | "lines" => {
                    let text = option_value(&args, &mut i, value.clone()).unwrap_or_default();
                    let parsed = if name == "bytes" {
                        text.trim_start_matches('+')
                            .parse::<u64>()
                            .ok()
                            .map(Count::Bytes)
                    } else {
                        parse_lines(&text)
                    };
                    match parsed {
                        Some(c) => count = c,
                        None => {
                            eprintln!("tail: invalid number of {}: '{}'", name, text);
                            std::process::exit(1);
                        }
                    }
                }
                "quiet" => quiet = true,
                "verbose" => verbose = true,
                other => {
                    eprintln!("tail: unrecognized option '--{}'", other);
                    eprintln!("Try 'tail --help' for more information.");
                    std::process::exit(1);
                }
            }
            continue;
        }
        let chars: Vec<char> = arg[1..].chars().collect();
        let mut idx = 0usize;
        while idx < chars.len() {
            match chars[idx] {
                'q' => quiet = true,
                'v' => verbose = true,
                'c' | 'n' => {
                    let inline: Option<String> = if idx + 1 < chars.len() {
                        Some(chars[idx + 1..].iter().collect())
                    } else {
                        None
                    };
                    if inline.is_some() {
                        idx = chars.len();
                    } else {
                        idx += 1;
                    }
                    let text = match inline {
                        Some(v) => v,
                        None => {
                            if i < args.len() {
                                let v = args[i].clone();
                                i += 1;
                                v
                            } else {
                                eprintln!("tail: option requires an argument -- '{}'", chars[idx]);
                                std::process::exit(1);
                            }
                        }
                    };
                    let parsed = if chars.get(idx.wrapping_sub(1)) == Some(&'c') {
                        text.trim_start_matches('+').parse::<u64>().ok().map(Count::Bytes)
                    } else {
                        parse_lines(&text)
                    };
                    match parsed {
                        Some(c) => count = c,
                        None => {
                            eprintln!("tail: invalid number: '{}'", text);
                            std::process::exit(1);
                        }
                    }
                    continue;
                }
                other => {
                    eprintln!("tail: invalid option -- '{}'", other);
                    eprintln!("Try 'tail --help' for more information.");
                    std::process::exit(1);
                }
            }
            idx += 1;
        }
    }

    let sources: Vec<Option<String>> = if files.is_empty() {
        vec![None]
    } else {
        files.iter().map(|f| if f == "-" { None } else { Some(f.clone()) }).collect()
    };

    let show_headers = files.len() > 1 && !quiet;
    let mut failed = false;
    let mut first = true;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for source in sources {
        let label = source.clone().unwrap_or_else(|| "standard input".to_string());
        let mut data = Vec::new();
        match &source {
            None => {
                if let Err(e) = std::io::stdin().read_to_end(&mut data) {
                    eprintln!("tail: standard input: {}", e);
                    failed = true;
                    continue;
                }
            }
            Some(path) => match std::fs::read(path) {
                Ok(bytes) => data = bytes,
                Err(e) => {
                    eprintln!("tail: cannot open '{}' for reading: {}", path, e);
                    failed = true;
                    continue;
                }
            },
        }
        if show_headers || verbose {
            // GNU separates consecutive per-file sections with a blank line.
            if !first {
                let _ = out.write_all(b"\n");
            }
            first = false;
            let _ = out.write_all(header(&label).as_bytes());
        }
        let _ = out.write_all(&take(&data, count));
    }
    let _ = out.flush();
    if failed {
        std::process::exit(1);
    }
}
