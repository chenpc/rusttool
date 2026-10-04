//! `mv(1)`: move (rename) files.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use mv::{
    backup_name, decide, plan, target_reason, wants_backup, Action, Options, Overwrite, Pair,
    Plan, Problem, Update,
};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: mv [OPTION]... [-T] SOURCE DEST
       mv [OPTION]... SOURCE... DIRECTORY
       mv [OPTION]... -t DIRECTORY SOURCE...

Rename SOURCE to DEST, or move SOURCE(s) to DIRECTORY.

      --backup[=CONTROL]     make a backup of each existing destination file
  -b                         like --backup but does not accept an argument
      --debug                explain how a file is copied.  Implies -v
  -f, --force                do not prompt before overwriting
  -i, --interactive          prompt before overwrite
  -n, --no-clobber           do not overwrite an existing file
      --no-copy              do not copy if renaming fails
  -S, --suffix=SUFFIX        override the usual backup suffix
  -t, --target-directory=DIRECTORY
                             move all SOURCE arguments into DIRECTORY
  -T, --no-target-directory  treat DEST as a normal file
  -u, --update               equivalent to --update[=older]
      --update[=UPDATE]      control which existing files are updated
  -v, --verbose              explain what is being done
      --strip-trailing-slashes
                             remove any trailing slashes from each SOURCE
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

fn is_empty_dir(path: &Path) -> bool {
    fs::read_dir(path)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false)
}

fn confirm_overwrite(destination: &Path, options: &Options) -> bool {
    if options.overwrite != Overwrite::Interactive {
        return true;
    }
    eprint!("{}", mv::prompt_message(&destination.to_string_lossy()));
    let _ = io::stderr().flush();
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    answer.trim().eq_ignore_ascii_case("y") || answer.trim() == "yes"
}

/// Move the contents of a directory into another one, the way `mv dir/* dest`
/// and `mv srcdir destdir` have to when the destination already exists.
fn move_into_directory(source: &Path, destination: &Path, options: &Options) -> io::Result<bool> {
    // mv srcdir destdir where destdir/srcdir does not exist renames; where it
    // does, the contents move into it.
    let nested = destination.join(mv::base_name(source));
    if fs::symlink_metadata(&nested).is_ok() {
        return move_tree_contents(source, &nested, options);
    }
    Ok(false)
}

fn move_tree_contents(source: &Path, destination: &Path, options: &Options) -> io::Result<bool> {
    let mut names: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(source)? {
        names.push(entry?.path());
    }
    names.sort();
    for entry in names {
        let target = destination.join(mv::base_name(&entry));
        move_one(&entry, &target, options)?;
    }
    fs::remove_dir(source)?;
    Ok(true)
}

/// Move one entry, choosing between a rename and a copy plus an unlink.
fn move_one(source: &Path, destination: &Path, options: &Options) -> io::Result<bool> {
    let source_meta = fs::symlink_metadata(source)?;
    let source_is_dir = source_meta.is_dir() && !source_meta.file_type().is_symlink();

    // A rename is enough whenever both ends are on one filesystem.
    match fs::rename(source, destination) {
        Ok(()) => return Ok(true),
        Err(error) => {
            if options.no_copy {
                if error.kind() == io::ErrorKind::CrossesDevices
                    || error.raw_os_error() == Some(libc::EXDEV)
                {
                    return Err(io::Error::new(
                        io::ErrorKind::CrossesDevices,
                        mv::cross_device_message(
                            &source.to_string_lossy(),
                            &destination.to_string_lossy(),
                        ),
                    ));
                }
            }
        }
    }

    if source_is_dir {
        if move_into_directory(source, destination, options)? {
            return Ok(true);
        }
        copy_then_remove(source, destination, options, true)?;
    } else {
        copy_then_remove(source, destination, options, false)?;
    }
    Ok(true)
}

