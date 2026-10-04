//! `uniq(1)`: report or omit repeated lines.

use std::fs::File;
use std::io::{self, Read, Write};

use uniq::{count_text, decide, group, terminator, Action, Group, GroupMethod, Options};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: uniq [OPTION]... [INPUT [OUTPUT]]

Filter adjacent matching lines from INPUT (or standard input), writing to OUTPUT
(or standard output).

  -c, --count             prefix lines by the number of occurrences
  -d, --repeated          only print duplicate lines, one for each group
  -D                      print all duplicate lines
      --all-repeated[=METHOD]
                          like -D, but allow separating groups with an empty
                          line; METHOD={none(default),prepend,separate}
  -f, --skip-fields=N     avoid comparing the first N fields
      --group[=METHOD]    show all items, separating groups with an empty line
  -i, --ignore-case       ignore differences in case when comparing
  -s, --skip-chars=N      avoid comparing the first N characters
  -u, --unique            only print unique lines
  -w, --check-chars=N     compare no more than N characters in lines
  -z, --zero-terminated   line delimiter is NUL, not newline
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

/// Parse the non-negative number -f, -s and -w take.
fn number(text: &str) -> Result<usize, String> {
    // A negative value is not a count, and neither is junk.
    if text.starts_with('-') || text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(text.to_string());
    }
    text.parse::<usize>().map_err(|_| text.to_string())
}

fn parse(args: &[String]) -> Result<(Options, Vec<String>), String> {
    let mut options = Options::default();
    let mut operands: Vec<String> = Vec::new();
    let mut no_more = false;
    let mut i = 0usize;

    while i < args.len() {
        let arg = args[i].clone();
        i += 1;
        if no_more || arg == "-" || !arg.starts_with('-') {
            operands.push(arg);
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
                    println!("uniq (rusttool) {}", VERSION);
                    std::process::exit(0);
                }
                "count" => options.count = true,
                "repeated" => options.repeated = true,
                "unique" => options.unique = true,
                "ignore-case" => options.compare.ignore_case = true,
                "zero-terminated" => options.zero_terminated = true,
                // The manual gives both options a default: none for
                // --all-repeated and separate for --group.
                "all-repeated" => {
                    let value = inline.clone().unwrap_or_default();
                    options.all_repeated_method = Some(match GroupMethod::parse(&value) {
                        Some(method) => method,
                        None => return Err(format!("@@METHOD@@{}\u{1}--all-repeated", value)),
                    })
                }
                "group" => {
                    let value = match &inline {
                        Some(value) => value.clone(),
                        None => "separate".to_string(),
                    };
                    options.group = Some(match GroupMethod::parse(&value) {
                        Some(method) => method,
                        None => return Err(format!("@@METHOD@@{}\u{1}--group", value)),
                    })
                }
                "skip-fields" => {
                    let value = inline.clone().or_else(|| next_value(&mut i));
                    match value.as_deref().map(number) {
                        Some(Ok(count)) => options.compare.skip_fields = count,
                        _ => return Err(format!("@@SKIP@@{}", value.unwrap_or_default())),
                    }
                }
                "skip-chars" => {
                    let value = inline.clone().or_else(|| next_value(&mut i));
                    match value.as_deref().map(number) {
                        Some(Ok(count)) => options.compare.skip_chars = count,
                        _ => {
                            return Err(uniq::invalid_check_chars_message(&value.unwrap_or_default()))
                        }
                    }
                }
                "check-chars" => {
                    let value = inline.clone().or_else(|| next_value(&mut i));
                    match value.as_deref().map(number) {
                        Some(Ok(count)) => options.compare.check_chars = Some(count),
                        _ => return Err(format!("@@CHECK@@{}", value.unwrap_or_default())),
                    }
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
                'c' => options.count = true,
                'd' => options.repeated = true,
                'D' => options.all_repeated = true,
                'u' => options.unique = true,
                'i' => options.compare.ignore_case = true,
                'z' => options.zero_terminated = true,
                'f' | 's' | 'w' => match attached(&letters, &mut index) {
                    Some(value) => match number(&value) {
                        Ok(count) => match letter {
                            'f' => options.compare.skip_fields = count,
                            's' => options.compare.skip_chars = count,
                            _ => options.compare.check_chars = Some(count),
                        },
                        Err(_) => return Err(format!("@@SKIP@@{}", value)),
                    },
                    None => {
                        return Err(format!("uniq: option requires an argument -- '{}'", letter))
                    }
                },
                other => return Err(format!("@@INVALID@@{}", other)),
            }
        }
    }

    // The manual has one input and one output at most.
    if let Some(extra) = operands.get(2) {
        return Err(format!("@@EXTRA@@{}", extra));
    }
    Ok((options, operands))
}

