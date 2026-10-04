//! `cp(1)`: copy files and directories.
//!
//! ```text
//! cp [OPTION]... [-T] SOURCE DEST
//! cp [OPTION]... SOURCE... DIRECTORY
//! cp [OPTION]... -t DIRECTORY SOURCE...
//! ```

use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use cp::{
    backup_name, base_name, decide, plan, treat_source_as_link, wants_backup, Action, Attributes,
    Options, Plan, Problem, SymlinkPolicy, Update, When,
};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: cp [OPTION]... [-T] SOURCE DEST
       cp [OPTION]... SOURCE... DIRECTORY
       cp [OPTION]... -t DIRECTORY SOURCE...

Copy SOURCE to DEST, or multiple SOURCE(s) to DIRECTORY.

  -a, --archive              same as -dR --preserve=all
      --attributes-only      don't copy the file data, just the attributes
  -b                         like --backup but does not accept an argument
      --backup[=CONTROL]     make a backup of each existing destination file
      --copy-contents        copy contents of special files when recursive
  -d                         same as --no-dereference --preserve=links
      --debug                explain how a file is copied.  Implies -v
  -f, --force                if an existing destination file cannot be opened,
                             remove it and try again
  -i, --interactive          prompt before overwrite
  -H                         follow command-line symbolic links in SOURCE
  -l, --link                 hard link files instead of copying
  -L, --dereference          always follow symbolic links in SOURCE
  -n, --no-clobber           do not overwrite an existing file
  -p                         same as --preserve=mode,ownership,timestamps
      --parents              use full source file name under DIRECTORY
  -P, --no-dereference       never follow symbolic links in SOURCE
      --preserve[=ATTR_LIST] preserve the specified attributes
      --no-preserve=ATTR_LIST
                             don't preserve the specified attributes
  -R, -r, --recursive        copy directories recursively
      --reflink[=WHEN]       control clone/CoW copies
      --remove-destination   remove each existing destination file first
  -s, --symbolic-link        make symbolic links instead of copying
  -S, --suffix=SUFFIX        override the usual backup suffix
      --sparse=WHEN          control creation of sparse files
      --strip-trailing-slashes
                             remove any trailing slashes from each SOURCE
  -t, --target-directory=DIRECTORY
                             copy all SOURCE arguments into DIRECTORY
  -T, --no-target-directory  treat DEST as a normal file
  -u, --update               equivalent to --update[=older]
      --update[=UPDATE]      control which existing files are updated
  -v, --verbose              explain what is being done
  -x, --one-file-system      stay on this file system
      --help     display this help and exit
      --version  output version information and exit
";

/// The mode bits the umask hides from a newly created file.

/// `(seconds, nanoseconds)` of a metadata timestamp.
fn mtime_of(metadata: &fs::Metadata) -> Option<(i64, i64)> {
    Some((metadata.mtime(), metadata.mtime_nsec()))
}

/// Copy file attributes the manual asks for with `--preserve`.
fn apply_attributes(destination: &Path, source_meta: &fs::Metadata, options: &Options) -> bool {
    let attributes = options.effective().preserve;
    let mut ok = true;
    if attributes.mode {
        let mode = source_meta.permissions().mode() & 0o7777;
        if let Err(error) = fs::set_permissions(destination, fs::Permissions::from_mode(mode)) {
            eprintln!(
                "{}",
                cp::setting_permissions_error(
                    &destination.to_string_lossy(),
                    &io_error_reason(&error)
                )
            );
            ok = false;
        }
    }
    if attributes.timestamps {
        if set_times(destination, source_meta, false).is_err() {
            eprintln!(
                "{}",
                cp::preserving_times_error(
                    &destination.to_string_lossy(),
                    "Operation not permitted"
                )
            );
            ok = false;
        }
    }
    if attributes.ownership {
        // Only root can give a file away, so a failure here is not fatal.
        let uid = source_meta.uid();
        let gid = source_meta.gid();
        let name = std::ffi::CString::new(destination.to_string_lossy().as_bytes()).unwrap_or_default();
        // SAFETY: the path is a valid NUL-terminated C string and a failing
        // chown only means we were not allowed to change the owner.
        let rc = unsafe { libc::chown(name.as_ptr(), uid, gid) };
        if rc != 0 {
            ok = ok && true;
        }
    }
    ok
}