/// The fallback for a rename that cannot be done: copy, then unlink.
fn copy_then_remove(source: &Path, destination: &Path, options: &Options, directory: bool) -> io::Result<()> {
    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }
    if directory {
        copy_tree(source, destination, options)?;
        fs::remove_dir_all(source)?;
    } else {
        let source_meta = fs::metadata(source)?;
        if source_meta.file_type().is_symlink() {
            let target = fs::read_link(source)?;
            fs::remove_file(destination).ok();
            std::os::unix::fs::symlink(target, destination)?;
            fs::remove_file(source)?;
            return Ok(());
        }
        fs::copy(source, destination)?;
        fs::set_permissions(destination, source_meta.permissions())?;
        fs::remove_file(source)?;
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path, _options: &Options) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    let mut names: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(source)? {
        names.push(entry?.path());
    }
    names.sort();
    for entry in names {
        let target = destination.join(mv::base_name(&entry));
        let meta = fs::symlink_metadata(&entry)?;
        if meta.file_type().is_symlink() {
            let link = fs::read_link(&entry)?;
            std::os::unix::fs::symlink(link, &target)?;
        } else if meta.is_dir() {
            copy_tree(&entry, &target, _options)?;
        } else {
            fs::copy(&entry, &target)?;
        }
    }
    let meta = fs::metadata(source)?;
    fs::set_permissions(destination, meta.permissions())?;
    Ok(())
}

fn mtime_of(meta: &fs::Metadata) -> Option<(i64, i64)> {
    Some((meta.mtime(), meta.mtime_nsec()))
}

/// Move one planned pair, deciding what to do from what the filesystem says.
fn perform(pair: &Pair, options: &Options) -> Result<bool, String> {
    let source = &pair.source;
    let destination = &pair.destination;

    let source_meta = match fs::symlink_metadata(source) {
        Ok(meta) => meta,
        Err(_) => {
            return Err(format!(
                "{}",
                mv::missing_source_message(&source.to_string_lossy())
            ))
        }
    };
    let dest_meta = fs::symlink_metadata(destination).ok();
    let dest_exists = dest_meta.is_some();
    let dest_is_dir = dest_meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
    let dest_is_empty = dest_is_dir && is_empty_dir(destination);
    // With -T the destination is a plain name, so a directory there is refused
    // before anything is moved.
    if options.no_target_directory && dest_is_dir {
        return Err(mv::overwrite_directory_message(&destination.to_string_lossy()));
    }
    let same_device = dest_meta
        .as_ref()
        .map(|m| m.dev() == source_meta.dev())
        .unwrap_or(true);

    // Moving something onto itself, through a different name, changes nothing.
    if dest_exists && source_meta.dev() == dest_meta.as_ref().unwrap().dev()
        && source_meta.ino() == dest_meta.as_ref().unwrap().ino()
    {
        return Err(mv::same_file_message(
            &source.to_string_lossy(),
            &destination.to_string_lossy(),
        ));
    }

    let source_is_dir = source_meta.is_dir() && !source_meta.file_type().is_symlink();
    let action = decide(
        source_is_dir,
        dest_exists,
        dest_is_dir,
        dest_is_empty,
        same_device,
        mtime_of(&source_meta),
        dest_meta.as_ref().and_then(mtime_of),
        options,
    );
    match action {
        Action::Skip | Action::SkipNotNewer | Action::SkipDeclined => return Ok(false),
        Action::NotReplacing => {
            return Err(mv::not_replacing_message(&destination.to_string_lossy()))
        }
        Action::TargetNotADirectory => {
            return Err(mv::target_not_a_directory_message(
                &destination.to_string_lossy(),
            ))
        }
        Action::OverwriteNonDirectory => {
            return Err(mv::overwrite_non_directory_message(
                &destination.to_string_lossy(),
                &source.to_string_lossy(),
            ))
        }
        Action::RefuseNonEmptyDirectory => {
            return Err(if dest_is_dir && !source_is_dir {
                mv::cannot_overwrite_directory_message(&destination.to_string_lossy())
            } else {
                mv::directory_not_empty_message(&destination.to_string_lossy())
            })
        }
        Action::RefuseCrossDevice => {
            return Err(mv::cross_device_message(
                &source.to_string_lossy(),
                &destination.to_string_lossy(),
            ))
        }
        Action::OverwriteConfirmed => {
            if !confirm_overwrite(destination, options) {
                return Ok(false);
            }
        }
        Action::Move => {}
    }

    // A backup is taken before anything is replaced.
    if dest_exists && wants_backup(&options, std::env::var("VERSION_CONTROL").ok().as_deref()) {
        let suffix = std::env::var("SIMPLE_BACKUP_SUFFIX").ok();
        let name = backup_name(&destination.to_string_lossy(), options, suffix.as_deref());
        if let Err(error) = fs::rename(destination, PathBuf::from(name)) {
            return Err(mv::cannot_move_message(
                &source.to_string_lossy(),
                &destination.to_string_lossy(),
                &io_error_reason(&error),
            ));
        }
    }

    match move_one(source, destination, options) {
        Ok(_) => {
            if options.effective().verbose {
                println!("{}", pair.verbose_line());
            }
            Ok(true)
        }
        Err(error) => {
            // A rename that cannot be done is reported with the reason, which is
            // what coreutils words it.
            let text = error.to_string();
            let denied = error.raw_os_error() == Some(libc::EACCES)
                || error.raw_os_error() == Some(libc::ENOENT);
            if denied {
                return Err(mv::cannot_move_message(
                    &source.to_string_lossy(),
                    &destination.to_string_lossy(),
                    &io_error_reason(&error),
                ));
            }
            Err(text)
        }
    }
}

