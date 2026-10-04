//! `expand(1)`: convert tabs to spaces.

use std::fs::File;
use std::io::{self, Read, Write};

use expand::TabStops;

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: expand [OPTION]... [FILE]...

Convert tabs in each FILE to spaces, writing to standard output.

With no FILE, or when FILE is -, read standard input.

Mandatory arguments to long options are mandatory for short options too.

  -i, --initial    do not convert tabs after non blanks
  -t, --tabs=N     have tabs N characters apart, not 8
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

fn parse(args: &[String]) -> Result<(TabStops, bool, Vec<String>), Failure> {
    let mut stops = TabStops {
        // Zero until finalize works out what the list means.
        size: 0,
        ..TabStops::default()
    };
    let mut initial_only = false;
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
                    println!("expand (rusttool) {}", VERSION);
                    std::process::exit(0);
                }
                "initial" => initial_only = true,
                "tabs" => {
                    let value = match inline {
                        Some(value) => value,
                        None => match args.get(index) {
                            Some(value) => {
                                index += 1;
                                value.clone()
                            }
                            None => {
                                return Err(Failure::Usage(expand::requires_argument_message(name)))
                            }
                        },
                    };
                    take_stops(&mut stops, &value)?;
                }
                _ => return Err(Failure::Usage(expand::unrecognized_option_message(name))),
            }
            continue;
        }

        // The short options are "it:0::1::...9::": a digit takes an *optional*
        // argument, so -8 means eight columns and -8x means the stop list "x".
        let letters: Vec<char> = arg.chars().skip(1).collect();
        let mut position = 0usize;
        while position < letters.len() {
            let letter = letters[position];
            position += 1;
            match letter {
                'i' => initial_only = true,
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
                        Some(value) => take_stops(&mut stops, &value)?,
                        None => {
                            return Err(Failure::Usage(expand::short_requires_argument_message(
                                't',
                            )))
                        }
                    }
                }
                digit if digit.is_ascii_digit() => {
                    // The rest of the bundle is this digit's optional argument.
                    let value = if position < letters.len() {
                        let value: String = letters[position..].iter().collect();
                        position = letters.len();
                        value
                    } else {
                        digit.to_string()
                    };
                    take_stops(&mut stops, &value)?;
                }
                other => return Err(Failure::Usage(expand::invalid_option_message(other))),
            }
        }
    }

    if let Err(message) = stops.finalize() {
        eprintln!("{}", message);
        return Err(Failure::Plain);
    }
    Ok((stops, initial_only, files))
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
    let (stops, initial_only, files) = match parse(&args) {
        Ok(parsed) => parsed,
        Err(Failure::Plain) => std::process::exit(1),
        Err(Failure::Usage(message)) => {
            eprintln!("{}", message);
            eprintln!("{}", expand::try_help_message());
            std::process::exit(1);
        }
    };

    let stdout = io::stdout();
    let mut status = 0;
    let mut buffer: Vec<u8> = Vec::new();

    match files.first() {
        None => {
            let bytes = slurp(None).unwrap_or_default();
            buffer.extend_from_slice(&bytes);
        }
        Some(_) => {
            // The operands form one character stream, because expand.c only
            // reaches for the next file from inside its line loop.
            for name in &files {
                match slurp(Some(name)) {
                    Ok(bytes) => buffer.extend_from_slice(&bytes),
                    Err(reason) => {
                        eprintln!("{}", expand::cannot_open_message(name, &reason));
                        status = 1;
                    }
                }
            }
        }
    }
    let mut expanded: Vec<u8> = Vec::new();
    expand::expand(&buffer, &stops, initial_only, &mut expanded);
    let mut out = io::BufWriter::new(stdout.lock());
    if out.write_all(&expanded).is_err() || out.flush().is_err() {
        eprintln!("expand: write error");
        status = 1;
    }
    std::process::exit(status);
}