/// Print one group's lines.
fn print_group(
    entry: &Group<'_>,
    action: Action,
    options: &Options,
    out: &mut dyn Write,
) -> io::Result<()> {
    let terminator = terminator(options);
    match action {
        Action::Nothing => Ok(()),
        Action::Line => {
            out.write_all(entry.first)?;
            out.write_all(&[terminator])
        }
        Action::Counted => {
            out.write_all(count_text(entry.count).as_bytes())?;
            out.write_all(entry.first)?;
            out.write_all(&[terminator])
        }
        Action::AllLines => {
            for line in &entry.lines {
                out.write_all(line)?;
                out.write_all(&[terminator])?;
            }
            Ok(())
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (options, operands) = match parse(&args) {
        Ok(parsed) => parsed,
        Err(message) => {
            // coreutils prints the diagnostic and, for a usage error, the Try
            // line instead of the whole help.
            let mut try_help = true;
            if let Some(name) = message.strip_prefix("@@UNRECOGNIZED@@") {
                eprintln!("{}", uniq::unrecognized_option_message(&format!("--{}", name)));
            } else if let Some(letter) = message.strip_prefix("@@INVALID@@") {
                eprintln!(
                    "{}",
                    uniq::invalid_option_message(letter.chars().next().unwrap_or('?'))
                );
            } else if let Some(text) = message.strip_prefix("@@REQUIRES@@") {
                eprintln!(
                    "{}",
                    uniq::requires_argument_message(text.chars().next().unwrap_or('?'))
                );
            } else if let Some(text) = message.strip_prefix("@@EXTRA@@") {
                eprintln!("{}", uniq::too_many_operands_message(text));
            } else if let Some(rest) = message.strip_prefix("@@METHOD@@") {
                let (value, option) = rest.split_once('\u{1}').unwrap_or((rest, "--group"));
                for line in uniq::invalid_method_lines(value, option) {
                    eprintln!("{}", line);
                }
            } else if let Some(text) = message.strip_prefix("@@SKIP@@") {
                eprintln!("{}", uniq::invalid_skip_message(text));
                try_help = false;
            } else if let Some(text) = message.strip_prefix("@@CHECK@@") {
                eprintln!("{}", uniq::invalid_check_chars_message(text));
                try_help = false;
            } else {
                eprintln!("{}", message);
                try_help = false;
            }
            if try_help {
                eprintln!("{}", uniq::try_help_message());
            }
            std::process::exit(1);
        }
    };

    // Read the input, with the terminator the option asks for.
    let terminator = terminator(&options);
    let mut buffer = Vec::new();
    let mut status = 0;
    let input_name = operands.first().map(|name| name.as_str());
    match input_name {
        None | Some("-") => {
            if let Err(error) = io::stdin().read_to_end(&mut buffer) {
                eprintln!("uniq: read error: {}", io_error_reason(&error));
                std::process::exit(1);
            }
        }
        Some(name) => match File::open(name) {
            Ok(mut handle) => {
                if let Err(error) = handle.read_to_end(&mut buffer) {
                    eprintln!("{}", uniq::cannot_open_message(name, &io_error_reason(&error)));
                    std::process::exit(1);
                }
            }
            Err(error) => {
                eprintln!("{}", uniq::cannot_open_message(name, &io_error_reason(&error)));
                std::process::exit(1);
            }
        },
    }
    // Nothing at all is not a record. `split` hands back one empty piece for an
    // empty buffer, and without this an empty input would come out as a single
    // empty line — or, under -c, a count of one.
    //
    // An input holding just a terminator is different: that is one record, and
    // the record is empty.
    let lines: Vec<&[u8]> = if buffer.is_empty() {
        Vec::new()
    } else {
        let had_terminator = buffer.last() == Some(&terminator);
        let pieces: Vec<&[u8]> = buffer.split(|byte| *byte == terminator).collect();
        let count = if had_terminator {
            pieces.len().saturating_sub(1)
        } else {
            pieces.len()
        };
        pieces.iter().take(count).copied().collect()
    };

    // Write to the output file when one was named.
    let stdout = io::stdout();
    let mut out: Box<dyn Write> = match operands.get(1) {
        Some(name) => match File::create(name) {
            Ok(handle) => Box::new(handle),
            Err(error) => {
                eprintln!("{}", uniq::cannot_write_message(name, &io_error_reason(&error)));
                std::process::exit(1);
            }
        },
        None => Box::new(io::BufWriter::new(stdout.lock())),
    };

    // -c with -D or --all-repeated would print a count per line of a group,
    // which the manual's option list refuses.
    if options.count && (options.all_repeated || options.all_repeated_method.is_some()) {
        eprintln!("{}", uniq::count_with_all_repeated_message());
        eprintln!("{}", uniq::try_help_message());
        std::process::exit(1);
    }

    let groups = group(&lines, &options.compare);
    let method = options.group_method_check();
    let printed: Vec<bool> = groups
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let action = decide(index, entry, &options);
            let prints = action != Action::Nothing;
            if prints {
                match method {
                    Some(method) => {
                        if index > 0 && method.separates() {
                            let _ = out.write_all(&[terminator]);
                        } else if index == 0 && method.prepends() {
                            let _ = out.write_all(&[terminator]);
                        }
                    }
                    None => {}
                }
                if print_group(entry, action, &options, &mut out).is_err() {
                    status = 1;
                }
            }
            prints
        })
        .collect();

    // --append and --both leave an empty line after the last group.
    if let Some(method) = method {
        if method.appends() && printed.iter().any(|printed| *printed) {
            let _ = out.write_all(&[terminator]);
        }
    }
    let _ = out.flush();
    std::process::exit(status);
}