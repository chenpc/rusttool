//! `dirname(1)`: strip the last component from a path.

use dirname::dirname;

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: dirname [OPTION]... NAME...

Strip non-directory component from FILE(s).

  -z, --zero      end output with NUL, not newline
  -a, --multiple   do not fail on missing operands
      --help      display this help and exit
      --version   output version information and exit
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut operands: Vec<String> = Vec::new();
    let mut no_more = false;
    let mut zero = false;
    let mut multiple = false;

    for arg in &args {
        if no_more || !arg.starts_with('-') {
            operands.push(arg.clone());
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
                    return;
                }
                "version" => {
                    println!("dirname (rusttool) {}", VERSION);
                    return;
                }
                "zero" => zero = true,
                "multiple" => multiple = true,
                other => {
                    eprintln!("dirname: unrecognized option '--{}'", other);
                    eprintln!("Try 'dirname --help' for more information.");
                    std::process::exit(1);
                }
            }
            continue;
        }
        for ch in arg[1..].chars() {
            match ch {
                'z' => zero = true,
                'a' => multiple = true,
                other => {
                    eprintln!("dirname: invalid option -- '{}'", other);
                    eprintln!("Try 'dirname --help' for more information.");
                    std::process::exit(1);
                }
            }
        }
    }

    if operands.is_empty() && !multiple {
        eprintln!("dirname: missing operand");
        eprintln!("Try 'dirname --help' for more information.");
        std::process::exit(1);
    }

    for operand in &operands {
        let text = dirname(operand);
        if zero {
            print!("{}\0", text);
        } else {
            println!("{}", text);
        }
    }
}
