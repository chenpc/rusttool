//! `cut(1)`: remove sections from each line of files.

use std::fs::File;
use std::io::{self, Read, Write};

use cut::{join, line_terminator, parse_list, select, Line, Options, Problem, Range, Unit};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: cut OPTION... [FILE]...

Print selected parts of lines from each FILE to standard output.

  -b, --bytes=LIST           select only these bytes
  -c, --characters=LIST      select only these characters
  -d, --delimiter=DELIM      use DELIM instead of TAB for field delimiter
  -f, --fields=LIST          select only these fields
      --complement           complement the set of selected bytes, characters
                             or fields
  -n                         (ignored)
      --output-delimiter=STRING
                             use STRING as the output delimiter
  -s, --only-delimited       do not print lines not containing delimiters
  -z, --zero-terminated      line delimiter is NUL, not newline
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

fn message_for(problem: Problem) -> String {
    match problem {
        Problem::DecreasingRange => cut::decreasing_range_message(),
        Problem::BadRange(unit) => cut::bad_range_message(unit),
        Problem::BadValue(value, unit) => cut::bad_value_message(&value, unit),
        Problem::RangeWithNoEndpoint(text) => cut::range_with_no_endpoint_message(&text),
        Problem::NumberedFromOne(unit) => cut::numbered_from_one_message(unit),
        Problem::NoUnit => cut::missing_unit_message(),
        Problem::SeveralUnits => cut::several_units_message(),
        Problem::BadDelimiter => cut::bad_delimiter_message(),
    }
}

/// The parsed command line: the options, the one LIST and the file operands.
struct Parsed {
    options: Options,
    list: String,
    files: Vec<String>,
}

fn parse(args: &[String]) -> Result<Parsed, String> {
    let mut options = Options::default();
    let mut lists: Vec<(Unit, String)> = Vec::new();
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
        // A value may be attached (--fields=1) or be the next argument.
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
                    println!("cut (rusttool) {}", VERSION);
                    std::process::exit(0);
                }
                "bytes" | "characters" | "fields" => {
                    let unit = match name {
                        "bytes" => Unit::Bytes,
                        "characters" => Unit::Characters,
                        _ => Unit::Fields,
                    };
                    match inline.clone().or_else(|| next_value(&mut i)) {
                        Some(list) => lists.push((unit, list)),
                        None => return Err(format!("@@REQUIRES@@{}", name)),
                    }
                }
                "delimiter" => match inline.clone().or_else(|| next_value(&mut i)) {
                    Some(value) => {
                        let bytes = value.as_bytes().to_vec();
                        if bytes.len() == 1 {
                            options.delimiter = Some(bytes[0]);
                        } else {
                            return Err("@@DELIMITER@@".to_string());
                        }
                    }
                    None => return Err("@@REQUIRES@@d".to_string()),
                },
                "output-delimiter" => match inline.clone().or_else(|| next_value(&mut i)) {
                    Some(value) => options.output_delimiter = Some(value.into_bytes()),
                    None => {
                        return Err("cut: option '--output-delimiter' requires an argument".to_string())
                    }
                },
                "complement" => options.complement = true,
                "only-delimited" => options.only_delimited = true,
                "zero-terminated" => options.zero_terminated = true,
                _ => return Err(format!("@@UNRECOGNIZED@@{}", name)),
            }
            continue;
        }

        // Short options, where a value may be attached to the letter.
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
                'b' | 'c' | 'f' => {
                    let unit = match letter {
                        'b' => Unit::Bytes,
                        'c' => Unit::Characters,
                        _ => Unit::Fields,
                    };
                    match attached(&letters, &mut index) {
                        Some(list) => lists.push((unit, list)),
                        None => return Err(format!("@@REQUIRES@@{}", letter)),
                    }
                }
                'd' => match attached(&letters, &mut index) {
                    Some(value) => {
                        let bytes = value.as_bytes().to_vec();
                        if bytes.len() == 1 {
                            options.delimiter = Some(bytes[0]);
                        } else {
                            return Err("@@DELIMITER@@".to_string());
                        }
                    }
                    None => return Err("cut: option requires an argument -- 'd'".to_string()),
                },
                's' => options.only_delimited = true,
                'z' => options.zero_terminated = true,
                // -n is accepted and ignored, as the manual says.
                'n' => {}
                other => return Err(format!("@@INVALID@@{}", other)),
            }
        }
    }

    // The manual allows one, and only one, of -b, -c and -f.
    if lists.len() > 1 {
        return Err("@@SEVERAL@@".to_string());
    }
    let (unit, list) = match lists.into_iter().next() {
        Some(found) => found,
        None => return Err("@@NO-UNIT@@".to_string()),
    };

    options.unit = Some(unit);
    options.files = files.clone();
    Ok(Parsed {
        options,
        list,
        files,
    })
}

