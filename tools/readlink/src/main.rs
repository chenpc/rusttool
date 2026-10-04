//! `readlink(1)`: print resolved symbolic links or canonical file names.

use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use readlink::{decide, Action, Mode, Options};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: readlink [OPTION]... FILE...

Print value of a symbolic link or canonical file name

  -e, --canonicalize-existing   canonicalize by following every symlink, all
                                components must exist
  -f, --canonicalize            canonicalize by following every symlink, all
                                but the last component must exist
  -m, --canonicalize-missing    canonicalize without requirements on
                                components existence
  -n, --no-newline              do not output the trailing delimiter
  -q, --quiet                   suppress most error messages
  -s, --silent                  suppress most error messages
  -v, --verbose                 report error messages
  -z, --zero                    end each output line with NUL, not newline
      --help     display this help and exit
      --version  output version information and exit
";

/// Resolve a path by following every symlink in every component.
///
/// `require_last` decides whether the final component has to exist, which is
/// the difference between -f, -e and -m. The intermediate components are
/// resolved as far as they exist, which is what -m relies on.
fn canonicalize(
    path: &Path,
    require_last: bool,
    options_need_intermediates: bool,
) -> io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        PathBuf::from("/")
    } else {
        std::env::current_dir()?
    };
    let mut resolved = absolute;
    let components: Vec<Component> = path.components().collect();
    let last = components.len().saturating_sub(1);
    for (index, component) in components.iter().enumerate() {
        let is_last = index == last;
        match component {
            Component::RootDir | Component::Prefix(_) => {}
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Normal(name) => {
                resolved.push(name);
                // Follow the link, and keep following while it points at
                // another link.
                let mut guard = 0;
                loop {
                    let metadata = fs::symlink_metadata(&resolved);
                    match metadata {
                        Ok(meta) if meta.file_type().is_symlink() => {
                            let target = fs::read_link(&resolved)?;
                            resolved.pop();
                            if target.is_absolute() {
                                resolved = PathBuf::from("/");
                            }
                            for part in target.components() {
                                match part {
                                    Component::CurDir => {}
                                    other => resolved.push(other.as_os_str()),
                                }
                            }
                            guard += 1;
                            if guard > 40 {
                                return Err(io::Error::from_raw_os_error(libc::ELOOP));
                            }
                        }
                        _ => break,
                    }
                }
                // -e needs the last component as well; -f and -e need the
                // ones before it; -m needs nothing to exist.
                let needed = if is_last { require_last } else { options_need_intermediates };
                if needed && fs::symlink_metadata(&resolved).is_err() {
                    return Err(io::Error::from_raw_os_error(libc::ENOENT));
                }
            }
        }
    }
    Ok(resolved)
}

/// The canonical name, with the current directory spelled out unless the input
/// was already absolute.
fn canonical_name(path: &str, options: &Options) -> io::Result<String> {
    let require_last = options.mode.requires_last_component();
    let resolved = canonicalize(Path::new(path), require_last, options.mode.requires_all_components())?;
    if Path::new(path).is_absolute() {
        return Ok(resolved.to_string_lossy().into_owned());
    }
    // coreutils keeps a relative answer relative when it can, but the manual
    // describes a canonical name, which is absolute.
    Ok(resolved.to_string_lossy().into_owned())
}

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
        if let Some(long) = arg.strip_prefix("--") {
            let name = long.split_once('=').map(|(name, _)| name).unwrap_or(long);
            let spelled = arg.clone();
            match name {
                "help" => {
                    print!("{}", HELP);
                    std::process::exit(0);
                }
                "version" => {
                    println!("readlink (rusttool) {}", VERSION);
                    std::process::exit(0);
                }
                "canonicalize" => options.mode = Mode::Canonicalize,
                "canonicalize-existing" => options.mode = Mode::CanonicalizeExisting,
                "canonicalize-missing" => options.mode = Mode::CanonicalizeMissing,
                "no-newline" => options.no_newline = true,
                "zero" => options.zero = true,
                "quiet" | "silent" => options.verbose = false,
                "verbose" => options.verbose = true,
                _ => return Err(format!("@@UNRECOGNIZED@@{}", spelled)),
            }
            continue;
        }
        let letters: Vec<char> = arg.chars().skip(1).collect();
        for letter in letters {
            match letter {
                'f' => options.mode = Mode::Canonicalize,
                'e' => options.mode = Mode::CanonicalizeExisting,
                'm' => options.mode = Mode::CanonicalizeMissing,
                'n' => options.no_newline = true,
                'z' => options.zero = true,
                'q' | 's' => options.verbose = false,
                'v' => options.verbose = true,
                other => return Err(format!("@@INVALID@@{}", other)),
            }
        }
    }
    Ok((options, operands))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (options, operands) = match parse(&args) {
        Ok(parsed) => parsed,
        Err(message) => {
            if let Some(name) = message.strip_prefix("@@UNRECOGNIZED@@") {
                eprintln!("{}", readlink::unrecognized_option_message(name));
            } else if let Some(letter) = message.strip_prefix("@@INVALID@@") {
                eprintln!("{}", readlink::invalid_option_message(letter));
            } else {
                eprintln!("{}", message);
                std::process::exit(1);
            }
            eprintln!("{}", readlink::try_help_message());
            std::process::exit(1);
        }
    };

    if operands.is_empty() {
        eprintln!("{}", readlink::missing_operand_message());
        std::process::exit(1);
    }

    let mut status = 0;
    for operand in &operands {
        let path = Path::new(operand);
        let metadata = fs::symlink_metadata(path);
        let is_link = metadata.as_ref().map(|m| m.file_type().is_symlink()).unwrap_or(false);
        let target = if is_link {
            fs::read_link(path).ok().map(|p| p.to_string_lossy().into_owned())
        } else {
            None
        };
        let canonical = if options.mode == Mode::Target {
            None
        } else {
            canonical_name(operand, &options).ok()
        };

        match decide(is_link, target.as_deref(), canonical.as_deref(), options.mode) {
            Action::Print(text) => {
                let mut out = io::stdout();
                let _ = out.write_all(text.as_bytes());
                if let Some(terminator) = options.terminator() {
                    let _ = out.write_all(&[terminator]);
                }
                let _ = out.flush();
            }
            Action::NotASymbolicLink => {
                // Nothing is printed, and the status shows that it was not a
                // link; an error is reported only in verbose mode.
                if options.verbose {
                    let reason = match &metadata {
                        Ok(_) => "Invalid argument".to_string(),
                        Err(error) => io_error_reason(error),
                    };
                    eprintln!(
                        "{}",
                        format!(
                            "readlink: {}: {}",
                            operand, reason
                        )
                    );
                }
                status = 1;
            }
            Action::Missing => {
                if options.verbose {
                    eprintln!("{}", readlink::no_such_file_message(operand));
                }
                status = 1;
            }
        }
    }
    std::process::exit(status);
}