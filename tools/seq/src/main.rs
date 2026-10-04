//! `seq(1)`: print a sequence of numbers.

use std::ffi::CString;

use seq::{check_format, default_format, looks_like_number, scan_arg, FormatError, Layout, Operand};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: seq [OPTION]... [FIRST [STEP]]... LAST

Print a sequence of numbers from FIRST to LAST, by STEP.

  -f, --format=FORMAT  use printf style FORMAT
  -s, --separator=STR  use STR instead of newline
  -w, --equal-width     equalise the width of all numbers
      --help            display this help and exit
      --version         output version information and exit
";

/// What `seq` leaves with when the command line is wrong. It is a plain 1, not
/// the 125 most coreutils tools use for "the tool itself failed".
const EXIT_USAGE: i32 = 1;

fn try_help() {
    eprintln!("Try 'seq --help' for more information.");
}

/// Print one value through printf, which is how `seq` outputs a number.
///
/// Going to the C library rather than formatting by hand is the point: the manual
/// says the format is "suitable for printing one argument of type 'double'", so
/// every flag, width, precision and conversion has to come out exactly as printf
/// would produce it, and there is no second implementation to keep in step.
///
/// The one thing that cannot be reproduced here is `%a`: `seq` passes a `long
/// double` and Rust cannot name that type, so the value goes across as a `double`
/// and the hexadecimal form comes out normalised differently. See the handover
/// note on `seq`.
fn printf_double(format: &str, value: f64) -> String {
    let text = match CString::new(format.as_bytes()) {
        Ok(text) => text,
        Err(_) => return String::new(),
    };
    // SAFETY: `text` is NUL-terminated, the variadic call gets exactly one double
    // which is what every conversion `seq` accepts asks for, and the buffer is
    // sized from the first snprintf's own report of the length it wanted, so
    // nothing is ever truncated.
    unsafe {
        let needed = libc::snprintf(std::ptr::null_mut(), 0, text.as_ptr(), value);
        if needed <= 0 {
            return String::new();
        }
        let mut buffer = vec![0u8; needed as usize + 1];
        libc::snprintf(buffer.as_mut_ptr() as *mut libc::c_char, buffer.len(), text.as_ptr(), value);
        buffer.truncate(needed as usize);
        String::from_utf8_lossy(&buffer).into_owned()
    }
}

/// The number as `seq` renders it, with the layout's suffix removed.
///
/// The rounding rule at the end of the sequence re-reads the rendered text, so
/// this is the text that has to be right.
fn rendered(format: &str, value: f64, layout: Layout) -> String {
    let text = printf_double(format, value);
    if text.len() >= layout.suffix_len {
        text[..text.len() - layout.suffix_len].to_string()
    } else {
        text
    }
}

