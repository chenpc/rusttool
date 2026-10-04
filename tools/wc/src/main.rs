//! `wc(1)`: count lines, words and bytes.

use std::io::Read;

use wc::{count, format_line, Options};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: wc [OPTION]... [FILE]...

Count lines, words and bytes in FILE(s).

  -c, --bytes     print the byte count
  -l, --lines     print the newline count
  -m, --chars     print the character count
  -w, --words     print the word count
      --help      display this help and exit
      --version   output version information and exit
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut options = Options::default();
    let mut files: Vec<String> = Vec::new();
    let mut no_more = false;

    for arg in &args {
        if no_more || *arg == "-" || !arg.starts_with('-') {
            files.push(arg.clone());
            continue;
        }
        if arg == "--" {
            no_more = true;
            continue;
        }
        if let Some(long) = arg.strip_prefix("--") {
            match long {
                "help" => {
                    print!("{}", HELP);
                    return;
                }
                "version" => {
                    println!("wc (rusttool) {}", VERSION);
                    return;
                }
                "bytes" => options.bytes = true,
                "lines" => options.lines = true,
                "chars" => options.chars = true,
                "words" => options.words = true,
                other => {
                    eprintln!("wc: unrecognized option '--{}'", other);
                    eprintln!("Try 'wc --help' for more information.");
                    std::process::exit(1);
                }
            }
            continue;
        }
        for ch in arg[1..].chars() {
            match ch {
                'c' => options.bytes = true,
                'l' => options.lines = true,
                'm' => options.chars = true,
                'w' => options.words = true,
                other => {
                    eprintln!("wc: invalid option -- '{}'", other);
                    eprintln!("Try 'wc --help' for more information.");
                    std::process::exit(1);
                }
            }
        }
    }

    let kinds = options.columns();
    let sources: Vec<Option<String>> = if files.is_empty() {
        vec![None]
    } else {
        files.iter().map(|f| if f == "-" { None } else { Some(f.clone()) }).collect()
    };

    let mut totals = vec![0u64; kinds.len()];
    let mut rows: Vec<(Vec<u64>, Option<String>)> = Vec::new();
    let mut failed = false;

    for source in &sources {
        let mut data = Vec::new();
        match source {
            None => {
                if let Err(e) = std::io::stdin().read_to_end(&mut data) {
                    eprintln!("wc: standard input: {}", e);
                    failed = true;
                    continue;
                }
            }
            Some(path) => match std::fs::read(path) {
                Ok(bytes) => data = bytes,
                Err(e) => {
                    eprintln!("wc: {}: {}", path, e);
                    failed = true;
                    continue;
                }
            },
        }
        let (l, w, b) = count(&data);
        let values = vec![l, w, b];
        let picked: Vec<u64> = kinds.iter().map(|k| values[match k {
            wc::Kind::Lines => 0,
            wc::Kind::Words => 1,
            wc::Kind::Bytes => 2,
        }]).collect();
        for (slot, value) in totals.iter_mut().zip(picked.iter()) {
            *slot += *value;
        }
        // GNU prints the name for every named operand, single file included.
        let label = source.clone();
        rows.push((picked, label));
    }

    // coreutils derives one print width from the input sizes: no operands, or a
    // single input with a single count, skips the stat entirely.
    let count_columns = kinds.len();
    let stats_known = !(files.is_empty() || (files.len() == 1 && count_columns == 1));
    let sizes: Vec<wc::InputSize> = sources
        .iter()
        .map(|source| match source {
            None => None,
            Some(path) => match std::fs::metadata(path) {
                Ok(meta) if meta.is_file() => Some(meta.len()),
                _ => None,
            },
        })
        .collect();
    let width = wc::number_width(&sizes, stats_known);

    for (values, label) in &rows {
        println!("{}", wc::format_line(values, width, label.as_deref()));
    }
    if rows.len() > 1 || options.total {
        println!("{}", wc::format_line(&totals, width, Some("total")));
    }
    if failed {
        std::process::exit(1);
    }
}
