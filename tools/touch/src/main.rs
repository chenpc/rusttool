//! `touch(1)`: change file timestamps.

use std::fs;
use std::io;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;
use std::time::UNIX_EPOCH;

use touch::{
    created_mode, date_source, decide, now, parse_stamp, to_epoch, Action, Options, Source, Which,
};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: touch [OPTION]... FILE...

Update the access and modification times of each FILE to the current time.

A FILE argument that does not exist is created empty, unless -c or -h is
supplied.

  -a                         change only the access time
  -c, --no-create            do not create any files
  -d, --date=STRING          parse STRING and use it instead of current time
  -f                         (ignored)
  -h, --no-dereference       affect each symbolic link instead of any
                             referenced file
  -m                         change only the modification time
  -r, --reference=FILE       use this file's times instead of current time
  -t STAMP                   use [[CC]YY]MMDDhhmm[.ss] instead of current time
      --time=WORD            change the specified time: access, atime, modify,
                             mtime, or use: for both
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

fn current_umask() -> u32 {
    // SAFETY: umask only writes to the mask we pass in.
    unsafe {
        let mask = libc::umask(0o022);
        libc::umask(mask);
        mask
    }
}

/// The `(seconds, nanoseconds)` pair of a file's times.
fn times_of(metadata: &fs::Metadata) -> io::Result<(i64, i64)> {
    Ok((metadata.mtime(), metadata.mtime_nsec()))
}

/// Write the timestamps with `utimensat(2)`.
fn set_times(path: &Path, access: (i64, i64), modification: (i64, i64), no_follow: bool) -> io::Result<()> {
    let times = [
        libc::timespec {
            tv_sec: access.0 as libc::time_t,
            tv_nsec: access.1,
        },
        libc::timespec {
            tv_sec: modification.0 as libc::time_t,
            tv_nsec: modification.1,
        },
    ];
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let flags = if no_follow { libc::AT_SYMLINK_NOFOLLOW } else { 0 };
    // SAFETY: both pointers stay valid for the call.
    let rc = unsafe { libc::utimensat(libc::AT_FDCWD, c_path.as_ptr(), times.as_ptr(), flags) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// The times to write, given the options and the reference file.
fn resolve_times(options: &Options, reference: Option<(i64, i64)>) -> Result<(i64, i64), String> {
    let now_seconds = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_secs() as i64, d.subsec_nanos() as i64))
        .unwrap_or((0, 0));
    let chosen = match date_source(options, now()) {
        Ok(source) => source,
        Err(message) => return Err(message),
    };
    let seconds = match chosen {
        Source::Now => now_seconds,
        Source::Reference(_) => match reference {
            Some((seconds, nanoseconds)) => (seconds, nanoseconds),
            None => now_seconds,
        },
        Source::Parsed(date) => (to_epoch(date), date.nanosecond),
    };
    Ok(seconds)
}

