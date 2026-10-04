//! `ln(1)`: make links between files.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use ln::{
    backup_name, decide, plan, relative_target, verbose_line, Action, Options, Plan, Problem,
};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: ln [OPTION]... [-T] TARGET LINK_NAME
       ln [OPTION]... TARGET
       ln [OPTION]... TARGET... DIRECTORY
       ln [OPTION]... -t DIRECTORY TARGET...

Create links between files.

  -b                         like --backup but does not accept an argument
      --backup[=CONTROL]     make a backup of each existing destination file
  -d, -F, --directory        allow the superuser to attempt to hard link directories
  -f, --force                remove existing destination files
  -i, --interactive          prompt whether to remove destinations
  -L, --logical              dereference TARGETs that are symbolic links
  -n, --no-dereference       treat LINK_NAME as a normal file if it is a
                             symbolic link to a directory
  -P, --physical             make hard links directly to symbolic links
  -r, --relative             with -s, create links relative to link location
  -s, --symbolic             make symbolic links instead of hard links
  -S, --suffix=SUFFIX        override the usual backup suffix
  -t, --target-directory=DIRECTORY
                             specify the DIRECTORY in which to create the links
  -T, --no-target-directory  treat LINK_NAME as a normal file always
  -v, --verbose              print what is being done
      --help     display this help and exit
      --version  output version information and exit
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

fn parse(args: &[String]) -> Result<(Options, Vec<String>), String> {
    let mut options = Options::default();
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
                    println!("ln (rusttool) {}", VERSION);
                    std::process::exit(0);
                }
                "symbolic" => options.symbolic = true,
                "force" => options.force = true,
                "interactive" => options.interactive = true,
                "verbose" => options.verbose = true,
                "no-dereference" => options.no_dereference = true,
                "relative" => options.relative = true,
                "no-target-directory" => options.no_target_directory = true,
                "directory" => options.directory = true,
                "logical" => options.logical = true,
                "physical" => options.logical = false,
                "backup" => options.backup = true,
                "suffix" => match inline.clone().or_else(|| next_value(&mut i)) {
                    Some(suffix) => options.suffix = suffix,
                    None => return Err("@@REQUIRES@@suffix".to_string()),
                },
                "target-directory" => match inline.clone().or_else(|| next_value(&mut i)) {
                    Some(directory) => options.target_directory = Some(directory),
                    None => return Err("@@REQUIRES@@target-directory".to_string()),
                },
                _ => return Err(format!("@@UNRECOGNIZED@@{}", name)),
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
                's' => options.symbolic = true,
                'f' => options.force = true,
                'i' => options.interactive = true,
                'v' => options.verbose = true,
                'n' => options.no_dereference = true,
                'r' => options.relative = true,
                'T' => options.no_target_directory = true,
                'd' | 'F' => options.directory = true,
                'L' => options.logical = true,
                'P' => options.logical = false,
                'b' => options.backup = true,
                't' => match attached(&letters, &mut index) {
                    Some(directory) => options.target_directory = Some(directory),
                    None => return Err("@@REQUIRES@@t".to_string()),
                },
                'S' => match attached(&letters, &mut index) {
                    Some(suffix) => options.suffix = suffix,
                    None => return Err("@@REQUIRES@@S".to_string()),
                },
                other => return Err(format!("@@INVALID@@{}", other)),
            }
        }
    }
    Ok((options, operands))
}

/// Whether an existing destination may be removed: only -f, -b and -n ask for
/// it, and -i asks the user.
fn confirm_removal(destination: &Path, options: &Options) -> bool {
    if !options.interactive {
        return options.force || options.backup || options.no_dereference;
    }
    eprint!(
        "ln: overwrite '{}'? ",
        destination.to_string_lossy()
    );
    let _ = io::stderr().flush();
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    answer.trim().eq_ignore_ascii_case("y") || answer.trim() == "yes"
}

/// Whether the destination is a symbolic link that points at a directory, which
/// is what -n is about.
fn is_symlink_to_dir(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => fs::metadata(path)
            .map(|target| target.is_dir())
            .unwrap_or(false),
        _ => false,
    }
}

