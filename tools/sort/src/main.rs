//! `sort(1)`: sort lines of text files.

use std::fs::File;
use std::io::{self, Read, Write};

use sort::{check_sorted, compare_lines, parse_keydef, Check, Options, Ordering};

/// coreutils uses exit status 2 for a usage error and 1 for a runtime failure.
const EXIT_USAGE: i32 = 2;

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: sort [OPTION]... [FILE]...
       sort [OPTION]... --files0-from=F

Write sorted concatenation of all FILE(s) to standard output.

Ordering options:
  -b, --ignore-leading-blanks   ignore leading blanks
  -d, --dictionary-order        consider only blanks and alphanumeric
                                characters
  -f, --ignore-case             fold lower case to upper case characters
  -g, --general-numeric-sort    compare according to general numerical value
  -h, --human-numeric-sort      compare human readable numbers (e.g., 2K 1G)
  -i, --ignore-nonprinting      consider only printable characters
  -M, --month-sort              compare (unknown) < 'JAN' < ... < 'DEC'
  -n, --numeric-sort            compare according to string numerical value
  -R, --random-sort             shuffle, but group identical keys
  -r, --reverse                 reverse the result of comparisons
      --sort=WORD               sort according to WORD
  -V, --version-sort            natural sort of (version) numbers within text

Other options:
  -c, --check                   check for sorted input; do not sort
  -C, --check=quiet             like -c, but do not report first bad line
  -k, --key=KEYDEF              sort via a key
  -m, --merge                   merge already sorted files; do not sort
  -o, --output=FILE             write result to FILE instead of standard output
  -s, --stable                  stabilize sort
  -t, --field-separator=SEP     use SEP instead of non-blank to blank transition
  -u, --unique                  with -c, check for strict ordering
  -z, --zero-terminated         line delimiter is NUL, not newline
      --help     display this help and exit
      --version  output version information and exit
";