fn parse(args: &[String]) -> Result<(Options, Vec<String>), String> {
    let mut options = Options::default();
    let mut operands: Vec<String> = Vec::new();
    // -i, -f and -n all write the same setting, and the last one wins.
    let mut overwrite_flags: Vec<Overwrite> = Vec::new();
    let mut i = 0usize;
    let mut no_more = false;

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
            let (name, inline) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value.to_string())),
                None => (long, None),
            };
            let value = |i: &mut usize| -> Option<String> {
                match inline.clone() {
                    Some(value) => Some(value),
                    None => {
                        if *i < args.len() {
                            let value = args[*i].clone();
                            *i += 1;
                            Some(value)
                        } else {
                            None
                        }
                    }
                }
            };
            match name {
                "help" => {
                    print!("{}", HELP);
                    std::process::exit(0);
                }
                "version" => {
                    println!("mv (rusttool) {}", VERSION);
                    std::process::exit(0);
                }
                "force" => overwrite_flags.push(Overwrite::Force),
                "interactive" => overwrite_flags.push(Overwrite::Interactive),
                "no-clobber" => overwrite_flags.push(Overwrite::NoClobber),
                "verbose" => options.verbose = true,
                "debug" => options.debug = true,
                "no-copy" => options.no_copy = true,
                "backup" => {
                    options.backup = Some(inline.clone().unwrap_or_else(|| "simple".to_string()))
                }
                "suffix" => match value(&mut i) {
                    Some(suffix) => options.suffix = suffix,
                    None => return Err("@@REQUIRES@@suffix".to_string()),
                },
                "target-directory" => match value(&mut i) {
                    Some(directory) => options.target_directory = Some(directory),
                    None => return Err("@@REQUIRES@@target-directory".to_string()),
                },
                "no-target-directory" => options.no_target_directory = true,
                "strip-trailing-slashes" => options.strip_trailing_slashes = true,
                "update" => {
                    let text = inline.clone().unwrap_or_else(|| "older".to_string());
                    options.update = Update::parse(&text)
                        .map_err(|bad| mv::invalid_update_message(&bad))?;
                }
                _ => return Err(format!("@@UNRECOGNIZED@@{}", name)),
            }
            continue;
        }

        let letters: Vec<char> = arg.chars().skip(1).collect();
        let mut index = 0usize;
        while index < letters.len() {
            let letter = letters[index];
            index += 1;
            let mut take_value = |letters: &[char], index: &mut usize| -> Option<String> {
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
                'f' => overwrite_flags.push(Overwrite::Force),
                'i' => overwrite_flags.push(Overwrite::Interactive),
                'n' => overwrite_flags.push(Overwrite::NoClobber),
                'v' => options.verbose = true,
                'u' => options.update = Update::Older,
                'b' => options.backup = Some("simple".to_string()),
                'T' => options.no_target_directory = true,
                't' => match take_value(&letters, &mut index) {
                    Some(directory) => options.target_directory = Some(directory),
                    None => return Err("@@REQUIRES@@t".to_string()),
                },
                'S' => match take_value(&letters, &mut index) {
                    Some(suffix) => options.suffix = suffix,
                    None => return Err("@@REQUIRES@@S".to_string()),
                },
                'Z' => {}
                other => return Err(format!("@@INVALID@@{}", other)),
            }
        }
    }
    options.overwrite = Overwrite::resolve(overwrite_flags.into_iter());
    Ok((options, operands))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (options, operands) = match parse(&args) {
        Ok(parsed) => parsed,
        Err(message) => {
            if let Some(name) = message.strip_prefix("@@UNRECOGNIZED@@") {
                eprintln!("{}", mv::unrecognized_option_message(&format!("--{}", name)));
            } else if let Some(letter) = message.strip_prefix("@@INVALID@@") {
                eprintln!(
                    "{}",
                    mv::invalid_option_message(letter.chars().next().unwrap_or('?'))
                );
            } else if let Some(name) = message.strip_prefix("@@REQUIRES@@") {
                eprintln!(
                    "{}",
                    mv::requires_argument_message(name.chars().next().unwrap_or('?'))
                );
            } else {
                eprintln!("{}", message);
                std::process::exit(1);
            }
            eprintln!("{}", mv::try_help_message());
            std::process::exit(1);
        }
    };

    // -t names a directory that has to be there.
    if let Some(directory) = &options.target_directory {
        match fs::metadata(directory) {
            Ok(meta) if meta.is_dir() => {}
            _ => {
                eprintln!("{}", mv::target_directory_missing_message(directory));
                std::process::exit(1);
            }
        }
    }

    // One operand leaves mv without a destination.
    if operands.len() == 1 {
        eprintln!("{}", mv::missing_destination_message(&operands[0]));
        eprintln!("{}", mv::try_help_message());
        std::process::exit(1);
    }

    let last = operands.last();
    let last_is_dir = last
        .map(|path| fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false))
        .unwrap_or(false);

    let mut status = 0;
    match plan(&operands, last_is_dir, &options) {
        Plan::Invalid(Problem::MissingOperand) => {
            eprintln!("{}", mv::missing_operand_message());
            std::process::exit(1);
        }
        Plan::Invalid(Problem::ExtraOperand(operand)) => {
            eprintln!("{}", mv::extra_operand_message(&operand));
            eprintln!("{}", mv::try_help_message());
            std::process::exit(1);
        }
        Plan::Invalid(Problem::TargetUnavailable { destination, reason }) => {
            let reason = if reason.is_empty() {
                let (exists, is_dir) = match fs::metadata(&destination) {
                    Ok(meta) => (true, meta.is_dir()),
                    Err(error) => (error.kind() != io::ErrorKind::NotFound, false),
                };
                target_reason(exists, is_dir).to_string()
            } else {
                reason
            };
            eprintln!("{}", mv::target_unavailable_message(&destination, &reason));
            std::process::exit(1);
        }
        Plan::Invalid(Problem::TargetDirectoryUnavailable { directory, reason }) => {
            eprintln!("{}", mv::target_directory_message(&directory, &reason));
            std::process::exit(1);
        }
        Plan::Invalid(Problem::MissingSource(_)) => {}
        Plan::Pairs(pairs) => {
            for pair in pairs {
                match perform(&pair, &options) {
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