//! `rmdir(1)`: remove empty directories.

use rmdir::{chain, Action};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: rmdir [OPTION]... DIRECTORY...

Remove the DIRECTORY(s).

  -p, --parents                      remove parent directories too
      --ignore-fail-on-non-empty    do not fail on non-empty directories
      --help                         display this help and exit
      --version                      output version information and exit
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut parents = false;
    let mut ignore_non_empty = false;
    let mut operands: Vec<String> = Vec::new();
    let mut no_more = false;

    for arg in &args {
        if no_more || *arg == "-" || !arg.starts_with('-') {
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
                    println!("rmdir (rusttool) {}", VERSION);
                    return;
                }
                "parents" => parents = true,
                "ignore-fail-on-non-empty" => ignore_non_empty = true,
                other => {
                    eprintln!("rmdir: unrecognized option '--{}'", other);
                    eprintln!("Try 'rmdir --help' for more information.");
                    std::process::exit(1);
                }
            }
            continue;
        }
        for ch in arg[1..].chars() {
            match ch {
                'p' => parents = true,
                other => {
                    eprintln!("rmdir: invalid option -- '{}'", other);
                    eprintln!("Try 'rmdir --help' for more information.");
                    std::process::exit(1);
                }
            }
        }
    }

    if operands.is_empty() {
        eprintln!("rmdir: missing operand");
        eprintln!("Try 'rmdir --help' for more information.");
        std::process::exit(1);
    }

    let mut failed = false;
    for operand in operands {
        let targets = if parents { chain(&operand) } else { vec![operand.clone()] };
        for target in targets {
            match std::fs::remove_dir(&target) {
                Ok(()) => {}
                Err(e) => {
                    let not_empty = e.raw_os_error() == Some(libc::ENOTEMPTY)
                        || e.raw_os_error() == Some(libc::EEXIST);
                    let missing = e.kind() == std::io::ErrorKind::NotFound;
                    if not_empty && ignore_non_empty && !missing {
                        continue;
                    }
                    let reason = match e.raw_os_error() {
                        Some(libc::ENOTEMPTY) | Some(libc::EEXIST) => {
                            format!("rmdir: failed to remove '{}': Directory not empty", target)
                        }
                        Some(libc::ENOENT) => {
                            format!("rmdir: failed to remove '{}': No such file or directory", target)
                        }
                        _ => format!("rmdir: failed to remove '{}': {}", target, e),
                    };
                    eprintln!("{}", reason);
                    failed = true;
                    break; // -p stops at the first failure, like coreutils
                }
            }
        }
    }
    let _ = Action::Remove;
    if failed {
        std::process::exit(1);
    }
}