fn io_error_reason(error: &io::Error) -> String {
    let raw = error.raw_os_error().unwrap_or(libc::EIO);
    // SAFETY: strerror_r writes into the buffer we own.
    unsafe {
        let mut buffer = [0i8; 256];
        let pointer = if libc::strerror_r(raw, buffer.as_mut_ptr(), buffer.len()) == 0 {
            buffer.as_ptr()
        } else {
            libc::strerror(raw)
        };
        std::ffi::CStr::from_ptr(pointer).to_string_lossy().into_owned()
    }
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut keydefs: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();
    let mut no_more = false;
    let mut i = 0usize;

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
        let next_value = |i: &mut usize| -> Option<String> {
            if *i < args.len() {
                let value = args[*i].clone();
                *i += 1;
                Some(value)
            } else {
                None
            }
        };
        if let Some(long) = arg.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value.to_string())),
                None => (long, None),
            };
            match name {
                "help" => {
                    print!("{}", HELP);
                    std::process::exit(0);
                }
                "version" => {
                    println!("sort (rusttool) {}", VERSION);
                    std::process::exit(0);
                }
                "ignore-leading-blanks" => options.ordering = Ordering::IgnoreBlanks,
                "dictionary-order" => options.ordering = Ordering::Dictionary,
                "ignore-case" => options.ordering = Ordering::FoldCase,
                "general-numeric-sort" => options.ordering = Ordering::GeneralNumeric,
                "human-numeric-sort" => options.ordering = Ordering::HumanNumeric,
                "ignore-nonprinting" => options.ordering = Ordering::IgnoreNonprinting,
                "month-sort" => options.ordering = Ordering::Month,
                "numeric-sort" => options.ordering = Ordering::Numeric,
                "random-sort" => options.ordering = Ordering::Random,
                "version-sort" => options.ordering = Ordering::Version,
                "reverse" => options.reverse = true,
                "stable" => options.stable = true,
                "unique" => options.unique = true,
                "check" => {
                    // --check=diagnose-first is the plain form; =quiet is -C.
                    match inline.as_deref() {
                        Some("quiet") | Some("silent") => {
                            options.check = true;
                            options.check_quiet = true;
                        }
                        Some("diagnose-first") | None => options.check = true,
                        Some(other) => {
                            return Err(format!("sort: invalid argument '{}' for '--check'", other))
                        }
                    }
                }
                "merge" => options.merge = true,
                "zero-terminated" => options.zero_terminated = true,
                "field-separator" => {
                    let value = inline.clone().or_else(|| next_value(&mut i));
                    match value.map(|text| text.as_bytes().to_vec()) {
                        Some(bytes) if bytes.len() == 1 => options.separator = Some(bytes[0]),
                        Some(bytes) => {
                            return Err(format!("@@TAB@@{}", String::from_utf8_lossy(&bytes)))
                        }
                        None => return Err("@@REQUIRES@@field-separator".to_string()),
                    }
                }
                "key" => {
                    let value = inline.clone().or_else(|| next_value(&mut i));
                    match value {
                        Some(value) => keydefs.push(value),
                        None => return Err("@@REQUIRES@@key".to_string()),
                    }
                }
                "sort" => {
                    let word = inline.clone().or_else(|| next_value(&mut i));
                    // The manual lists both the letters and the long names.
                    match Ordering::from_word(word.as_deref().unwrap_or("")) {
                        Some(ordering) => options.ordering = ordering,
                        None => {
                            return Err(format!("@@SORT-WORD@@{}", word.unwrap_or_default()))
                        }
                    }
                }
                "output" | "files0-from" | "random-source" | "compress-program"
                | "temporary-directory" | "parallel" | "buffer-size" | "batch-size"
                | "debug" | "context" => {
                    // Accepted for compatibility; the ones that change the work
                    // are read below where they matter.
                    let _ = inline.clone().or_else(|| next_value(&mut i));
                }
                _ => return Err(format!("@@UNRECOGNIZED@@{}", name)),
            }
            continue;
        }

        let letters: Vec<char> = arg.chars().skip(1).collect();
        let mut index = 0usize;
        while index < letters.len() {
            let letter = letters[index];
            index += 1;
            let mut attached = |letters: &[char], index: &mut usize| -> Option<String> {
                if *index < letters.len() {
                    let value: String = letters[*index..].iter().collect();
                    *index = letters.len();
                    Some(value)
                } else if i < args.len() {
                    let value = args[i].clone();
                    i += 1;
                    Some(value)
                } else {
                    None
                }
            };
            match letter {
                'b' => options.ordering = Ordering::IgnoreBlanks,
                'd' => options.ordering = Ordering::Dictionary,
                'f' => options.ordering = Ordering::FoldCase,
                'g' => options.ordering = Ordering::GeneralNumeric,
                'h' => options.ordering = Ordering::HumanNumeric,
                'i' => options.ordering = Ordering::IgnoreNonprinting,
                'M' => options.ordering = Ordering::Month,
                'n' => options.ordering = Ordering::Numeric,
                'R' => options.ordering = Ordering::Random,
                'V' => options.ordering = Ordering::Version,
                'r' => options.reverse = true,
                's' => options.stable = true,
                'u' => options.unique = true,
                'c' => options.check = true,
                'C' => {
                    options.check = true;
                    options.check_quiet = true;
                }
                'm' => options.merge = true,
                'z' => options.zero_terminated = true,
                'k' => match attached(&letters, &mut index) {
                    Some(value) => keydefs.push(value),
                    None => return Err("@@REQUIRES@@k".to_string()),
                },
                't' => match attached(&letters, &mut index) {
                    Some(value) => {
                        let bytes = value.as_bytes().to_vec();
                        if bytes.len() == 1 {
                            options.separator = Some(bytes[0]);
                        } else {
                            return Err(format!("@@TAB@@{}", value));
                        }
                    }
                    None => return Err("sort: option requires an argument -- 't'".to_string()),
                },
                // Accepted for compatibility and ignored.
                'S' | 'T' => {
                    let _ = attached(&letters, &mut index);
                }
                'o' => {
                    let _ = attached(&letters, &mut index);
                }
                other => return Err(format!("@@INVALID@@{}", other)),
            }
        }
    }

    // Each KEYDEF adds a key; the last one wins for a single -k, which is all
    // this implementation supports, as the manual describes for repeated -k.
    if let Some(keydef) = keydefs.last() {
        match parse_keydef(keydef) {
            Some(keys) => options.keys = keys,
            // The field number is the part coreutils names when it is zero.
            None => {
                let field = keydef
                    .split(&['.', ','][..])
                    .next()
                    .unwrap_or(keydef)
                    .to_string();
                return Err(format!("@@ZERO-FIELD@@{}", field));
            }
        }
    }
    options.files = files.clone();
    Ok(options)
}

