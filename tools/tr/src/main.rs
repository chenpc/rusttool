//! `tr(1)`: translate, squeeze, and/or delete characters.

use std::io::{self, Read, Write};

use tr::{arrays, check_operands, contains, translate, OperandProblem, Options, Problem};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: tr [OPTION]... STRING1 [STRING2]

Translate, squeeze, and/or delete characters from standard input, writing to
standard output.

  -c, -C, --complement        use the complement of ARRAY1
  -d, --delete                delete characters in ARRAY1, do not translate
  -s, --squeeze-repeats       replace each sequence of a repeated character
                              that is listed in the last specified ARRAY with a
                              single occurrence of that character
  -t, --truncate-set1         first truncate ARRAY1 to length of ARRAY2
      --help     display this help and exit
      --version  output version information and exit
";

fn message_for(problem: Problem) -> String {
    match problem {
        Problem::BadRange(text) => tr::bad_range_message(&text),
        Problem::BadClass(name) => tr::bad_class_message(&name),
    }
}

/// The usage errors, each with the second line coreutils adds and the Try line.
fn operand_message(problem: OperandProblem) -> Vec<String> {
    match problem {
        OperandProblem::Missing => vec![tr::missing_operand_message()],
        OperandProblem::MissingAfterTranslating(operand) => vec![
            tr::missing_after_message(&operand),
            tr::two_strings_translating_message(),
        ],
        OperandProblem::MissingAfterDeletingAndSqueezing(operand) => vec![
            tr::missing_after_message(&operand),
            tr::two_strings_message(),
        ],
        OperandProblem::Extra(operand) => vec![tr::extra_operand_message(&operand)],
        OperandProblem::ExtraWhileDeleting(operand) => vec![
            tr::extra_while_deleting_message(&operand),
            tr::only_one_string_message(),
        ],
    }
}

fn parse(args: &[String]) -> Result<(Options, Vec<String>), String> {
    let mut options = Options::default();
    let mut operands: Vec<String> = Vec::new();
    let mut no_more = false;
    let mut i = 0usize;

    while i < args.len() {
        let arg = args[i].clone();
        i += 1;
        if no_more || arg == "-" || !arg.starts_with('-') || arg == "[=...=]" {
            operands.push(arg);
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
                    std::process::exit(0);
                }
                "version" => {
                    println!("tr (rusttool) {}", VERSION);
                    std::process::exit(0);
                }
                "complement" => options.complement = true,
                "delete" => options.delete = true,
                "squeeze-repeats" => options.squeeze = true,
                "truncate-set1" => options.truncate = true,
                other => return Err(format!("@@UNRECOGNIZED@@{}", other)),
            }
            continue;
        }
        let letters: Vec<char> = arg.chars().skip(1).collect();
        for letter in letters {
            match letter {
                'c' | 'C' => options.complement = true,
                'd' => options.delete = true,
                's' => options.squeeze = true,
                't' => options.truncate = true,
                other => return Err(format!("@@INVALID@@{}", other)),
            }
        }
    }

    Ok((options, operands))
}

/// Translate, delete and squeeze one buffer.
fn convert(
    input: &[u8],
    set1: &[u8],
    set2: &[u8],
    options: Options,
    out: &mut dyn Write,
) -> io::Result<()> {
    // Squeezing happens after translation or deletion, over the last array that
    // was specified, as the manual says.
    let squeeze_set: Vec<u8> = if options.squeeze {
        if set2.is_empty() {
            set1.to_vec()
        } else {
            set2.to_vec()
        }
    } else {
        Vec::new()
    };

    let mut previous: Option<u8> = None;
    for byte in input {
        let out_byte = if options.delete {
            if contains(set1, *byte) {
                continue;
            }
            *byte
        } else if set2.is_empty() {
            *byte
        } else {
            translate(*byte, set1, set2).unwrap_or(*byte)
        };
        if options.squeeze && contains(&squeeze_set, out_byte) && previous == Some(out_byte) {
            continue;
        }
        out.write_all(&[out_byte])?;
        previous = Some(out_byte);
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (options, operands) = match parse(&args) {
        Ok(parsed) => parsed,
        Err(message) => {
            // coreutils prints the diagnostic and the Try line, not the whole
            // help.
            if let Some(name) = message.strip_prefix("@@UNRECOGNIZED@@") {
                eprintln!("{}", tr::unrecognized_option_message(&format!("--{}", name)));
            } else if let Some(letter) = message.strip_prefix("@@INVALID@@") {
                let letter = letter.chars().next().unwrap_or('?');
                eprintln!("{}", tr::invalid_option_message(letter));
            } else {
                eprintln!("{}", message);
            }
            eprintln!("{}", tr::try_help_message());
            std::process::exit(1);
        }
    };

    // The mode decides how many STRINGs are needed, and which are extra.
    let (text1, text2) = match check_operands(&operands, options) {
        Ok(pair) => pair,
        Err(problem) => {
            for line in operand_message(problem) {
                eprintln!("{}", line);
            }
            eprintln!("{}", tr::try_help_message());
            std::process::exit(1);
        }
    };

    let (set1, set2) = match arrays(&text1, text2.as_deref(), options) {
        Ok(arrays) => arrays,
        Err(problem) => {
            eprintln!("{}", message_for(problem));
            std::process::exit(1);
        }
    };

    let mut input = Vec::new();
    if let Err(error) = io::stdin().read_to_end(&mut input) {
        eprintln!("tr: read error: {}", error);
        std::process::exit(1);
    }
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if convert(&input, &set1, &set2, options, &mut out).is_err() {
        std::process::exit(1);
    }
    let _ = out.flush();
    std::process::exit(0);
}