//! `mkdir(1)`: create directories.

use mkdir::{components, Options};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: mkdir [OPTION]... DIRECTORY...

Create the DIRECTORY(s).

  -p, --parents     no error if existing, make parent directories
  -v, --verbose     print a line for each created directory
  -m, --mode=MODE   set file mode
      --help        display this help and exit
      --version     output version information and exit
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut options = Options { parents: false, verbose: false, tolerate_existing: false };
    let mut mode: Option<u32> = None;
    let mut operands: Vec<String> = Vec::new();
    let mut no_more = false;
    let mut i = 0usize;

    while i < args.len() {
        let arg = args[i].clone();
        i += 1;
        if no_more || arg == "-" || !arg.starts_with('-') {
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
                    println!("mkdir (rusttool) {}", VERSION);
                    return;
                }
                "parents" => {
                    options.parents = true;
                    options.tolerate_existing = true;
                }
                "verbose" => options.verbose = true,
                "mode" => {
                    let value = value.or_else(|| {
                        if i < args.len() {
                            let v = args[i].clone();
                            i += 1;
                            Some(v)
                        } else {
                            None
                        }
                    });
                    match value.as_deref().and_then(|v| u32::from_str_radix(v, 8).ok()) {
                        Some(m) => mode = Some(m),
                        None => {
                            eprintln!(
                                "mkdir: invalid mode: '{}'",
                                value.unwrap_or_default()
                            );
                            std::process::exit(1);
                        }
                    }
                }
                other => {
                    eprintln!("mkdir: unrecognized option '--{}'", other);
                    eprintln!("Try 'mkdir --help' for more information.");
                    std::process::exit(1);
                }
            }
            continue;
        }
        let bytes: Vec<char> = arg[1..].chars().collect();
        let mut idx = 0usize;
        while idx < bytes.len() {
            match bytes[idx] {
                'p' => {
                    options.parents = true;
                    options.tolerate_existing = true;
                }
                'v' => options.verbose = true,
                'm' => {
                    let value: String = if idx + 1 < bytes.len() {
                        bytes[idx + 1..].iter().collect()
                    } else if i < args.len() {
                        let v = args[i].clone();
                        i += 1;
                        v
                    } else {
                        eprintln!("mkdir: option requires an argument -- 'm'");
                        std::process::exit(1);
                    };
                    match u32::from_str_radix(&value, 8) {
                        Ok(m) => mode = Some(m),
                        Err(_) => {
                            eprintln!("mkdir: invalid mode: '{}'", value);
                            std::process::exit(1);
                        }
                    }
                    idx = bytes.len();
                    continue;
                }
                other => {
                    eprintln!("mkdir: invalid option -- '{}'", other);
                    eprintln!("Try 'mkdir --help' for more information.");
                    std::process::exit(1);
                }
            }
            idx += 1;
        }
    }

    if operands.is_empty() {
        eprintln!("mkdir: missing operand");
        eprintln!("Try 'mkdir --help' for more information.");
        std::process::exit(1);
    }

    let mut failed = false;
    for operand in operands {
        let targets = if options.parents {
            components(&operand)
        } else {
            vec![operand.clone()]
        };
        for target in targets {
            let result = std::fs::create_dir(&target);
            match result {
                Ok(()) => {
                    if let Some(m) = mode {
                        let _ = std::fs::set_permissions(
                            &target,
                            std::os::unix::fs::PermissionsExt::from_mode(m),
                        );
                    }
                    if options.verbose {
                        println!("created directory '{}'", target);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let is_dir = std::fs::metadata(&target).map(|m| m.is_dir()).unwrap_or(false);
                    if !(options.tolerate_existing && is_dir) {
                        eprintln!(
                            "mkdir: cannot create directory '{}': File exists",
                            target
                        );
                        failed = true;
                    }
                }
                Err(e) => {
                    eprintln!("mkdir: cannot create directory '{}': {}", target, e);
                    failed = true;
                }
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}