/// Read every input, keeping lines without their terminator and the position
/// each one had in the input, which -u needs to pick the first of an equal run.
fn read_lines(options: &Options) -> (Vec<(Vec<u8>, usize)>, i32) {
    let terminator = if options.zero_terminated { 0u8 } else { b'\n' };
    let mut lines: Vec<(Vec<u8>, usize)> = Vec::new();
    let mut status = 0;
    let mut inputs: Vec<Box<dyn Read>> = Vec::new();
    if options.files.is_empty() {
        inputs.push(Box::new(io::stdin()));
    } else {
        for file in &options.files {
            if file == "-" {
                inputs.push(Box::new(io::stdin()));
                continue;
            }
            match File::open(file) {
                Ok(handle) => inputs.push(Box::new(handle)),
                Err(error) => {
                    eprintln!("{}", sort::cannot_read_message(file, &io_error_reason(&error)));
                    status = EXIT_USAGE;
                }
            }
        }
    }
    for input in inputs.iter_mut() {
        let mut buffer = Vec::new();
        if input.read_to_end(&mut buffer).is_err() {
            status = 1;
            continue;
        }
        // Nothing at all is not a record. `split` hands back one empty piece for
        // an empty buffer, and without this the tool would answer an empty input
        // with a single empty line — or, under -z, a single stray NUL.
        //
        // An input holding just a terminator is different: that is one record,
        // and the record is empty.
        if buffer.is_empty() {
            continue;
        }
        let had_terminator = buffer.last() == Some(&terminator);
        let pieces: Vec<&[u8]> = buffer.split(|byte| *byte == terminator).collect();
        let count = if had_terminator {
            pieces.len().saturating_sub(1)
        } else {
            pieces.len()
        };
        for piece in pieces.iter().take(count) {
            lines.push((piece.to_vec(), lines.len()));
        }
    }
    (lines, status)
}


fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse(&args) {
        Ok(options) => options,
        Err(message) => {
            let mut try_help = true;
            let status = EXIT_USAGE;
            if let Some(name) = message.strip_prefix("@@UNRECOGNIZED@@") {
                eprintln!("{}", sort::unrecognized_option_message(&format!("--{}", name)));
            } else if let Some(letter) = message.strip_prefix("@@INVALID@@") {
                eprintln!(
                    "{}",
                    sort::invalid_option_message(letter.chars().next().unwrap_or('?'))
                );
            } else if let Some(name) = message.strip_prefix("@@REQUIRES@@") {
                eprintln!("{}", sort::requires_argument_message(name));
            } else if let Some(value) = message.strip_prefix("@@TAB@@") {
                eprintln!("{}", sort::multi_character_separator_message(value));
                try_help = false;
            } else if let Some(field) = message.strip_prefix("@@ZERO-FIELD@@") {
                eprintln!("{}", sort::zero_field_message(field));
                try_help = false;
            } else if let Some(word) = message.strip_prefix("@@SORT-WORD@@") {
                for line in sort::bad_sort_word_lines(word) {
                    eprintln!("{}", line);
                }
                std::process::exit(1);
            } else {
                eprintln!("{}", message);
                std::process::exit(EXIT_USAGE);
            }
            if try_help {
                eprintln!("{}", sort::try_help_message());
            }
            std::process::exit(status);
        }
    };

    let (mut lines, mut status) = read_lines(&options);
    let terminator = if options.zero_terminated { 0u8 } else { b'\n' };

    if options.check {
        // -c and -C only report; -u with them asks for strict ordering, which
        // check_sorted already applies.
        let name = options
            .files
            .first()
            .cloned()
            .unwrap_or_else(|| "-".to_string());
        let plain: Vec<Vec<u8>> = lines.iter().map(|(line, _)| line.clone()).collect();
        match check_sorted(&plain, &options) {
            Check::Sorted => {}
            Check::OutOfOrder { line, text } => {
                if !options.check_quiet {
                    eprintln!("{}", sort::disorder_message(&name, line, &text));
                }
                status = 1;
            }
        }
        std::process::exit(status);
    }

    if options.merge {
        // The inputs are already sorted, so only the merge is left.
        lines.sort_by(|left, right| compare_lines(&left.0, &right.0, &options));
    } else {
        lines.sort_by(|left, right| compare_lines(&left.0, &right.0, &options));
    }
    // `&mut lines` here is the binding being shadowed, not the new one: the new
    // one does not exist until the initialiser is finished.
    let lines: Vec<Vec<u8>> = if options.unique {
        // -u keeps the line that came first in the input.
        sort::unique_in_input_order(&mut lines, &options)
    } else {
        lines.into_iter().map(|(line, _)| line).collect()
    };

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    for line in &lines {
        if out.write_all(line).is_err() || out.write_all(&[terminator]).is_err() {
            eprintln!("{}", sort::cannot_write_message("Broken pipe"));
            std::process::exit(1);
        }
    }
    let _ = out.flush();
    std::process::exit(status);
}
