//! `paste(1)`: merge lines of files.

use std::fs::File;
use std::io::{self, Read, Write};
use std::cell::RefCell;
use std::rc::Rc;

use paste::{Input, Mode, Options};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: paste [OPTION]... [FILE]...

Write lines consisting of the sequentially corresponding lines from
each FILE, separated by TABs, to standard output.

With no FILE, or when FILE is -, read standard input.

Mandatory arguments to long options are mandatory for short options too.

  -d, --delimiters=LIST  reuse characters from LIST instead of TABs
  -s, --serial           paste one file at a time instead of in parallel
  -z, --zero-terminated  line delimiter is NUL, not newline
      --help     display this help and exit
      --version  output version information and exit

With -s, GNU coreutils pastes the lines of each file one after another, while
without -s it joins the first lines of every file together.

The delimiters in LIST are used cyclically, restarting at the front of the list
on every output line. A backslash escapes the character after it, and \\0 means
no delimiter at all for that position.
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
    /// The message is already on stderr and the program stops, no Try line.
    Plain,
    /// The message comes first and the Try line follows.
    Usage(String),
}

fn parse(args: &[String]) -> Result<Options, Failure> {
    let mut options = Options::default();
    let mut files: Vec<String> = Vec::new();
    let mut argument: Option<Vec<u8>> = None;
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
                    println!("paste (rusttool) {}", VERSION);
                    std::process::exit(0);
                }
                "serial" => options.mode = Mode::Serial,
                "zero-terminated" => options.terminator = 0,
                "delimiters" => match inline {
                    Some(value) => argument = Some(value.into_bytes()),
                    None => match args.get(index) {
                        Some(value) => {
                            index += 1;
                            argument = Some(value.clone().into_bytes());
                        }
                        None => {
                            return Err(Failure::Usage(paste::long_requires_argument_message(
                                name,
                            )))
                        }
                    },
                },
                _ => return Err(Failure::Usage(paste::unrecognized_option_message(name))),
            }
            continue;
        }

        // Short options are one bundle; -d takes the rest of it, or the next
        // argument when the bundle ends right after the letter.
        let letters: Vec<char> = arg.chars().skip(1).collect();
        let mut position = 0usize;
        while position < letters.len() {
            let letter = letters[position];
            position += 1;
            match letter {
                's' => options.mode = Mode::Serial,
                'z' => options.terminator = 0,
                'd' => {
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
                        Some(value) => argument = Some(value.into_bytes()),
                        None => {
                            return Err(Failure::Usage(paste::requires_argument_message(letter)))
                        }
                    }
                }
                other => return Err(Failure::Usage(paste::invalid_option_message(other))),
            }
        }
    }

    // coreutils collapses the escapes once, before any file is opened, so a
    // bad list is fatal even when every file is missing too.
    options.delimiters = match argument {
        Some(value) => match Options::from_argument(&value) {
            Ok(list) => list,
            Err(_) => {
                eprintln!("{}", paste::trailing_backslash_message(&value));
                return Err(Failure::Plain);
            }
        },
        None => options.delimiters,
    };
    // With no operand at all the manual says read standard input, so the single
    // column case has to name it too.
    if files.is_empty() {
        files.push("-".to_string());
    }
    options.files = files;
    Ok(options)
}

/// Read a whole file, or report why it could not be read.
fn slurp(name: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    match File::open(name) {
        Ok(mut handle) => match handle.read_to_end(&mut bytes) {
            Ok(_) => Ok(bytes),
            Err(error) => Err(io_error_reason(&error)),
        },
        Err(error) => Err(io_error_reason(&error)),
    }
}

/// An operand for standard input, reusing the cursor the first `-` made.
///
/// Reading the stream once and sharing the cursor is what makes `paste - -` take
/// turns on it, the way coreutils does by passing the same `FILE *` twice.
fn open_stdin(
    cache: &mut Option<Rc<RefCell<paste::Cursor>>>,
    name: &str,
) -> Input {
    if let Some(cursor) = cache {
        return Input::sharing(name.to_string(), Rc::clone(cursor));
    }
    let mut bytes = Vec::new();
    let _ = io::stdin().read_to_end(&mut bytes);
    let cursor = Rc::new(RefCell::new(paste::Cursor::new(bytes)));
    *cache = Some(Rc::clone(&cursor));
    Input::sharing(name.to_string(), cursor)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse(&args) {
        Ok(options) => options,
        Err(Failure::Plain) => std::process::exit(1),
        Err(Failure::Usage(message)) => {
            eprintln!("{}", message);
            eprintln!("{}", paste::try_help_message());
            std::process::exit(1);
        }
    };

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let mut status = 0;
    let mut buffer: Vec<u8> = Vec::new();

    // Every `-` is the same stream, so the bytes are read once and the cursor
    // is shared: the columns then take turns on it, exactly as one `FILE *`
    // would.
    let mut stdin_cursor: Option<Rc<RefCell<paste::Cursor>>> = None;

    if options.mode == Mode::Serial {
        // Serial mode walks the files one at a time, so a file it cannot open
        // is only worth a warning: the rest still gets pasted.
        for name in &options.files {
            if name == "-" {
                let mut input = open_stdin(&mut stdin_cursor, name);
                let bytes = input.drain();
                paste::serial(&bytes, &options, &mut buffer);
                continue;
            }
            match slurp(name) {
                Ok(bytes) => paste::serial(&bytes, &options, &mut buffer),
                Err(reason) => {
                    eprintln!("{}", paste::cannot_open_message(name.as_bytes(), &reason));
                    status = 1;
                    // A file that could not be opened contributes no row at
                    // all; only a file that opened and turned out to be empty
                    // still owes one.
                }
            }
        }
    } else {
        // Parallel mode stops at the first file it cannot open.
        let mut inputs: Vec<Input> = Vec::new();
        for name in &options.files {
            if name == "-" {
                inputs.push(open_stdin(&mut stdin_cursor, name));
                continue;
            }
            match slurp(name) {
                Ok(bytes) => inputs.push(Input::new(name.clone(), bytes)),
                Err(reason) => {
                    eprintln!("{}", paste::cannot_open_message(name.as_bytes(), &reason));
                    std::process::exit(1);
                }
            }
        }
        paste::parallel(&mut inputs, &options, &mut buffer);
    }

    if out.write_all(&buffer).is_err() || out.flush().is_err() {
        eprintln!("paste: write error");
        status = 1;
    }
    std::process::exit(status);
}