fn parse(args: &[String]) -> Result<(Options, Vec<String>), String> {
    let mut options = Options::default();
    let mut operands: Vec<String> = Vec::new();
    let mut access_flag = false;
    let mut modification_flag = false;
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
                    println!("touch (rusttool) {}", VERSION);
                    std::process::exit(0);
                }
                "no-create" => options.no_create = true,
                "no-dereference" => options.no_dereference = true,
                "date" => match value(&mut i) {
                    Some(text) => options.date = Some(text),
                    None => return Err("touch: option '--date' requires an argument".to_string()),
                },
                "reference" => match value(&mut i) {
                    Some(file) => options.reference = Some(file),
                    None => {
                        return Err("touch: option '--reference' requires an argument".to_string())
                    }
                },
                "time" => match value(&mut i) {
                    Some(word) => {
                        options.which = options
                            .which
                            .with_word(&word)
                            .map_err(|word| format!("@@TIME-WORD@@{}", word))?
                    }
                    None => return Err("touch: option '--time' requires an argument".to_string()),
                },
                _ => return Err(format!("touch: unrecognized option '--{}'", name)),
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
                'a' => access_flag = true,
                'm' => modification_flag = true,
                'c' => options.no_create = true,
                'h' => options.no_dereference = true,
                'f' => options.force = true,
                'd' => match take_value(&letters, &mut index) {
                    Some(text) => options.date = Some(text),
                    None => return Err("touch: option requires an argument -- 'd'".to_string()),
                },
                'r' => match take_value(&letters, &mut index) {
                    Some(file) => options.reference = Some(file),
                    None => return Err("touch: option requires an argument -- 'r'".to_string()),
                },
                't' => match take_value(&letters, &mut index) {
                    Some(text) => options.stamp = Some(text),
                    None => return Err("touch: option requires an argument -- 't'".to_string()),
                },
                other => return Err(format!("touch: invalid option -- '{}'", other)),
            }
        }
    }

    // -a and -m together still mean both, whichever order they came in. They
    // only apply when one of them was given, so that --time keeps its meaning.
    if access_flag || modification_flag {
        options.which = match (access_flag, modification_flag) {
            (true, false) => Which::ACCESS,
            (false, true) => Which::MODIFICATION,
            _ => Which::BOTH,
        };
    }
    Ok((options, operands))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (options, operands) = match parse(&args) {
        Ok(parsed) => parsed,
        Err(message) => {
            if let Some(word) = message.strip_prefix("@@TIME-WORD@@") {
                for line in touch::invalid_time_word_lines(word) {
                    eprintln!("{}", line);
                }
                std::process::exit(1);
            }
            eprintln!("{}", message);
            eprint!("{}", HELP);
            std::process::exit(1);
        }
    };

    if operands.is_empty() {
        eprintln!("{}", touch::missing_operand_message());
        eprintln!("{}", "Try 'touch --help' for more information.");
        std::process::exit(1);
    }

    // The reference file is read once, as the manual implies it is the model
    // for every operand.
    let reference = match &options.reference {
        Some(file) => match fs::metadata(file).map(|meta| times_of(&meta)) {
            Ok(Ok(times)) => Some(times),
            _ => {
                eprintln!("{}", touch::reference_error_message(file));
                std::process::exit(1);
            }
        },
        None => None,
    };
    let times = match resolve_times(&options, reference) {
        Ok(times) => times,
        Err(_) => {
            // parse_stamp and parse_date report their own wording.
            if let Some(text) = &options.stamp {
                if let Err(message) = parse_stamp(text) {
                    let _ = message;
                    eprintln!("{}", touch::invalid_stamp_message(text));
                    std::process::exit(1);
                }
            }
            if let Some(text) = &options.date {
                eprintln!("{}", touch::invalid_date_message(text));
                std::process::exit(1);
            }
            std::process::exit(1);
        }
    };

    let mut status = 0;
    for operand in &operands {
        // A lone "-" means the file behind standard output.
        let path = if operand == "-" {
            Path::new("/proc/self/fd/1")
        } else {
            Path::new(operand)
        };
        let exists = fs::symlink_metadata(path).is_ok();
        match decide(exists, &options) {
            Action::Skip => continue,
            Action::Set => {}
        }
        if !exists {
            // Create it empty, with the mode open(2) would give.
            if let Err(error) = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .mode(created_mode(current_umask()))
                .open(path)
            {
                eprintln!(
                    "{}",
                    touch::cannot_touch_message(operand, &io_error_reason(&error))
                );
                status = 1;
                continue;
            }
        }
        // The timestamp the option did not ask for keeps its old value, which is
        // what -a and -m mean.
        let access = if options.which.access {
            times
        } else {
            existing_time(path, options.no_dereference, true).unwrap_or(times)
        };
        let modification = if options.which.modification {
            times
        } else {
            existing_time(path, options.no_dereference, false).unwrap_or(times)
        };
        if let Err(error) = set_times(path, access, modification, options.no_dereference) {
            eprintln!(
                "{}",
                touch::cannot_touch_message(operand, &io_error_reason(&error))
            );
            status = 1;
        }
    }
    std::process::exit(status);
}

/// Read one of a file's existing timestamps, so that -a and -m leave the other
/// one alone.
fn existing_time(path: &Path, no_follow: bool, access: bool) -> Option<(i64, i64)> {
    let metadata = if no_follow {
        fs::symlink_metadata(path).ok()?
    } else {
        fs::metadata(path).ok()?
    };
    if access {
        Some((metadata.atime(), metadata.atime_nsec()))
    } else {
        times_of(&metadata).ok()
    }
}