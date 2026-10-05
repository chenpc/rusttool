//! `head(1)`: print the first part of files.

use std::io::{Read, Write};

use head::{header, parse_count, Count, Options, Taker};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: head [OPTION]... [FILE]...

Print first part of FILE(s).

  -c, --bytes=[-]N  print first N bytes
  -n, --lines=[-]N  print first N lines
  -q, --quiet       never print file headers
  -v, --verbose     always print file headers
      --help        display this help and exit
      --version     output version information and exit
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
    let mut options = Options::default();
    let mut files: Vec<String> = Vec::new();
    let mut no_more = false;
    let mut i = 0usize;
    let mut want_bytes = false;

    while i < args.len() {
        let arg = args[i].clone();
        i += 1;
        if no_more || arg == "-" || !arg.starts_with('-') {
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
                    println!("head (rusttool) {}", VERSION);
                    return;
                }
                "bytes" => {
                    want_bytes = true;
                    let shown = value.clone();
                    match option_value(&args, &mut i, value).and_then(|v| parse_count(&v)) {
                        Some(c) => options.count = c,
                        None => {
                            eprintln!("head: invalid number of bytes: '{}'", shown.unwrap_or_default());
                            std::process::exit(1);
                        }
                    }
                }
                "lines" => {
                    want_bytes = false;
                    let shown = value.clone();
                    match option_value(&args, &mut i, value).and_then(|v| parse_count(&v)) {
                        Some(c) => options.count = c,
                        None => {
                            eprintln!("head: invalid number of lines: '{}'", shown.unwrap_or_default());
                            std::process::exit(1);
                        }
                    }
                }
                "quiet" => options.quiet = true,
                "verbose" => options.verbose = true,
                other => {
                    eprintln!("head: unrecognized option '--{}'", other);
                    eprintln!("Try 'head --help' for more information.");
                    std::process::exit(1);
                }
            }
            continue;
        }
        let chars: Vec<char> = arg[1..].chars().collect();
        let mut idx = 0usize;
        while idx < chars.len() {
            match chars[idx] {
                'q' => options.quiet = true,
                'v' => options.verbose = true,
                'c' | 'n' => {
                    want_bytes = chars[idx] == 'c';
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
                    let text = option_value(&args, &mut i, inline).unwrap_or_default();
                    match parse_count(&text) {
                        // -c counts bytes, -n counts lines.
                        Some(Count::Lines(n)) if want_bytes => options.count = Count::Bytes(n),
                        Some(c) => options.count = c,
                        None => {
                            eprintln!("head: invalid number of lines: '{}'", text);
                            std::process::exit(1);
                        }
                    }
                    continue;
                }
                other => {
                    eprintln!("head: invalid option -- '{}'", other);
                    eprintln!("Try 'head --help' for more information.");
                    std::process::exit(1);
                }
            }
            idx += 1;
        }
    }
    let _ = want_bytes;

    let sources: Vec<Option<String>> = if files.is_empty() {
        vec![None]
    } else {
        files.iter().map(|f| if f == "-" { None } else { Some(f.clone()) }).collect()
    };

    let show_headers = files.len() > 1 && !options.quiet;
    let mut failed = false;
    let mut first = true;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for source in sources {
        let label = source.clone().unwrap_or_else(|| "standard input".to_string());
        // Open the source before printing the header: GNU reports an unopenable
        // file without ever printing its banner.
        let reader: Box<dyn Read> = match &source {
            None => Box::new(std::io::stdin()),
            Some(path) => match std::fs::File::open(path) {
                Ok(file) => Box::new(file),
                Err(e) => {
                    eprintln!("head: cannot open '{}' for reading: {}", path, errno(&e));
                    failed = true;
                    continue;
                }
            },
        };
        let mut reader = reader;
        let mut buffer = [0u8; 8192];
        let mut taker = Taker::new(options.count);
        if show_headers || options.verbose {
            // GNU separates consecutive per-file sections with a blank line.
            if !first {
                let _ = out.write_all(b"\n");
            }
            first = false;
            let _ = out.write_all(header(&label).as_bytes());
        }
        loop {
            let read = match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => {
                    eprintln!("head: {}", e);
                    failed = true;
                    break;
                }
            };
            let piece = taker.push(&buffer[..read]);
            if !piece.is_empty() {
                let _ = out.write_all(piece);
            }
            if taker.done() {
                break;
            }
        }
    }
    let _ = out.flush();
    if failed {
        std::process::exit(1);
    }
}

fn errno(e: &std::io::Error) -> String {
    let text = e.to_string();
    match text.find(" (os error ") {
        Some(idx) => text[..idx].to_string(),
        None => text,
    }
}
