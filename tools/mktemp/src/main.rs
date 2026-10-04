//! `mktemp(1)`: create a temporary file or directory.

use std::fs;
use std::io;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use mktemp::{
    directory_for, directory_mode, file_mode, fill_split, template, width, Problem, Split,
    Template, ALPHABET,
};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: mktemp [OPTION]... [TEMPLATE]

Create a temporary file or directory, safely, and print its name.  TEMPLATE must
contain at least 3 consecutive 'X's in last component.  If TEMPLATE is not
specified, use tmp.XXXXXXXXXX, and --tmpdir is implied.  Files are created
u+rw, and directories u+rwx, minus umask restrictions.

  -d, --directory          create a directory, not a file
  -u, --dry-run            do not create anything; merely print a name
  -q, --quiet              suppress diagnostics about file/dir-creation failure
      --suffix=SUFF        append SUFF to TEMPLATE
  -p DIR, --tmpdir[=DIR]   interpret TEMPLATE relative to DIR
  -t                       interpret TEMPLATE as a single file name component
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

/// Random characters for the Xs, drawn from the alphabet the manual implies.
fn random_characters(count: usize) -> String {
    let mut out = String::with_capacity(count);
    let mut state: u64;
    // Seed from the clock and the pid, which is enough for a name that only has
    // to be unique in one directory.
    // SAFETY: clock_gettime fills in the struct we own.
    unsafe {
        let mut times: libc::timespec = std::mem::zeroed();
        libc::clock_gettime(libc::CLOCK_REALTIME, &mut times);
        state = (times.tv_sec as u64) ^ ((times.tv_nsec as u64) << 17);
    }
    // SAFETY: getpid has no preconditions.
    state ^= unsafe { libc::getpid() } as u64;
    for _ in 0..count {
        // xorshift64, which is plenty for spreading the bits.
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out.push(ALPHABET[(state % ALPHABET.len() as u64) as usize] as char);
    }
    out
}

fn parse(args: &[String]) -> Result<mktemp::Options, String> {
    let mut options = mktemp::Options::default();
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
                    println!("mktemp (rusttool) {}", VERSION);
                    std::process::exit(0);
                }
                "directory" => options.directory = true,
                "dry-run" => options.dry_run = true,
                "quiet" => options.quiet = true,
                "suffix" => match inline {
                    Some(suffix) => options.suffix = Some(suffix),
                    None => return Err("@@REQUIRES@@suffix".to_string()),
                },
                // A bare --tmpdir means "use $TMPDIR".
                "tmpdir" => options.tmpdir = Some(inline),
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
                'd' => options.directory = true,
                'u' => options.dry_run = true,
                'q' => options.quiet = true,
                't' => options.single_component = true,
                // -p takes its directory as a separate argument, so a bare -p
                // is an error; --tmpdir is the one that means $TMPDIR.
                'p' => match take_value(&letters, &mut index) {
                    Some(directory) => options.tmpdir = Some(Some(directory)),
                    None => return Err("@@REQUIRES@@p".to_string()),
                },
                other => return Err(format!("@@INVALID@@{}", other)),
            }
        }
    }

    if operands.len() > 1 {
        return Err("@@TOO-MANY@@".to_string());
    }
    options.positional = operands.into_iter().next();
    Ok(options)
}

