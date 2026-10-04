//! `nl(1)`: number lines of files.

use std::fs::File;
use std::io::{self, BufWriter, Read, Write};

use nl::{Delimiters, Format, Numberer, Options, Section, Style};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: nl [OPTION]... [FILE]...

Write each FILE to standard output, with line numbers added.

With no FILE, or when FILE is -, read standard input.
Mandatory arguments to long options are mandatory for short options too.

  -b, --body-numbering=STYLE      use STYLE for numbering body lines
  -d, --section-delimiter=CC      use CC for logical page delimiters
  -f, --footer-numbering=STYLE    use STYLE for numbering footer lines
  -h, --header-numbering=STYLE    use STYLE for numbering header lines
  -i, --line-increment=NUMBER     line number increment at each line
  -l, --join-blank-lines=NUMBER   group of NUMBER empty lines counted as one
  -n, --number-format=FORMAT      insert line numbers according to FORMAT
  -p, --no-renumber               do not reset line numbers for each section
  -s, --number-separator=STRING   add STRING after (possible) line number
  -v, --starting-line-number=NUMBER  first line number for each section
  -w, --number-width=NUMBER       use NUMBER columns for line numbers
      --help     display this help and exit
      --version  output version information and exit

Default options are: -bt -d'\\:' -fn -hn -i1 -l1 -n'rn' -s<TAB> -v1 -w6

CC are two delimiter characters used to construct logical page delimiters;
a missing second character implies ':'.  As a GNU extension one can specify
more than two characters, and also specifying the empty string (-d '')
disables section matching.

STYLE is one of:

  a      number all lines
  t      number only nonempty lines
  n      number no lines
  pBRE   number only lines that contain a match for the basic regular
         expression, BRE

FORMAT is one of:

  ln     left justified, no leading zeros
  rn     right justified, no leading zeros
  rz     right justified, leading zeros
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