/// Set the access and modification times with `utimensat(2)`.
fn set_times(path: &Path, source_meta: &fs::Metadata, no_follow: bool) -> io::Result<()> {
    let times = [
        libc::timespec {
            tv_sec: source_meta.atime() as libc::time_t,
            tv_nsec: source_meta.atime_nsec() as i64,
        },
        libc::timespec {
            tv_sec: source_meta.mtime() as libc::time_t,
            tv_nsec: source_meta.mtime_nsec() as i64,
        },
    ];
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let flags = if no_follow { libc::AT_SYMLINK_NOFOLLOW } else { 0 };
    // SAFETY: both pointers are valid for the duration of the call.
    let rc = unsafe { libc::utimensat(libc::AT_FDCWD, c_path.as_ptr(), times.as_ptr(), flags) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// The wording coreutils uses for an errno value.
fn io_error_reason(error: &io::Error) -> String {
    let raw = error.raw_os_error().unwrap_or(libc::EIO);
    // SAFETY: strerror_r wants a buffer it may or may not fill.
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

/// Whether the destination may be written, honouring -i.
fn confirm_overwrite(destination: &Path, options: &Options) -> bool {
    if !options.effective().interactive {
        return true;
    }
    eprint!("{}", cp::prompt_message(&destination.to_string_lossy()));
    let _ = io::stderr().flush();
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    answer.trim().eq_ignore_ascii_case("y") || answer.trim() == "yes"
}

/// Move an existing destination out of the way, making the backup the manual
/// describes when one is asked for.

/// Copy one regular file.
/// Copy one regular file, creating the destination's parent if -p needs it.
/// The text -v prints for one copy.
fn report(options: &Options, source: &Path, destination: &Path) {
    if options.effective().verbose {
        println!(
            "'{}' -> '{}'",
            source.to_string_lossy(),
            destination.to_string_lossy()
        );
    }
}

/// The wording coreutils uses for a copy that could not be done.
fn wrap_copy_error(source: &Path, destination: &Path, error: &io::Error) -> io::Error {
    let reason = io_error_reason(error);
    let message = match error.raw_os_error() {
        Some(libc::EACCES) | Some(libc::EPERM) => {
            cp::open_error(&source.to_string_lossy(), &reason)
        }
        Some(libc::ENOENT) => cp::cannot_stat_message(&source.to_string_lossy()),
        Some(libc::EISDIR) => cp::is_a_directory_message(&source.to_string_lossy()),
        Some(libc::ENOTDIR) => cp::not_a_directory_message(&destination.to_string_lossy()),
        _ => cp::create_error(&destination.to_string_lossy(), &reason),
    };
    io::Error::new(error.kind(), message)
}

fn copy_file(source: &Path, destination: &Path) -> io::Result<()> {
    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!(
                        "{}",
                        cp::directory_create_error(&parent.to_string_lossy(), &io_error_reason(&e))
                    ),
                )
            })?;
        }
    }
    let mut input = fs::File::open(source)?;
    // Create or truncate, keeping whatever mode the destination already had so
    // that an existing file does not lose its permissions.
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(destination)?;
    io::copy(&mut input, &mut output)?;
    output.flush()?;
    Ok(())
}

/// Copy one symbolic link by recreating it, so the destination is a link too.
fn copy_symlink(source: &Path, destination: &Path) -> io::Result<()> {
    let target = fs::read_link(source)?;
    if destination.exists() {
        // Replacing a link means removing the link, never its referent.
        let meta = fs::symlink_metadata(destination)?;
        if meta.file_type().is_symlink() {
            fs::remove_file(destination)?;
        }
    }
    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }
    std::os::unix::fs::symlink(target, destination)
}

/// Copy one directory tree.
fn copy_tree(source: &Path, destination: &Path, options: &Options, on_command_line: bool) -> io::Result<()> {
    let source_meta = fs::symlink_metadata(source)?;
    if treat_source_as_link(source_meta.file_type().is_symlink(), on_command_line, options) {
        return copy_symlink(source, destination);
    }
    fs::create_dir_all(destination)?;
    let entries = fs::read_dir(source)?;
    // The order the directory gives is the order -v prints, so it is kept.
    let names: Vec<PathBuf> = entries.map(|entry| entry.map(|e| e.path())).collect::<io::Result<_>>()?;
    for entry in names {
        let name = base_name(&entry);
        let target = destination.join(&name);
        copy_one(&entry, &target, options, false, None)?;
    }
    // The directory's own attributes go last, so writing into it does not
    // reset the mode.
    apply_attributes(destination, &source_meta, options);
    Ok(())
}

