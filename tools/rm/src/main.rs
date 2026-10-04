//! `rm(1)`: remove files or directories.

use std::io::{BufRead, Write};

use rm::{decide, directory_message, missing_message, Action, Options};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: rm [OPTION]... [FILE]...

Remove (unlink) the FILE(s).

  -f, --force           ignore nonexistent files, never prompt
  -i, --interactive     prompt before every removal
  -d, --dir             remove empty directories
  -r, -R, --recursive   remove directories and their contents
      --help            display this help and exit
      --version         output version information and exit
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut options = Options { preserve_root: true, ..Options::default() };
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
            match long {
                "help" => {
                    print!("{}", HELP);
                    return;
                }
                "version" => {
                    println!("rm (rusttool) {}", VERSION);
                    return;
                }
                "force" => options.force = true,
                "interactive" => options.interactive = true,
                "dir" => options.dir = true,
                "recursive" => options.recursive = true,
                "one-file-system" => options.one_file_system = true,
                "no-preserve-root" => options.preserve_root = false,
                other => {
                    eprintln!("rm: unrecognized option '--{}'", other);
                    eprintln!("Try 'rm --help' for more information.");
                    std::process::exit(1);
                }
            }
            continue;
        }
        for ch in arg[1..].chars() {
            match ch {
                'f' => options.force = true,
                'i' => options.interactive = true,
                'd' => options.dir = true,
                'r' | 'R' => options.recursive = true,
                'v' => {}
                other => {
                    eprintln!("rm: invalid option -- '{}'", other);
                    eprintln!("Try 'rm --help' for more information.");
                    std::process::exit(1);
                }
            }
        }
    }

    if operands.is_empty() && !options.force {
        eprintln!("rm: missing operand");
        eprintln!("Try 'rm --help' for more information.");
        std::process::exit(1);
    }

    let mut failed = false;
    let stdin = std::io::stdin();
    for operand in operands {
        if options.preserve_root && operand == "/" {
            eprintln!("rm: it is dangerous to operate recursively on '/'");
            eprintln!("rm: use --no-preserve-root to override this failsafe");
            failed = true;
            continue;
        }
        let meta = std::fs::symlink_metadata(&operand);
        let (exists, is_dir) = match meta {
            Ok(m) => (true, m.is_dir()),
            Err(_) => (false, false),
        };
        match decide(&operand, exists, is_dir, &options) {
            Action::IgnoreMissing => continue,
            Action::RefuseDirectory => {
                eprintln!("{}", directory_message(&operand));
                failed = true;
                continue;
            }
            Action::Remove => {}
        }
        if options.interactive && !options.force {
            print!("rm: remove {}? ", operand);
            let _ = std::io::stdout().flush();
            let mut answer = String::new();
            let _ = stdin.lock().read_line(&mut answer);
            if !answer.trim().eq_ignore_ascii_case("y") {
                continue;
            }
        }
        let result = if is_dir {
            std::fs::remove_dir_all(&operand)
        } else {
            std::fs::remove_file(&operand)
        };
        if let Err(e) = result {
            let reason = match e.kind() {
                std::io::ErrorKind::NotFound => missing_message(&operand),
                _ => format!("rm: cannot remove '{}': {}", operand, e),
            };
            if !(options.force && e.kind() == std::io::ErrorKind::NotFound) {
                eprintln!("{}", reason);
                failed = true;
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}
