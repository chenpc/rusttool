//! `basename(1)`: strip directory and suffix from a path.

use basename::basename;

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: basename [OPTION]... NAME...

Strip directory and suffix from FILE(s).

  -a, --multiple   handle multiple FILEs
  -s, --suffix=SUFFIX  strip a suffix
      --help       display this help and exit
      --version    output version information and exit
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut suffix: Option<String> = None;
    let mut operands: Vec<String> = Vec::new();
    let mut multiple = false;
    let mut no_more = false;
    let mut i = 0usize;

    while i < args.len() {
        let arg = args[i].clone();
        i += 1;
        if no_more || !arg.starts_with('-') {
            operands.push(arg);
            continue;
        }
        if arg == "--" {
            no_more = true;
            continue;
        }
        if let Some(long) = arg.strip_prefix("--") {
            let (name, value) = match long.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (long, None),
            };
            match name {
                "help" => {
                    print!("{}", HELP);
                    return;
                }
                "version" => {
                    println!("basename (rusttool) {}", VERSION);
                    return;
                }
                "suffix" => {
                    let v = value.or_else(|| {
                        if i < args.len() {
                            let v = args[i].clone();
                            i += 1;
                            Some(v)
                        } else {
                            None
                        }
                    });
                    match v {
                        Some(v) => suffix = Some(v),
                        None => {
                            eprintln!("basename: option '--suffix' requires an argument");
                            std::process::exit(1);
                        }
                    }
                }
                "multiple" => multiple = true,
                other => {
                    eprintln!("basename: unrecognized option '--{}'", other);
                    eprintln!("Try 'basename --help' for more information.");
                    std::process::exit(1);
                }
            }
            continue;
        }
        for ch in arg[1..].chars() {
            match ch {
                'a' => multiple = true,
                's' => {
                    if i < args.len() {
                        suffix = Some(args[i].clone());
                        i += 1;
                    } else {
                        eprintln!("basename: option requires an argument -- 's'");
                        std::process::exit(1);
                    }
                }
                other => {
                    eprintln!("basename: invalid option -- '{}'", other);
                    eprintln!("Try 'basename --help' for more information.");
                    std::process::exit(1);
                }
            }
        }
    }

    if operands.is_empty() {
        eprintln!("basename: missing operand");
        eprintln!("Try 'basename --help' for more information.");
        std::process::exit(1);
    }

    // GNU: with -a every operand is a name; without it only the first counts
    // and the rest are suffixes.
    if multiple {
        for operand in &operands {
            println!("{}", basename(operand, suffix.as_deref()));
        }
    } else {
        let name = &operands[0];
        let mut all: Vec<&str> = operands[1..].iter().map(|s| s.as_str()).collect();
        if suffix.is_none() && !all.is_empty() {
            // Every trailing operand is a suffix candidate; the longest match wins.
            let mut best: Option<&str> = None;
            for candidate in &all {
                if name.ends_with(candidate)
                    && best.map(|b| candidate.len() > b.len()).unwrap_or(true)
                {
                    best = Some(candidate);
                }
            }
            if let Some(best) = best {
                all.retain(|c| *c != best);
            }
            println!("{}", basename(name, best));
        } else {
            println!("{}", basename(name, suffix.as_deref()));
        }
    }
}