fn parse(args: &[String]) -> Result<(Options, Vec<String>), Failure> {
    let mut options = Options::default();
    let mut files: Vec<String> = Vec::new();
    let mut ok = true;
    let mut no_more = false;
    let mut i = 0usize;

    // A STYLE value is a letter, or 'p' plus a basic regular expression.
    //
    // A bad letter is only a warning: coreutils records the error, reads the
    // rest of the command line and prints one Try line at the end. A `pBRE`
    // that will not compile is fatal on the spot, because the regex compiler
    // itself calls error() and never reaches the usage hint.
    enum StyleError {
        /// `invalid ... numbering style: ‘x’`, warned about and skipped.
        Style(String),
        /// `Invalid regular expression`, fatal without the Try line.
        Regex(String),
    }

    fn build_style(letter: char, value: &str) -> Result<Style, StyleError> {
        match value.as_bytes().first() {
            Some(b'a') if value.len() == 1 => Ok(Style::All),
            Some(b't') if value.len() == 1 => Ok(Style::NonEmpty),
            Some(b'n') if value.len() == 1 => Ok(Style::None),
            Some(b'p') => match nl::bre::Regex::new(&value[1..]) {
                Ok(regex) => Ok(Style::Pattern(regex)),
                Err(_) => {
                    Err(StyleError::Regex(nl::invalid_regex_message(
                        "Invalid regular expression",
                    )))
                }
            },
            _ => Err(StyleError::Style(nl::bad_style_message(letter, value))),
        }
    }

    // The numeric options, with the bounds coreutils uses.
    fn build_number(
        what: &str,
        value: &str,
        low: i64,
        high: i64,
    ) -> Result<i64, String> {
        match value.parse::<i64>() {
            Ok(number) if number >= low && number <= high => Ok(number),
            Ok(_) => Err(nl::bad_number_message(
                what,
                value,
                Some("Numerical result out of range"),
            )),
            Err(_) => Err(nl::bad_number_message(what, value, None)),
        }
    }

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
                    println!("nl (rusttool) {}", VERSION);
                    std::process::exit(0);
                }
                "header-numbering" | "body-numbering" | "footer-numbering"
                | "starting-line-number" | "line-increment" | "join-blank-lines"
                | "number-separator" | "number-width" | "number-format"
                | "section-delimiter" => {
                    let letter = match name {
                        "header-numbering" => 'h',
                        "body-numbering" => 'b',
                        "footer-numbering" => 'f',
                        "starting-line-number" => 'v',
                        "line-increment" => 'i',
                        "join-blank-lines" => 'l',
                        "number-separator" => 's',
                        "number-width" => 'w',
                        "number-format" => 'n',
                        _ => 'd',
                    };
                    let value = match inline.clone().or_else(|| next_value(&mut i)) {
                        Some(value) => value,
                        None => {
                            return Err(Failure::Fatal(nl::long_requires_argument_message(
                                name,
                            )))
                        }
                    };
                    match name {
                        "header-numbering" | "body-numbering" | "footer-numbering" => {
                            match build_style(letter, &value) {
                                Ok(style) => match letter {
                                    'h' => options.header = style,
                                    'f' => options.footer = style,
                                    _ => options.body = style,
                                },
                                Err(StyleError::Style(message)) => {
                                    eprintln!("{}", message);
                                    ok = false;
                                }
                                Err(StyleError::Regex(message)) => {
                                    eprintln!("{}", message);
                                    return Err(Failure::Plain);
                                }
                            }
                        }
                        "starting-line-number" => match build_number(
                            "starting line number",
                            &value,
                            i64::MIN,
                            i64::MAX,
                        ) {
                            Ok(number) => options.start = number,
                            Err(message) => {
                                eprintln!("{}", message);
                                return Err(Failure::Plain);
                            }
                        },
                        "line-increment" => match build_number(
                            "line number increment",
                            &value,
                            i64::MIN,
                            i64::MAX,
                        ) {
                            Ok(number) => options.increment = number,
                            Err(message) => {
                                eprintln!("{}", message);
                                return Err(Failure::Plain);
                            }
                        },
                        "join-blank-lines" => match build_number(
                            "line number of blank lines",
                            &value,
                            1,
                            i64::MAX,
                        ) {
                            Ok(number) => options.join_blank = number,
                            Err(message) => {
                                eprintln!("{}", message);
                                return Err(Failure::Plain);
                            }
                        },
                        "number-width" => {
                            match build_number("line number field width", &value, 1, i64::MAX) {
                                Ok(number) => options.width = number as usize,
                                Err(message) => {
                                    eprintln!("{}", message);
                                    return Err(Failure::Plain);
                                }
                            }
                        }
                        "number-format" => match Format::parse(&value) {
                            Some(format) => options.format = format,
                            None => {
                                eprintln!("{}", nl::bad_format_message(&value));
                                ok = false;
                            }
                        },
                        "number-separator" => options.separator = value.into_bytes(),
                        _ => options.delimiters = Delimiters::from_option(value.as_bytes()),
                    }
                }
                "no-renumber" => options.no_renumber = true,
                _ => return Err(Failure::Fatal(nl::unrecognized_option_message(name))),
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
                'h' | 'b' | 'f' | 'v' | 'i' | 'l' | 's' | 'w' | 'n' | 'd' => {
                    let value = match attached(&letters, &mut index) {
                        Some(value) => value,
                        None => return Err(Failure::Fatal(nl::requires_argument_message(letter))),
                    };
                    match letter {
                        'h' | 'b' | 'f' => match build_style(letter, &value) {
                            Ok(style) => match letter {
                                'h' => options.header = style,
                                'f' => options.footer = style,
                                _ => options.body = style,
                            },
                            Err(StyleError::Style(message)) => {
                                eprintln!("{}", message);
                                ok = false;
                            }
                            Err(StyleError::Regex(message)) => {
                                eprintln!("{}", message);
                                return Err(Failure::Plain);
                            }
                        },
                        'v' => match build_number("starting line number", &value, i64::MIN, i64::MAX)
                        {
                            Ok(number) => options.start = number,
                            Err(message) => {
                                eprintln!("{}", message);
                                return Err(Failure::Plain);
                            }
                        },
                        'i' => {
                            match build_number("line number increment", &value, i64::MIN, i64::MAX) {
                                Ok(number) => options.increment = number,
                                Err(message) => {
                                    eprintln!("{}", message);
                                    return Err(Failure::Plain);
                                }
                            }
                        }
                        'l' => match build_number("line number of blank lines", &value, 1, i64::MAX)
                        {
                            Ok(number) => options.join_blank = number,
                            Err(message) => {
                                eprintln!("{}", message);
                                return Err(Failure::Plain);
                            }
                        },
                        'w' => match build_number("line number field width", &value, 1, i64::MAX) {
                            Ok(number) => options.width = number as usize,
                            Err(message) => {
                                eprintln!("{}", message);
                                return Err(Failure::Plain);
                            }
                        },
                        'n' => match Format::parse(&value) {
                            Some(format) => options.format = format,
                            None => {
                                eprintln!("{}", nl::bad_format_message(&value));
                                ok = false;
                            }
                        },
                        's' => options.separator = value.into_bytes(),
                        _ => options.delimiters = Delimiters::from_option(value.as_bytes()),
                    }
                }
                'p' => options.no_renumber = true,
                other => return Err(Failure::Fatal(nl::invalid_option_message(other))),
            }
        }
    }

    if !ok {
        return Err(Failure::Usage);
    }
    options.files = files.clone();
    Ok((options, files))
}