/// Try to create `name`, retrying with fresh random characters on a clash.
fn create(options: &mktemp::Options, name: &Path) -> io::Result<()> {
    if options.directory {
        fs::DirBuilder::new()
            .mode(directory_mode(current_umask()))
            .create(name)
    } else {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(file_mode(current_umask()))
            .open(name)
            .map(|_| ())
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse(&args) {
        Ok(options) => options,
        Err(message) => {
            // coreutils prints the diagnostic and, for a usage error, the Try
            // line rather than the whole help.
            let mut try_help = true;
            if let Some(name) = message.strip_prefix("@@UNRECOGNIZED@@") {
                eprintln!("{}", mktemp::unrecognized_option_message(&format!("--{}", name)));
            } else if let Some(letter) = message.strip_prefix("@@INVALID@@") {
                let letter = letter.chars().next().unwrap_or('?');
                eprintln!("{}", mktemp::invalid_option_message(letter));
            } else if let Some(letter) = message.strip_prefix("@@REQUIRES@@") {
                let letter = letter.chars().next().unwrap_or('?');
                eprintln!("{}", mktemp::requires_argument_message(letter));
            } else if message == "@@TOO-MANY@@" {
                eprintln!("{}", mktemp::too_many_templates_message());
            } else {
                eprintln!("{}", message);
                try_help = false;
            }
            if try_help {
                eprintln!("{}", mktemp::try_help_message());
            }
            std::process::exit(1);
        }
    };

    let template = match template(&options) {
        Ok(template) => template,
        Err(Problem::TooFewXs(text)) => {
            eprintln!("{}", mktemp::too_few_xs_message(&text));
            std::process::exit(1);
        }
        Err(Problem::AbsoluteWithTmpdir(text)) => {
            eprintln!("{}", mktemp::absolute_with_tmpdir_message(&text));
            std::process::exit(1);
        }
        Err(Problem::SlashesWithDashT(text)) => {
            eprintln!("{}", mktemp::slashes_with_t_message(&text));
            std::process::exit(1);
        }
        Err(Problem::SuffixHasSlash(suffix)) => {
            eprintln!("{}", mktemp::suffix_slash_message(&suffix));
            std::process::exit(1);
        }
    };

    let temporary = std::env::var("TMPDIR").ok();
    let (split, directory) = match template {
        // The documented default: tmp.XXXXXXXXXX in the temporary directory.
        Template::Implicit => (
            Split {
                prefix: "tmp.".into(),
                xs: 10,
                suffix: String::new(),
            },
            Some(PathBuf::from(
                directory_for(&options, temporary.as_deref()),
            )),
        ),
        Template::Explicit(split) => {
            // Without -p or -t the template keeps its own directory part, so
            // the prefix is split into the directory to create it in and the
            // name that stays behind.
            let mut split = split;
            let directory = match options.single_component || options.tmpdir.is_some() {
                true => Some(PathBuf::from(directory_for(&options, temporary.as_deref()))),
                false => match split.prefix.rfind('/') {
                    Some(index) => {
                        let head = split.prefix[..=index].to_string();
                        split.prefix = split.prefix[index + 1..].to_string();
                        Some(PathBuf::from(head))
                    }
                    None => None,
                },
            };
            (split, directory)
        }
    };

    let width = width(&split);
    if options.dry_run {
        let name = fill_split(&split, &random_characters(width));
        match &directory {
            Some(directory) if !directory.as_os_str().is_empty() => {
                println!("{}", directory.join(&name).display())
            }
            _ => println!("{}", name),
        }
        std::process::exit(0);
    }

    // The manual's promise is that the name is created safely, so retry until
    // an exclusive create succeeds.
    if let Some(directory) = &directory {
        if !directory.as_os_str().is_empty() && !directory.exists() {
            let error = io::Error::from_raw_os_error(libc::ENOENT);
            // The diagnostic names the template, which is the directory plus the
            // component that could not be created in it.
            let name = fill_split(&split, &random_characters(width));
            report_failure(
                &options,
                &directory.join(name),
                &io_error_reason(&error),
            );
        }
    }
    for _ in 0..100 {
        let name = fill_split(&split, &random_characters(width));
        let candidate = match &directory {
            Some(directory) if !directory.as_os_str().is_empty() => directory.join(&name),
            _ => PathBuf::from(&name),
        };
        match create(&options, &candidate) {
            Ok(()) => {
                println!("{}", candidate.display());
                std::process::exit(0);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                report_failure(&options, &candidate, &io_error_reason(&error));
            }
        }
    }
    let name = fill_split(&split, &random_characters(width));
    let candidate = match &directory {
        Some(directory) if !directory.as_os_str().is_empty() => directory.join(name),
        _ => PathBuf::from(name),
    };
    if !options.quiet {
        eprintln!(
            "{}",
            mktemp::cannot_create_message(&candidate.to_string_lossy(), "File exists")
        );
    }
    std::process::exit(1);
}

fn report_failure(options: &mktemp::Options, name: &Path, reason: &str) -> ! {
    if !options.quiet {
        eprintln!("{}", mktemp::cannot_create_message(&name.to_string_lossy(), reason));
    }
    std::process::exit(1)
}