/// Copy whatever `source` is onto `destination`.
fn copy_one(
    source: &Path,
    destination: &Path,
    options: &Options,
    on_command_line: bool,
    original_operand: Option<&str>,
) -> io::Result<()> {
    let effective = options.effective();
    let link_meta = fs::symlink_metadata(source);
    // -r on a directory that was spelled with a trailing slash is reported with
    // that spelling, which is what coreutils does.
    let original_operand = match original_operand {
        Some(text) if options.strip_trailing_slashes => Some(text),
        other => other,
    };
    // -r on a directory that was spelled with a trailing slash is reported with
    // that spelling, which is what coreutils does.
    let original_operand = match original_operand {
        Some(text) if options.strip_trailing_slashes => Some(text),
        other => other,
    };
    let source_meta = match link_meta {
        Ok(meta) => meta,
        // An unreadable source is named as one that cannot be opened.
        Err(error) if error.raw_os_error() == Some(libc::EACCES) => {
            return Err(io::Error::new(
                error.kind(),
                cp::open_error(&source.to_string_lossy(), &io_error_reason(&error)),
            ))
        }
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            let reason = io_error_reason(&error);
            let message = match error.raw_os_error() {
                Some(libc::EACCES) | Some(libc::EPERM) => {
                    cp::open_error(&source.to_string_lossy(), &reason)
                }
                _ => cp::cannot_stat_message(&source.to_string_lossy()),
            };
            return Err(io::Error::new(error.kind(), message));
        }
        // A source that follows to nothing is reported by name, which is what
        // coreutils does for a dangling symbolic link.
        Err(_) => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                cp::cannot_stat_message(&source.to_string_lossy()),
            ))
        }
    };
    let is_link = source_meta.file_type().is_symlink();
    let is_dir = source_meta.is_dir();

    if effective.symbolic {
        if let Some(parent) = destination.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        return std::os::unix::fs::symlink(source, destination);
    }
    if effective.hard_link {
        if destination.exists() {
            fs::remove_file(destination)?;
        }
        return fs::hard_link(source, destination);
    }

    let dest_meta = fs::symlink_metadata(destination).ok();
    let dest_exists = dest_meta.is_some();
    let dest_is_dir = dest_meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
    let dest_follows = dest_meta
        .as_ref()
        .map(|m| !m.file_type().is_symlink() && m.is_dir())
        .unwrap_or(false);

    let action = decide(
        is_dir,
        is_link,
        dest_exists,
        dest_follows,
        mtime_of(&source_meta),
        dest_meta.as_ref().and_then(mtime_of),
        options,
    );
    match action {
        Action::Skip | Action::SkipNotNewer | Action::SkipDeclined => return Ok(()),
        Action::RefuseDirectory => {
            // The operand is reported as the user wrote it, so
            // --strip-trailing-slashes still shows the slash that was given.
            // The manual's wording quotes the operand as the user wrote it, so a
            // trailing slash is part of the name that is reported.
            // The operand is quoted as the user wrote it, trailing slash and
            // all, which is what coreutils reports.
            let spelled = match original_operand {
                Some(text) => text.to_string(),
                None => source.to_string_lossy().into_owned(),
            };
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                cp::refuse_directory_message(&spelled),
            ));
        }
        Action::DestinationIsFile => {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "{}",
                    if dest_is_dir {
                        cp::cannot_overwrite_directory_message(&destination.to_string_lossy())
                    } else {
                        cp::cannot_overwrite_file_message(&destination.to_string_lossy())
                    }
                ),
            ))
        }
        _ => {}
    }

    if dest_exists && !confirm_overwrite(destination, options) {
        return Ok(());
    }
    // A backup is taken before anything is replaced, and it decides whether the
    // destination is still the file it was.
    let takes_backup =
        dest_exists && wants_backup(&options, std::env::var("VERSION_CONTROL").ok().as_deref());
    if takes_backup {
        let suffix = std::env::var("SIMPLE_BACKUP_SUFFIX").ok();
        let name = backup_name(&destination.to_string_lossy(), &options, suffix.as_deref());
        if let Err(error) = fs::rename(destination, PathBuf::from(name)) {
            return Err(io::Error::new(
                error.kind(),
                cp::unlink_error(&destination.to_string_lossy(), &io_error_reason(&error)),
            ));
        }
    }
    if effective.remove_destination && dest_exists {
        if is_link || !dest_is_dir {
            fs::remove_file(destination).ok();
        }
    }
    // After a backup or a forced removal the destination is a new file, so it
    // takes the source's mode.
    let dest_exists = dest_exists && !takes_backup && !effective.remove_destination;

    if is_link && treat_source_as_link(true, on_command_line, options) {
        copy_symlink(source, destination)?;
        report(options, source, destination);
        return Ok(());
    }

    if is_dir {
        // -v reports the directory before what is inside it.
        report(options, source, destination);
        copy_tree(source, destination, options, on_command_line)?;
        return Ok(());
    }
    {
        if let Some(parent) = destination.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        let source_meta = fs::metadata(source).map_err(|error| {
            io::Error::new(error.kind(), cp::cannot_stat_message(&source.to_string_lossy()))
        })?;
        if dest_exists {
            if let Some(meta) = &dest_meta {
                // Keep the destination's mode; -p replaces it afterwards.
                let _ = fs::set_permissions(destination, fs::Permissions::from_mode(meta.permissions().mode()));
            }
        } else {
            fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(destination)?;
            if !dest_exists {
                // A destination cp created gets the source's permissions even
                // without -p; one that was already there keeps its own.
                let wanted = if effective.preserve.mode {
                    cp::preserved_mode(source_meta.permissions().mode())
                } else {
                    cp::destination_mode(source_meta.permissions().mode())
                };
                fs::set_permissions(destination, fs::Permissions::from_mode(wanted))?;
            }
        }
        if action != Action::AttributesOnly {
            if let Err(error) = copy_file(source, destination) {
                // -f removes a destination that cannot be opened for writing and
                // tries again, as the manual describes.
                let denied = error.raw_os_error() == Some(libc::EACCES)
                    || error.raw_os_error() == Some(libc::EROFS);
                if !(effective.force && denied && dest_exists) {
                    return Err(wrap_copy_error(source, destination, &error));
                }
                fs::remove_file(destination).ok();
                copy_file(source, destination)?;
                // The destination is new now, so it takes the source's mode.
                let wanted = if effective.preserve.mode {
                    cp::preserved_mode(source_meta.permissions().mode())
                } else {
                    cp::destination_mode(source_meta.permissions().mode())
                };
                fs::set_permissions(destination, fs::Permissions::from_mode(wanted))?;
            }
        }
        if !dest_exists || effective.preserve.mode {
            // --attributes-only is about the attributes, and so is -p, so both
            // put the source's mode on the destination.
            let wanted = if effective.preserve.mode {
                cp::preserved_mode(source_meta.permissions().mode())
            } else {
                cp::destination_mode(source_meta.permissions().mode())
            };
            fs::set_permissions(destination, fs::Permissions::from_mode(wanted))?;
        }
        apply_attributes(destination, &source_meta, options);
    }

    report(options, source, destination);
    Ok(())
}