fn main() {
    // `seq.c` opens the locale, and the quoting shape of every diagnostic follows
    // from it.
    quoting::open_locale();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut separator = "\n".to_string();
    let mut equal_width = false;
    let mut format: Option<String> = None;
    let mut operands: Vec<String> = Vec::new();
    // Option parsing stops at the first operand, the way the leading `+` in the
    // short option string asks for. Nothing is permuted to the front, so
    // `seq -w 1 10 -s ,` complains about an extra operand rather than taking `-s`.
    let mut operands_only = false;
    let mut index = 0usize;

    while index < args.len() {
        let argument = args[index].clone();
        index += 1;
        if operands_only {
            operands.push(argument);
            continue;
        }
        if argument == "--" {
            operands_only = true;
            continue;
        }
        // A leading `-` is only an option when what follows is not a number: a
        // sequence counts down as often as it counts up, and `seq -3` is a last
        // operand rather than a bundle of options.
        if argument == "-" || looks_like_number(&argument) {
            operands_only = true;
            operands.push(argument);
            continue;
        }
        if !argument.starts_with('-') {
            operands_only = true;
            operands.push(argument);
            continue;
        }

        let value_for = |index: &mut usize, inline: Option<String>| -> Option<String> {
            match inline {
                Some(value) => Some(value),
                None => match args.get(*index) {
                    Some(next) => {
                        *index += 1;
                        Some(next.clone())
                    }
                    None => None,
                },
            }
        };
        if let Some(long) = argument.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value.to_string())),
                None => (long, None),
            };
            match name {
                "help" => {
                    print!("{}", HELP);
                    return;
                }
                "version" => {
                    println!("seq (rusttool) {}", VERSION);
                    return;
                }
                "separator" => match value_for(&mut index, inline) {
                    Some(value) => separator = value,
                    None => {
                        eprintln!("seq: option '--separator' requires an argument");
                        try_help();
                        std::process::exit(EXIT_USAGE);
                    }
                },
                "format" => match value_for(&mut index, inline) {
                    Some(value) => format = Some(value),
                    None => {
                        eprintln!("seq: option '--format' requires an argument");
                        try_help();
                        std::process::exit(EXIT_USAGE);
                    }
                },
                "equal-width" => equal_width = true,
                other => {
                    eprintln!("seq: unrecognized option '--{}'", other);
                    try_help();
                    std::process::exit(EXIT_USAGE);
                }
            }
            continue;
        }

        let letters: Vec<char> = argument.chars().skip(1).collect();
        let mut at = 0usize;
        while at < letters.len() {
            match letters[at] {
                's' | 'f' => {
                    let option = letters[at];
                    let inline: Option<String> = if at + 1 < letters.len() {
                        Some(letters[at + 1..].iter().collect())
                    } else {
                        None
                    };
                    at = letters.len();
                    let text = match value_for(&mut index, inline) {
                        Some(text) => text,
                        None => {
                            eprintln!("seq: option requires an argument -- '{}'", option);
                            try_help();
                            std::process::exit(EXIT_USAGE);
                        }
                    };
                    if option == 's' {
                        separator = text;
                    } else {
                        format = Some(text);
                    }
                }
                'w' => {
                    equal_width = true;
                    at += 1;
                }
                other => {
                    eprintln!("seq: invalid option -- '{}'", other);
                    try_help();
                    std::process::exit(EXIT_USAGE);
                }
            }
        }
    }

    // A format is checked before anything is printed, and its complaints come
    // without the Try line: upstream reports them straight rather than through
    // `usage()`.
    let mut layout = Layout::default();
    if let Some(text) = &format {
        match check_format(text) {
            Ok(found) => layout = found,
            Err(problem) => report(problem, text),
        }
    }
    if format.is_some() && equal_width {
        eprintln!("seq: format string may not be specified with equal-width");
        try_help();
        std::process::exit(EXIT_USAGE);
    }

    if operands.len() > 3 {
        eprintln!("seq: extra operand {}", quoting::quote(&operands[3]));
        try_help();
        std::process::exit(EXIT_USAGE);
    }
    if operands.is_empty() {
        eprintln!("seq: missing operand");
        try_help();
        std::process::exit(EXIT_USAGE);
    }

    let mut numbers: Vec<Operand> = Vec::with_capacity(operands.len());
    for (position, text) in operands.iter().enumerate() {
        match scan_arg(text) {
            Some(number) => numbers.push(number),
            None => {
                // `nan` parses but is refused in its own words; anything that
                // does not parse at all is an invalid number.
                let word = if scan_arg(text).is_none()
                    && text.trim().parse::<f64>().map(f64::is_nan).unwrap_or(false)
                {
                    "invalid not-a-number argument"
                } else {
                    "invalid floating point argument"
                };
                let _ = position;
                eprintln!("seq: {}: {}", word, quoting::quote(text));
                try_help();
                std::process::exit(EXIT_USAGE);
            }
        }
    }

    let (first, step, last) = match numbers.len() {
        1 => (Operand { value: 1.0, width: 1, precision: 0 }, Operand { value: 1.0, width: 1, precision: 0 }, numbers[0]),
        2 => (numbers[0], Operand { value: 1.0, width: 1, precision: 0 }, numbers[1]),
        _ => {
            if numbers[1].value == 0.0 {
                eprintln!(
                    "seq: invalid Zero increment value: {}",
                    quoting::quote(&operands[1])
                );
                try_help();
                std::process::exit(EXIT_USAGE);
            }
            (numbers[0], numbers[1], numbers[2])
        }
    };

    let text = match &format {
        Some(template) => seq::strip_long_double(template),
        None => default_format(first, step, last, equal_width),
    };

    // The sequence walks `first + i * step` rather than adding step each time, so
    // a long run does not drift, and the number past LAST gets a second look: if
    // it renders as LAST but renders differently from the one before, it is real.
    let mut out_of_range = if step.value < 0.0 { first.value < last.value } else { last.value < first.value };
    let mut out = String::new();
    if !out_of_range {
        let mut x = first.value;
        let mut index = 1.0f64;
        loop {
            let previous = x;
            out.push_str(&printf_double(&text, x));
            if out_of_range {
                break;
            }
            x = first.value + index * step.value;
            index += 1.0;
            out_of_range = if step.value < 0.0 { x < last.value } else { last.value < x };
            if out_of_range {
                let past = rendered(&text, x, layout);
                let body = &past[layout.prefix_len.min(past.len())..];
                let rounds_to_last = body.trim().parse::<f64>().map(|v| v == last.value).unwrap_or(false);
                if rounds_to_last && rendered(&text, previous, layout) != past {
                    out.push_str(&separator);
                    out.push_str(&printf_double(&text, x));
                }
                break;
            }
            out.push_str(&separator);
        }
        out.push('\n');
    }
    print!("{}", out);
}

/// Report a refused format and leave. There is no Try line here.
fn report(problem: FormatError, format: &str) -> ! {
    eprintln!("{}", problem.message_in(format, quoting::fancy_quotes()));
    std::process::exit(EXIT_USAGE);
}