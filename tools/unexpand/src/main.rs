//! `unexpand(1)`: convert spaces to tabs.

use std::fs::File;
use std::io::{self, Read, Write};

use unexpand::{Scope, TabStops};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: unexpand [OPTION]... [FILE]...

Convert blanks in each FILE to tabs, writing to standard output.

With no FILE, or when FILE is -, read standard input.

Mandatory arguments to long options are mandatory for short options too.

  -a, --all             convert all blanks, instead of just initial blanks
      --first-only      convert only leading sequences of blanks (overrides -a)
  -t, --tabs=N          have tabs N characters apart instead of 8 (enables -a)
      --help     display this help and exit
      --version  output version information and exit

A tab stop list may end with /TABS to keep tabs TABS columns apart after the
last explicitly listed stop, or +TABS to align the remaining stops relative to
the last one.  A specifier must come before the number it applies to.
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

/// How a run ended.
enum Failure {
    /// The message is already on stderr, no Try line.
    Plain,
    /// The message comes first and the Try line follows.
    Usage(String),
}

/// Hand a `-t` value to the stop list, reporting every complaint it draws.
fn take_stops(stops: &mut TabStops, value: &str) -> Result<(), Failure> {
    let errors = stops.parse(value);
    if errors.is_empty() {
        return Ok(());
    }
    for error in &errors {
        eprintln!("{}", error.message());
    }
    Err(Failure::Plain)
}

fn parse(args: &[String]) -> Result<(TabStops, Scope, Vec<String>), Failure> {
    let mut stops = TabStops {
        size: 0,
        ..TabStops::default()
    };
    // `-a` and the `-t` that implies it, both overridable by --first-only.
    let mut convert_entire_line = false;
    let mut first_only = false;
    let mut files: Vec<String> = Vec::new();
    let mut no_more = false;
    let mut index = 0usize;

    while index < args.len() {
        let arg = args[index].clone();
        index += 1;
        if no_more || arg == "-" || !arg.starts_with('-') {
            files.push(arg);
            continue;
        }
        if arg == "--" {
            no_more = true;
            continue;
        }

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
                    println!("unexpand (rusttool) {}", VERSION);
                    std::process::exit(0);
                }
                "all" => convert_entire_line = true,
                "first-only" => first_only = true,
                "tabs" => {
                    let value = match inline {
                        Some(value) => value,
                        None => match args.get(index) {
                            Some(value) => {
                                index += 1;
                                value.clone()
                            }
                            None => {
                                return Err(Failure::Usage(
                                    unexpand::requires_argument_message(name),
                                ))
                            }
                        },
                    };
                    // -t turns -a on by itself.
                    convert_entire_line = true;
                    take_stops(&mut stops, &value)?;
                }
                _ => {
                    return Err(Failure::Usage(unexpand::unrecognized_option_message(name)))
                }
            }
            continue;
        }

        // The short options are ",0123456789at:": a comma or a digit is an
        // option of its own, so -8 means eight columns and -2,6 is a list.
        let letters: Vec<char> = arg.chars().skip(1).collect();
        let mut position = 0usize;
        while position < letters.len() {
            let letter = letters[position];
            position += 1;
            match letter {
                'a' => convert_entire_line = true,
                't' => {
                    let value = if position < letters.len() {
                        let value: String = letters[position..].iter().collect();
                        position = letters.len();
                        Some(value)
                    } else {
                        match args.get(index) {
                            Some(value) => {
                                index += 1;
                                Some(value.clone())
                            }
                            None => None,
                        }
                    };
                    match value {
                        Some(value) => {
                            convert_entire_line = true;
                            take_stops(&mut stops, &value)?;
                        }
                        None => {
                            return Err(Failure::Usage(
                                unexpand::short_requires_argument_message('t'),
                            ))
                        }
                    }
                }
                // A digit starts a number, and a comma closes it. The rest of
                // the bundle is the number, which is why -8x is the list "8x".
                ',' | '0'..='9' => {
                    let mut text = String::new();
                    if letter != ',' {
                        text.push(letter);
                    }
                    while position < letters.len() {
                        text.push(letters[position]);
                        position += 1;
                    }
                    take_stops(&mut stops, &text)?;
                }
                other => {
                    return Err(Failure::Usage(unexpand::invalid_option_message(other)))
                }
            }
        }
    }

    let scope = if first_only {
        Scope::FirstOnly
    } else if convert_entire_line {
        Scope::All
    } else {
        Scope::FirstOnly
    };
    if let Err(message) = stops.finalize() {
        eprintln!("{}", message);
        return Err(Failure::Plain);
    }
    Ok((stops, scope, files))
}

/// Read one operand, with `-` and a missing list both meaning standard input.
fn slurp(name: Option<&String>) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let read = match name {
        None => io::stdin().read_to_end(&mut bytes),
        Some(name) if name == "-" => io::stdin().read_to_end(&mut bytes),
        Some(name) => match File::open(name) {
            Ok(mut handle) => handle.read_to_end(&mut bytes),
            Err(error) => return Err(io_error_reason(&error)),
        },
    };
    match read {
        Ok(_) => Ok(bytes),
        Err(error) => Err(io_error_reason(&error)),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (stops, scope, files) = match parse(&args) {
        Ok(parsed) => parsed,
        Err(Failure::Plain) => std::process::exit(1),
        Err(Failure::Usage(message)) => {
            eprintln!("{}", message);
            eprintln!("{}", unexpand::try_help_message());
            std::process::exit(1);
        }
    };

    let mut status = 0;
    let mut buffer: Vec<u8> = Vec::new();
    match files.first() {
        None => {
            let bytes = slurp(None).unwrap_or_default();
            buffer.extend_from_slice(&bytes);
        }
        Some(_) => {
            // The operands form one character stream, because unexpand.c only
            // reaches for the next file from inside its line loop.
            for name in &files {
                match slurp(Some(name)) {
                    Ok(bytes) => buffer.extend_from_slice(&bytes),
                    Err(reason) => {
                        eprintln!("{}", unexpand::cannot_open_message(name, &reason));
                        status = 1;
                    }
                }
            }
        }
    }

    let mut out: Vec<u8> = Vec::new();
    unexpand::unexpand(&buffer, &stops, scope, &mut out);
    let stdout = io::stdout();
    let mut handle = io::BufWriter::new(stdout.lock());
    if handle.write_all(&out).is_err() || handle.flush().is_err() {
        eprintln!("unexpand: write error");
        status = 1;
    }
    std::process::exit(status);
}
