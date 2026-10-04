//! `comm(1)`: compare two sorted files line by line.

use std::fs::File;
use std::io::{self, Read, Write};

use comm::{OrderCheck, Options};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: comm [OPTION]... FILE1 FILE2

Compare sorted files FILE1 and FILE2 line by line.

When FILE1 or FILE2 (not both) is -, read standard input.

  -1                      suppress column 1 (lines unique to FILE1)
  -2                      suppress column 2 (lines unique to FILE2)
  -3                      suppress column 3 (lines that appear in both files)
      --check-order      check that the input is correctly sorted, even if all
                            input lines are pairable
      --nocheck-order    do not check that the input is correctly sorted
      --output-delimiter=STR
                          separate columns with STR
      --total            output a summary
  -z, --zero-terminated  line delimiter is NUL, not newline
      --help     display this help and exit
      --version  output version information and exit

Note, comparisons honor the rules specified by 'LC_COLLATE'.
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
    /// Nothing has been printed yet; the Try line is all that is left.
    Usage,
    /// The message comes first and the Try line follows.
    UsageWith(String),
}

fn parse(args: &[String]) -> Result<Options, Failure> {
    let mut options = Options::default();
    let mut files: Vec<String> = Vec::new();
    // coreutils remembers whether a delimiter was given at all, because giving
    // two different ones is an error while giving the same one twice is not.
    let mut delimiter_given: Option<Vec<u8>> = None;
    let mut no_more = false;
    let mut index = 0usize;

    // glibc's getopt_long permutes, so an option may follow an operand; the
    // operands are gathered as they are met and looked at afterwards.
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
                    println!("comm (rusttool) {}", VERSION);
                    std::process::exit(0);
                }
                "check-order" => options.order_check = OrderCheck::Enabled,
                "nocheck-order" => options.order_check = OrderCheck::Disabled,
                "total" => options.total = true,
                "zero-terminated" => options.terminator = 0,
                "output-delimiter" => {
                    let value = match inline {
                        Some(value) => value.into_bytes(),
                        None => match args.get(index) {
                            Some(value) => {
                                index += 1;
                                value.clone().into_bytes()
                            }
                            None => {
                                return Err(Failure::UsageWith(
                                    comm::requires_argument_message(name),
                                ))
                            }
                        },
                    };
                    if let Some(previous) = &delimiter_given {
                        if *previous != value {
                            eprintln!("{}", comm::multiple_delimiters_message());
                            return Err(Failure::Plain);
                        }
                    }
                    delimiter_given = Some(value);
                }
                _ => return Err(Failure::UsageWith(comm::unrecognized_option_message(name))),
            }
            continue;
        }

        let letters: Vec<char> = arg.chars().skip(1).collect();
        let mut position = 0usize;
        while position < letters.len() {
            let letter = letters[position];
            position += 1;
            match letter {
                '1' => options.only_file_1 = false,
                '2' => options.only_file_2 = false,
                '3' => options.both = false,
                'z' => options.terminator = 0,
                other => return Err(Failure::UsageWith(comm::invalid_option_message(other))),
            }
        }
    }

    options.output_delimiter = delimiter_given.unwrap_or_else(|| options.output_delimiter);

    // comm.c insists on exactly two operands, and names the one it tripped over.
    if files.len() < 2 {
        match files.last() {
            Some(last) => {
                eprintln!("{}", comm::missing_operand_after_message(last));
                return Err(Failure::Usage);
            }
            None => {
                eprintln!("{}", comm::missing_operand_message());
                return Err(Failure::Usage);
            }
        }
    }
    if files.len() > 2 {
        eprintln!("{}", comm::extra_operand_message(&files[2]));
        return Err(Failure::Usage);
    }
    options.files = files;
    Ok(options)
}

/// Read one operand. `-` is standard input, and coreutils hands every `-` the
/// same stream, so a second `-` continues where the first one stopped.
fn open(name: &str, terminator: u8, stdin: &mut Option<comm::Stream>) -> Result<comm::Reader, String> {
    if name == "-" {
        if stdin.is_none() {
            let mut bytes = Vec::new();
            if let Err(error) = io::stdin().read_to_end(&mut bytes) {
                *stdin = Some(comm::Stream::new(Vec::new()));
                return Err(io_error_reason(&error));
            }
            *stdin = Some(comm::Stream::new(bytes));
        }
        return Ok(stdin.as_ref().unwrap().reader(terminator, false));
    }
    let mut bytes = Vec::new();
    let read = match File::open(name) {
        Ok(mut handle) => handle.read_to_end(&mut bytes),
        Err(error) => return Err(io_error_reason(&error)),
    };
    match read {
        Ok(_) => Ok(comm::Stream::new(bytes).reader(terminator, false)),
        Err(error) => Err(io_error_reason(&error)),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse(&args) {
        Ok(options) => options,
        Err(Failure::Plain) => std::process::exit(1),
        Err(Failure::Usage) => {
            eprintln!("{}", comm::try_help_message());
            std::process::exit(1);
        }
        Err(Failure::UsageWith(message)) => {
            eprintln!("{}", message);
            eprintln!("{}", comm::try_help_message());
            std::process::exit(1);
        }
    };

    let mut stdin: Option<comm::Stream> = None;
    // comm.c closes both streams once the merge is done, and closing standard
    // input twice fails -- which happens before the summary is printed, so a
    // `--total` run of `comm - -` never gets to write one.
    let double_stdin = options.files.iter().filter(|name| *name == "-").count() > 1;
    let mut options = options;
    if double_stdin {
        options.total = false;
    }
    let mut left = match open(&options.files[0], options.terminator, &mut stdin) {
        Ok(reader) => reader,
        Err(reason) => {
            eprintln!(
                "{}",
                comm::cannot_open_message(options.files[0].as_bytes(), &reason)
            );
            std::process::exit(1);
        }
    };
    let mut right = match open(&options.files[1], options.terminator, &mut stdin) {
        Ok(reader) => reader,
        Err(reason) => {
            eprintln!(
                "{}",
                comm::cannot_open_message(options.files[1].as_bytes(), &reason)
            );
            std::process::exit(1);
        }
    };
    // The two readers are told apart here rather than in `open`, because only
    // the caller knows which operand is the first one.
    left.set_first();

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let mut buffer: Vec<u8> = Vec::new();
    let (summary, stopped) = comm::merge(&mut left, &mut right, &options, &mut buffer);
    if out.write_all(&buffer).is_err() || out.flush().is_err() {
        eprintln!("comm: write error");
        std::process::exit(1);
    }

    // Each file that was reported gets one message, in the order the merge ran
    // into them, and the closing complaint only appears when nothing was fatal.
    for which in &summary.reported {
        eprintln!("{}", comm::unsorted_file_message(*which));
    }

    // A fatal order error leaves comm.c through error(), so it never reaches the
    // close that would fail on standard input being closed twice.
    if stopped {
        std::process::exit(1);
    }
    // Otherwise both streams are closed, and closing standard input twice is
    // what turns `comm - -` into a failure.
    if double_stdin {
        eprintln!(
            "{}",
            comm::cannot_open_message(b"-", "Bad file descriptor")
        );
        std::process::exit(1);
    }
    if summary.disordered {
        eprintln!("{}", comm::unsorted_input_message());
        std::process::exit(1);
    }
    std::process::exit(0);
}