/// Write the selected parts of every line in `buffer`.
fn write_selected(
    buffer: &[u8],
    ranges: &[Range],
    options: &Options,
    out: &mut dyn Write,
) -> io::Result<()> {
    let terminator = line_terminator(options);
    if buffer.is_empty() {
        // Nothing in, nothing out: not even an empty line.
        return Ok(());
    }
    // A file that does not end in a terminator still has its last line printed,
    // so the split only drops the empty piece a trailing terminator produces.
    let had_terminator = buffer.last() == Some(&terminator);
    let pieces: Vec<&[u8]> = buffer.split(|byte| *byte == terminator).collect();
    let count = if had_terminator {
        pieces.len().saturating_sub(1)
    } else {
        pieces.len()
    };
    for line in pieces.iter().take(count) {
        match select(line, ranges, options) {
            Line::Selected(parts) => {
                out.write_all(&join(&parts, options))?;
                out.write_all(&[terminator])?;
            }
            Line::Undelimited(bytes) => {
                // The manual says such a line is printed as it is, delimiter
                // and all.
                out.write_all(&bytes)?;
                if bytes.last() != Some(&terminator) {
                    out.write_all(&[terminator])?;
                }
            }
            Line::Skip => {}
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match parse(&args) {
        Ok(parsed) => parsed,
        Err(message) => {
            // coreutils prints the diagnostic and the Try line.
            if let Some(name) = message.strip_prefix("@@UNRECOGNIZED@@") {
                eprintln!("{}", cut::unrecognized_option_message(&format!("--{}", name)));
            } else if let Some(letter) = message.strip_prefix("@@INVALID@@") {
                eprintln!(
                    "{}",
                    cut::invalid_option_message(letter.chars().next().unwrap_or('?'))
                );
            } else if let Some(name) = message.strip_prefix("@@REQUIRES@@") {
                let letter = name.chars().next().unwrap_or('?');
                eprintln!("{}", cut::requires_argument_message(letter));
            } else if message == "@@SEVERAL@@" {
                eprintln!("{}", cut::several_units_message());
            } else if message == "@@NO-UNIT@@" {
                eprintln!("{}", cut::missing_unit_message());
            } else if message == "@@DELIMITER@@" {
                eprintln!("{}", cut::bad_delimiter_message());
            } else {
                eprintln!("{}", message);
                std::process::exit(1);
            }
            eprintln!("{}", cut::try_help_message());
            std::process::exit(1);
        }
    };
    let Parsed {
        options,
        list,
        files,
    } = parsed;
    let unit = options.unit.unwrap_or(Unit::Fields);
    let ranges = match parse_list(&list, unit) {
        Ok(ranges) => ranges,
        Err(problem) => {
            eprintln!("{}", message_for(problem));
            eprintln!("{}", cut::try_help_message());
            std::process::exit(1);
        }
    };

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let mut status = 0;

    // With no FILE, or with -, standard input is read.
    let mut inputs: Vec<Box<dyn Read>> = Vec::new();
    if files.is_empty() {
        inputs.push(Box::new(io::stdin()));
    } else {
        for file in &files {
            if file == "-" {
                inputs.push(Box::new(io::stdin()));
                continue;
            }
            match File::open(file) {
                Ok(handle) => inputs.push(Box::new(handle)),
                Err(error) => {
                    eprintln!("{}", cut::cannot_open_message(file, &io_error_reason(&error)));
                    status = 1;
                }
            }
        }
    }

    for input in inputs.iter_mut() {
        let mut buffer = Vec::new();
        if let Err(error) = input.read_to_end(&mut buffer) {
            eprintln!("cut: read error: {}", io_error_reason(&error));
            status = 1;
            continue;
        }
        if write_selected(&buffer, &ranges, &options, &mut out).is_err() {
            status = 1;
        }
    }
    let _ = out.flush();
    std::process::exit(status);
}