/// How a run ended.
enum Failure {
    /// The message is already printed, or is printed here, and the program
    /// stops without the Try line.
    Plain,
    /// The message comes first and the Try line follows.
    Fatal(String),
    /// Getopt already printed its diagnostic; only the usage follows.
    Usage,
}

/// Split `buffer` into lines, keeping a last line without a terminator.
fn lines_of(buffer: &[u8]) -> Vec<&[u8]> {
    if buffer.is_empty() {
        return Vec::new();
    }
    let had_terminator = buffer.last() == Some(&b'\n');
    let mut pieces: Vec<&[u8]> = buffer.split(|byte| *byte == b'\n').collect();
    if had_terminator {
        pieces.pop();
    }
    pieces
}

fn number_buffer(
    buffer: &[u8],
    options: &Options,
    section: &mut Section,
    numberer: &mut Numberer,
    out: &mut dyn Write,
) -> io::Result<()> {
    for line in lines_of(buffer) {
        match options.delimiters.classify(line) {
            Some(found) => {
                // A delimiter is replaced by an empty line and nothing else.
                numberer.enter_section(options);
                *section = found;
                out.write_all(&nl::delimiter_line())?;
            }
            None => {
                let number = numberer.number(line, *section, options);
                out.write_all(&nl::text_line(number, line, options))?;
                if numberer.overflowed() {
                    eprintln!("{}", nl::overflow_message());
                    return Err(io::Error::other("overflow"));
                }
            }
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (options, files) = match parse(&args) {
        Ok(parsed) => parsed,
        Err(Failure::Usage) => {
            eprintln!("{}", nl::try_help_message());
            std::process::exit(1);
        }
        Err(Failure::Plain) => std::process::exit(1),
        Err(Failure::Fatal(message)) => {
            eprintln!("{}", message);
            eprintln!("{}", nl::try_help_message());
            std::process::exit(1);
        }
    };

    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let mut status = 0;

    // The counter and the section carry over from one file to the next, which
    // is what coreutils does by keeping them outside process_file.
    let mut section = Section::Body;
    let mut numberer = Numberer::new(&options);

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
                    eprintln!("{}", nl::cannot_open_message(file, &io_error_reason(&error)));
                    status = 1;
                }
            }
        }
    }

    for input in inputs.iter_mut() {
        let mut buffer = Vec::new();
        if let Err(error) = input.read_to_end(&mut buffer) {
            eprintln!("nl: read error: {}", io_error_reason(&error));
            status = 1;
            continue;
        }
        if number_buffer(&buffer, &options, &mut section, &mut numberer, &mut out).is_err() {
            std::process::exit(1);
        }
    }
    let _ = out.flush();
    std::process::exit(status);
}