/// Parse the command line into options and operands.
fn parse(args: &[String]) -> Result<(Options, Vec<String>, bool), String> {
    let mut options = Options::default();
    // Only the short spelling warns, as the warning itself points out.
    let mut warned_no_clobber = false;
    let mut operands: Vec<String> = Vec::new();
    let mut i = 0usize;
    let mut no_more = false;

    // -L, -H and -P fight over one setting, so the last one on the command
    // line wins.
    let apply_symlinks = |options: &mut Options, flag: &str| {
        options.symlinks = SymlinkPolicy::parse(flag);
    };

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
                    println!("cp (rusttool) {}", VERSION);
                    std::process::exit(0);
                }
                "archive" => options.archive = true,
                "recursive" => options.recursive = true,
                "dereference" => apply_symlinks(&mut options, "-L"),
                "no-dereference" => apply_symlinks(&mut options, "-P"),
                "preserve" => {
                    let text = value(&mut i).unwrap_or_default();
                    options.preserve = Attributes::parse(&text).map_err(|bad| {
                        format!("cp: invalid attribute name '{}'", bad)
                    })?;
                }
                "no-preserve" => {
                    let text = value(&mut i).unwrap_or_default();
                    options.preserve = options
                        .preserve
                        .without(&text)
                        .map_err(|bad| format!("cp: invalid attribute name '{}'", bad))?;
                }
                "attributes-only" => options.attributes_only = true,
                "backup" => options.backup = Some(inline.clone().unwrap_or_else(|| "simple".to_string())),
                "suffix" => match value(&mut i) {
                    Some(suffix) => options.suffix = suffix,
                    None => return Err("@@REQUIRES@@suffix".to_string()),
                },
                "target-directory" => match value(&mut i) {
                    Some(directory) => options.target_directory = Some(directory),
                    None => return Err("@@REQUIRES@@target-directory".to_string()),
                },
                "no-target-directory" => options.no_target_directory = true,
                "parents" => options.parents = true,
                "update" => {
                    let text = inline.clone().unwrap_or_else(|| "older".to_string());
                    options.update = Update::parse(&text)
                        .map_err(|bad| cp::invalid_update_message(&bad))?;
                }
                "link" => options.hard_link = true,
                "symbolic-link" => options.symbolic = true,
                "verbose" => options.verbose = true,
                "force" => options.force = true,
                "interactive" => options.interactive = true,
                "no-clobber" => options.update = Update::None,
                "one-file-system" => options.one_file_system = true,
                "strip-trailing-slashes" => options.strip_trailing_slashes = true,
                "remove-destination" => options.remove_destination = true,
                "copy-contents" => options.copy_contents = true,
                "debug" => options.debug = true,
                "sparse" => {
                    let text = inline.clone().unwrap_or_else(|| "always".to_string());
                    options.sparse = When::parse(&text)
                        .map_err(|bad| cp::invalid_when_message(&bad))?;
                }
                "reflink" => {
                    let text = inline.clone().unwrap_or_default();
                    options.reflink = When::parse(&text)
                        .map_err(|bad| format!("cp: invalid argument '{}' for '--reflink' option", bad))?;
                }
                "preserve-mode" => options.preserve.mode = true,
                _ => return Err(format!("@@UNRECOGNIZED@@{}", name)),
            }
            continue;
        }

        // A short option cluster, where a value may be attached (-tDIR) or be
        // the next argument (-t DIR).
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
                'a' => options.archive = true,
                'r' | 'R' => options.recursive = true,
                'v' => options.verbose = true,
                'f' => options.force = true,
                'i' => options.interactive = true,
                'n' => {
                    options.update = Update::None;
                    warned_no_clobber = true;
                }
                'u' => options.update = Update::Older,
                'p' => options.preserve = Attributes::mode_ownership_timestamps(),
                'd' => apply_symlinks(&mut options, "-d"),
                'H' => apply_symlinks(&mut options, "-H"),
                'L' => apply_symlinks(&mut options, "-L"),
                'P' => apply_symlinks(&mut options, "-P"),
                'l' => options.hard_link = true,
                's' => options.symbolic = true,
                'x' => options.one_file_system = true,
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
    Ok((options, operands, warned_no_clobber))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (options, operands, warned_no_clobber) = match parse(&args) {
        Ok(parsed) => parsed,
        Err(message) => {
            // coreutils prints the diagnostic and the Try line, not the help.
            if let Some(name) = message.strip_prefix("@@UNRECOGNIZED@@") {
                eprintln!("{}", cp::unrecognized_option_message(&format!("--{}", name)));
            } else if let Some(letter) = message.strip_prefix("@@INVALID@@") {
                eprintln!(
                    "{}",
                    cp::invalid_option_message(letter.chars().next().unwrap_or('?'))
                );
            } else if let Some(name) = message.strip_prefix("@@REQUIRES@@") {
                eprintln!("{}", cp::requires_argument_message(name));
            } else {
                eprintln!("{}", message);
                std::process::exit(1);
            }
            eprintln!("{}", cp::try_help_message());
            std::process::exit(1);
        }
    };
    if warned_no_clobber {
        eprintln!("{}", cp::NO_CLOBBER_WARNING);
    }
    let effective = options.effective();

    if let Some(flag) = effective.conflicting_recursion() {
        eprintln!("{}", cp::recursive_conflict_message(flag));
        std::process::exit(1);
    }

    // Whether the last operand is an existing directory decides between "the
    // name of the copy" and "a directory to copy into".
    let last = operands.last();
    let last_is_dir = last
        .map(|path| fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false))
        .unwrap_or(false);
    if effective.parents && operands.len() >= 2 {
        let destination = operands[operands.len() - 1].clone();
        if !fs::metadata(&destination).map(|m| m.is_dir()).unwrap_or(false) {
            eprintln!("{}", cp::parents_needs_directory_message());
            eprintln!("{}", cp::try_help_message());
            std::process::exit(1);
        }
    }
    if let Some(path) = &effective.target_directory {
        match fs::metadata(path) {
            Ok(meta) if !meta.is_dir() => {
                eprintln!("{}", cp::target_directory_message(path, "Not a directory"));
                std::process::exit(1);
            }
            Err(_) => {
                eprintln!(
                    "{}",
                    format!(
                        "cp: failed to access '{}': No such file or directory",
                        path
                    )
                );
                std::process::exit(1);
            }
            _ => {}
        }
    }

    let mut status = 0;
    match plan(&operands, last_is_dir, &options) {
        Plan::Invalid(Problem::TargetUnavailable { destination, reason }) => {
            let reason = if reason.is_empty() {
                let (exists, is_dir) = match fs::metadata(&destination) {
                    Ok(meta) => (true, meta.is_dir()),
                    Err(error) => {
                        if error.kind() == io::ErrorKind::NotFound {
                            (false, false)
                        } else {
                            (true, false)
                        }
                    }
                };
                cp::target_reason(exists, is_dir).to_string()
            } else {
                reason
            };
            eprintln!("{}", cp::target_unavailable_message(&destination, &reason));
            std::process::exit(1);
        }
        Plan::Invalid(Problem::TargetDirectoryUnavailable { directory, reason }) => {
            eprintln!("{}", cp::target_directory_message(&directory, &reason));
            std::process::exit(1);
        }
        Plan::Invalid(Problem::ExtraOperand(operand)) => {
            eprintln!("{}", cp::extra_operand_message(&operand));
            eprintln!("{}", cp::try_help_message());
            std::process::exit(1);
        }
        Plan::Invalid(Problem::ParentsNeedsDirectory) => {
            eprintln!("{}", cp::parents_needs_directory_message());
            eprintln!("{}", cp::try_help_message());
            std::process::exit(1);
        }
        Plan::Invalid(Problem::MissingOperand) => {
            match operands.last() {
                Some(operand) => eprintln!("{}", cp::missing_destination_message(operand)),
                None => eprintln!("{}", cp::missing_operand_message()),
            }
            eprintln!("{}", cp::try_help_message());
            std::process::exit(1);
        }
        Plan::Stdin(destination) => {
            let mut buffer = Vec::new();
            if let Err(error) = io::stdin().read_to_end(&mut buffer) {
                eprintln!("cp: {}", error);
                std::process::exit(1);
            }
            if let Err(error) = fs::write(&destination, &buffer) {
                eprintln!("{}", cp::create_error(&destination.to_string_lossy(), &io_error_reason(&error)));
                status = 1;
            }
        }
        Plan::Pairs(pairs) => {
            for pair in pairs {
                let source = pair.source.clone();
                let destination = pair.destination.clone();
                // Copying a file onto itself is refused before anything is
                // written, which is what keeps `cp a a` from truncating it.
                if let (Ok(source_meta), Ok(destination_meta)) = (
                    fs::metadata(&source),
                    fs::metadata(&destination),
                ) {
                    if source_meta.dev() == destination_meta.dev()
                        && source_meta.ino() == destination_meta.ino()
                    {
                        eprintln!(
                            "{}",
                            cp::same_file_message(
                                &pair.source_name(),
                                &pair.destination_name()
                            )
                        );
                        status = 1;
                        continue;
                    }
                }
                if let Err(error) =
                    copy_one(&source, &destination, &options, true, Some(&pair.spelled))
                {
                    let text = error.to_string();
                    eprintln!("{}", text);
                    status = 1;
                    continue;
                }
            }
        }
    }
    std::process::exit(status);
}