fn make_link(target: &Path, destination: &Path, options: &Options) -> Result<bool, String> {
    let destination_exists = fs::symlink_metadata(destination).is_ok();
    // The user is only asked under -i; without it an existing destination is
    // simply refused.
    let declined = destination_exists
        && options.interactive
        && !confirm_removal(destination, options);
    let action = decide(
        destination_exists,
        is_symlink_to_dir(destination),
        declined,
        options,
    );
    match action {
        Action::Create => {}
        Action::Replace => {
            // -f and -b both take the existing file out of the way, the first
            // by renaming it aside.
            if options.backup {
                let name = backup_name(
                    &destination.to_string_lossy(),
                    &options.suffix,
                );
                if fs::rename(destination, PathBuf::from(name)).is_err() {
                    fs::remove_file(destination).map_err(|error| {
                        ln::failed_to_create_message(
                            options.symbolic,
                            &destination.to_string_lossy(),
                            &io_error_reason(&error),
                        )
                    })?;
                }
            } else {
                fs::remove_file(destination).map_err(|error| {
                    ln::failed_to_create_message(
                        options.symbolic,
                        &destination.to_string_lossy(),
                        &io_error_reason(&error),
                    )
                })?;
            }
        }
        Action::SkipDeclined => return Ok(false),
        Action::RefuseExists => {
            return Err(ln::file_exists_message(
                options.symbolic,
                &destination.to_string_lossy(),
            ))
        }
    }

    // A hard link needs the target to be there; a symbolic link may be text.
    if !options.symbolic && fs::metadata(target).is_err() {
        return Err(ln::cannot_stat_message(&target.to_string_lossy()));
    }

    let result = if options.symbolic {
        let text = if options.relative {
            relative_target(&target.to_string_lossy(), destination)
        } else {
            target.to_string_lossy().into_owned()
        };
        std::os::unix::fs::symlink(&text, destination)
    } else {
        fs::hard_link(target, destination)
    };
    match result {
        Ok(()) => {
            if options.verbose {
                println!(
                    "{}",
                    verbose_line(&destination.to_string_lossy(), &target.to_string_lossy())
                );
            }
            Ok(true)
        }
        Err(error) => Err(ln::failed_to_create_message(
            options.symbolic,
            &destination.to_string_lossy(),
            &io_error_reason(&error),
        )),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (options, operands) = match parse(&args) {
        Ok(parsed) => parsed,
        Err(message) => {
            if let Some(name) = message.strip_prefix("@@UNRECOGNIZED@@") {
                eprintln!("{}", ln::unrecognized_option_message(&format!("--{}", name)));
            } else if let Some(letter) = message.strip_prefix("@@INVALID@@") {
                eprintln!(
                    "{}",
                    ln::invalid_option_message(letter.chars().next().unwrap_or('?'))
                );
            } else if let Some(name) = message.strip_prefix("@@REQUIRES@@") {
                eprintln!("{}", ln::requires_argument_message(name));
            } else {
                eprintln!("{}", message);
                std::process::exit(1);
            }
            eprintln!("{}", ln::try_help_message());
            std::process::exit(1);
        }
    };

    // -t has to name a directory.
    if let Some(directory) = &options.target_directory {
        match fs::metadata(directory) {
            Ok(meta) if meta.is_dir() => {}
            _ => {
                // The reason is the one the directory itself gives.
                let reason = match fs::metadata(directory) {
                    Ok(_) => "Not a directory".to_string(),
                    Err(error) => io_error_reason(&error),
                };
                eprintln!("{}", ln::target_directory_message(directory, &reason));
                std::process::exit(1);
            }
        }
    }

    let last = operands.last();
    let last_is_dir = last
        .map(|path| fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false))
        .unwrap_or(false);

    let mut status = 0;
    match plan(&operands, last_is_dir, &options) {
        Plan::Invalid(Problem::MissingOperand) => {
            eprintln!("{}", ln::missing_operand_message());
            eprintln!("{}", ln::try_help_message());
            std::process::exit(1);
        }
        Plan::Invalid(Problem::ExtraOperand(operand)) => {
            eprintln!("{}", ln::extra_operand_message(&operand));
            eprintln!("{}", ln::try_help_message());
            std::process::exit(1);
        }
        Plan::Invalid(Problem::TargetNotADirectory(destination)) => {
            eprintln!("{}", ln::target_not_a_directory_message(&destination));
            std::process::exit(1);
        }
        Plan::Invalid(Problem::TargetDirectory(directory)) => {
            eprintln!(
                "{}",
                ln::target_directory_message(&directory, "No such file or directory")
            );
            std::process::exit(1);
        }
        Plan::Links(links) => {
            for link in links {
                match make_link(&link.target, &link.destination, &options) {
                    Ok(_) => {}
                    Err(message) => {
                        eprintln!("{}", message);
                        status = 1;
                    }
                }
            }
        }
    }
    std::process::exit(status);
}