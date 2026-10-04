//! `tee(1)`: copy standard input to each FILE, and to standard output.

use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;

use tee::{created_mode, react, write_error, Options, OutputError, Reaction};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: tee [OPTION]... [FILE]...

Copy standard input to each FILE, and also to standard output.

  -a, --append              append to the given FILEs, do not overwrite
  -i, --ignore-interrupts   ignore interrupt signals
  -p                        operate in a more appropriate MODE with pipes
      --output-error[=MODE] set behavior on write error
      --help     display this help and exit
      --version  output version information and exit

MODE determines behavior with write errors on the outputs:
  warn          diagnose errors writing to any output
  warn-nopipe   diagnose errors writing to any output not a pipe
  exit          exit on error writing to any output
  exit-nopipe   exit on error writing to any output not a pipe
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

/// Whether a descriptor is a pipe or a socket, which is what the `nopipe`
/// modes ask about: coreutils treats those as pipes and everything else, a
/// terminal or a regular file included, as not.
fn is_pipe(fd: i32) -> bool {
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: fstat fills in the stat buffer we own.
    if unsafe { libc::fstat(fd, &mut stat) } != 0 {
        return false;
    }
    let mode = stat.st_mode & libc::S_IFMT;
    mode == libc::S_IFIFO || mode == libc::S_IFSOCK
}

fn parse(args: &[String]) -> Result<(Options, Vec<String>), String> {
    let mut options = Options::default();
    let mut files: Vec<String> = Vec::new();
    let mut no_more = false;
    let mut explicit_mode = false;
    let mut i = 0usize;

    while i < args.len() {
        let arg = args[i].clone();
        i += 1;
        if no_more || arg == "-" || !arg.starts_with('-') {
            files.push(arg);
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
                    println!("tee (rusttool) {}", VERSION);
                    std::process::exit(0);
                }
                "append" => options.append = true,
                "ignore-interrupts" => options.ignore_interrupts = true,
                "output-error" => {
                    options.output_error = OutputError::parse(&inline.clone().unwrap_or_default())
                        .map_err(|mode| format!("@@MODE@@{}", mode))?;
                    explicit_mode = true;
                }
                _ => return Err(format!("@@UNRECOGNIZED@@{}", name)),
            }
            continue;
        }
        let letters: Vec<char> = arg.chars().skip(1).collect();
        for letter in letters {
            match letter {
                'a' => options.append = true,
                'i' => options.ignore_interrupts = true,
                'p' => options.pipe_mode = true,
                other => return Err(format!("@@INVALID@@{}", other)),
            }
        }
    }
    // -p only sets the default, so it does not override an explicit mode.
    if explicit_mode {
        options.pipe_mode = false;
    }
    Ok((options, files))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (options, files) = match parse(&args) {
        Ok(parsed) => parsed,
        Err(message) => {
            if let Some(name) = message.strip_prefix("@@UNRECOGNIZED@@") {
                eprintln!("{}", tee::unrecognized_option_message(&format!("--{}", name)));
            } else if let Some(letter) = message.strip_prefix("@@INVALID@@") {
                eprintln!(
                    "{}",
                    tee::invalid_option_message(letter.chars().next().unwrap_or('?'))
                );
            } else if let Some(mode) = message.strip_prefix("@@MODE@@") {
                for line in tee::invalid_mode_lines(mode) {
                    eprintln!("{}", line);
                }
                std::process::exit(1);
            } else {
                eprintln!("{}", message);
                std::process::exit(1);
            }
            eprintln!("{}", tee::try_help_message());
            std::process::exit(1);
        }
    };
    let effective = options.effective();

    if effective.ignore_interrupts {
        // SAFETY: SIG_IGN is a plain function pointer constant.
        unsafe {
            libc::signal(libc::SIGINT, libc::SIG_IGN);
        }
    }

    let mut outputs: Vec<(String, Box<dyn Write>)> = Vec::new();
    let mut status = 0;
    for file in &files {
        let mut open = OpenOptions::new();
        open.write(true).create(true);
        if effective.append {
            open.append(true);
        } else {
            open.truncate(true);
        }
        if !effective.append && !std::path::Path::new(file).exists() {
            // A new file gets the mode open(2) would give it.
            open.mode(created_mode(current_umask()));
        }
        match open.open(file) {
            Ok(handle) => outputs.push((file.clone(), Box::new(handle))),
            Err(error) => {
                // The manual's warn behaviour: the file is reported and the
                // other outputs, standard output included, still get the data.
                eprintln!("{}", write_error(file, &io_error_reason(&error)));
                status = 1;
            }
        }
    }

    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    let stdout_fd = stdout.as_raw_fd();
    let stdout_is_pipe = is_pipe(stdout_fd);

    let mut buffer = [0u8; 64 * 1024];
    let mut input = io::stdin();
    loop {
        let read = match input.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                eprintln!("tee: standard input: {}", io_error_reason(&error));
                status = 1;
                break;
            }
        };

        // Standard output first, then the files, and a failure on either is
        // judged by the mode the manual describes.
        if let Err(error) = stdout.write_all(&buffer[..read]) {
            let _ = stdout.flush();
            match react(stdout_is_pipe, &options) {
                Reaction::Ignore => {}
                Reaction::Warn => {
                    eprintln!("{}", tee::stdout_error(&io_error_reason(&error)));
                    status = 1;
                }
                Reaction::WarnAndExit => {
                    eprintln!("{}", tee::stdout_error(&io_error_reason(&error)));
                    std::process::exit(1);
                }
            }
            break;
        }

        // Every file is written, even after one has failed, unless the mode
        // says to stop.
        for (name, handle) in outputs.iter_mut() {
            if let Err(error) = handle.write_all(&buffer[..read]) {
                match react(false, &options) {
                    Reaction::Ignore => {}
                    Reaction::Warn => {
                        eprintln!("{}", write_error(name, &io_error_reason(&error)));
                        status = 1;
                    }
                    Reaction::WarnAndExit => {
                        eprintln!("{}", write_error(name, &io_error_reason(&error)));
                        std::process::exit(1);
                    }
                }
            }
        }
    }

    for (_, handle) in outputs.iter_mut() {
        let _ = handle.flush();
    }
    let _ = stdout.flush();
    std::process::exit(status);
}