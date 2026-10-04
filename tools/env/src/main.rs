//! `env(1)`: run a program in a modified environment.
//!
//! All the parsing and splitting lives in the library; this file only does the
//! work the parser asks for: rewiring the environment, moving the process
//! somewhere else, and handing the command over to `execvp`.

use std::ffi::{CString, OsString};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;

use env::{
    invalid_signal_message, parse_mask_signals, parse_signal_actions, signal_name,
    split_string, try_help_message, Action, Direction, Parser, Request,
};
use quoting::{open_locale, quote, quoteaf};

const VERSION: &str = "1.0";

const HELP: &str = "\
Usage: %s [OPTION]... [-] [NAME=VALUE]... [COMMAND [ARG]...]
Set each NAME to VALUE in the environment and run COMMAND.

Mandatory arguments to long options are mandatory for short options too.
  -i, --ignore-environment  start with an empty environment
  -0, --null           end each output line with NUL, not newline
  -u, --unset=NAME     remove variable from the environment
  -C, --chdir=DIR      change working directory to DIR
  -S, --split-string=S  process and split S into separate arguments;
                        used to pass multiple arguments on shebang lines
      --block-signal[=SIG]    block delivery of SIG signal(s) to COMMAND
      --default-signal[=SIG]  reset handling of SIG signal(s) to the default
      --ignore-signal[=SIG]   set handling of SIG signal(s) to do nothing
      --list-signal-handling  list non default signal handling to stderr
  -v, --debug          print verbose information for each processing step
      --help        display this help and exit
      --version     output version information and exit

A mere - implies -i.  If no COMMAND, print the resulting environment.

SIG may be a signal name like 'PIPE', or a signal number like '13'.
Without SIG, all known signals are included.  Multiple signals can be
comma-separated.  An empty SIG argument is a no-op.

Exit status:
  125  if the env command itself fails
  126  if COMMAND is found but cannot be invoked
  127  if COMMAND cannot be found
  -    the exit status of COMMAND otherwise
";

/// `EXIT_CANCELED`: the tool itself could not do its job.
const EXIT_CANCELED: i32 = 125;
/// The command was found but could not be run.
const EXIT_CANNOT_INVOKE: i32 = 126;
/// The command was not found.
const EXIT_ENOENT: i32 = 127;

/// The system's spelling of an errno, the way `error()` reports it.
fn reason_for(raw: i32) -> String {
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

fn last_reason() -> String {
    reason_for(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO))
}

/// Apply what `--default-signal` and `--ignore-signal` asked for.
///
/// The debug line is printed for every signal that was asked about, including the
/// ones whose change failed and was let through: KILL and STOP cannot be touched,
/// and `--default-signal` with no argument expects that rather than stopping.
fn reset_signal_handlers(actions: &[Action], debug: bool, program: &str) {
    for number in 1..actions.len() {
        let action = actions[number];
        if action == Action::Unchanged {
            continue;
        }
        let ignore_errors = matches!(action, Action::DefaultNoerr | Action::IgnoreNoerr);
        let to_default = matches!(action, Action::Default | Action::DefaultNoerr);
        let index = number as libc::c_int;
        // SAFETY: both calls pass a valid signal number from the table, and a
        // live sigaction to fill in.
        let mut current: libc::sigaction = unsafe { std::mem::zeroed() };
        let mut failed = unsafe { libc::sigaction(index, std::ptr::null(), &mut current) } != 0;
        if failed {
            if !ignore_errors {
                eprintln!(
                    "{}: failed to get signal action for signal {}: {}",
                    program, number, last_reason()
                );
                std::process::exit(EXIT_CANCELED);
            }
        } else {
            current.sa_sigaction = if to_default { libc::SIG_DFL } else { libc::SIG_IGN };
            failed = unsafe { libc::sigaction(index, &current, std::ptr::null_mut()) } != 0;
            if failed && !ignore_errors {
                eprintln!(
                    "{}: failed to set signal action for signal {}: {}",
                    program, number, last_reason()
                );
                std::process::exit(EXIT_CANCELED);
            }
        }
        if debug {
            eprintln!(
                "Reset signal {} ({}) to {}{}",
                signal_name(number as i32).unwrap_or_default(),
                number,
                if to_default { "DEFAULT" } else { "IGNORE" },
                if failed { " (failure ignored)" } else { "" }
            );
        }
    }
}

/// Install the mask that `--block-signal` described.
fn set_signal_proc_mask(block: &[bool], unblock: &[bool], debug: bool, program: &str) {
    let mut set: libc::sigset_t = unsafe { std::mem::zeroed() };
    // SAFETY: `set` is a live sigset_t; the query form takes no mask.
    if unsafe { libc::sigprocmask(0, std::ptr::null(), &mut set) } != 0 {
        eprintln!("{}: failed to get signal process mask: {}", program, last_reason());
        std::process::exit(EXIT_CANCELED);
    }
    for number in 1..block.len() {
        if !block[number] && !unblock[number] {
            continue;
        }
        let index = number as libc::c_int;
        // SAFETY: `set` is live and `index` is a signal number.
        unsafe {
            libc::sigdelset(&mut set, index);
            if block[number] {
                libc::sigaddset(&mut set, index);
            }
        }
        // The two signals reserved for the C library are in the mask but have no
        // name to report, and the system tool leaves them out of the log.
        if debug {
            if let Some(name) = signal_name(number as i32) {
                eprintln!(
                    "signal {} ({}) mask set to {}",
                    name,
                    number,
                    if unblock[number] { "UNBLOCK" } else { "BLOCK" }
                );
            }
        }
    }
    // SAFETY: `set` was just rebuilt.
    if unsafe { libc::sigprocmask(libc::SIG_SETMASK, &set, std::ptr::null_mut()) } != 0 {
        eprintln!("{}: failed to set signal process mask: {}", program, last_reason());
        std::process::exit(EXIT_CANCELED);
    }
}

/// Report every signal that is not simply left alone.
fn list_signal_handling() {
    let mut set: libc::sigset_t = unsafe { std::mem::zeroed() };
    // SAFETY: `set` is live; the query form takes no mask.
    if unsafe { libc::sigprocmask(0, std::ptr::null(), &mut set) } != 0 {
        std::process::exit(EXIT_CANCELED);
    }
    for number in 1..=64 {
        let index = number as libc::c_int;
        // SAFETY: `set` is live and the index is a signal number.
        let blocked = unsafe { libc::sigismember(&set, index) } == 1;
        let mut current: libc::sigaction = unsafe { std::mem::zeroed() };
        // SAFETY: the index is a signal number and `current` is live.
        if unsafe { libc::sigaction(index, std::ptr::null(), &mut current) } != 0 {
            continue;
        }
        let ignored = current.sa_sigaction == libc::SIG_IGN;
        if !blocked && !ignored {
            continue;
        }
        let name = match signal_name(number) {
            Some(name) => name,
            None => continue,
        };
        eprintln!(
            "{:<10} ({:>2}): {}{}{}",
            name,
            number,
            if blocked { "BLOCK" } else { "" },
            if blocked && ignored { "," } else { "" },
            if ignored { "IGNORE" } else { "" }
        );
    }
}

extern "C" {
    /// The C library's environment array. Declared mutable because the
    /// empty-name case has to grow it, which no libc call will do.
    static mut environ: *mut *mut libc::c_char;
}

/// The environment as the C library has it, one raw `NAME=VALUE` entry per
/// element.
///
/// The std wrappers are not used here because they keep their own bookkeeping:
/// `vars_os` will not show an entry whose name is empty, and `env` is expected to
/// print one (`env =x` really does put `=x` there). `env.c` prints the entries
/// verbatim, so this returns them unparsed too.
fn environment() -> Vec<Vec<u8>> {
    let mut entries = Vec::new();
    // SAFETY: the head pointer is only read, and the array it names is a run of
    // NUL-terminated strings closed by a null *element* — not by a null pointer,
    // which is the detail that makes a naive walk run off the end.
    let mut cursor = unsafe { environ };
    while !cursor.is_null() {
        let entry = unsafe { *cursor };
        if entry.is_null() {
            break;
        }
        entries.push(unsafe { std::ffi::CStr::from_ptr(entry) }.to_bytes().to_vec());
        cursor = unsafe { cursor.add(1) };
    }
    entries
}

/// One variable's value, or `None` when it is not set.
fn look_up(name: &str) -> Option<String> {
    let text = CString::new(name).ok()?;
    // SAFETY: `text` is NUL-terminated; getenv returns a pointer owned by the C
    // library, or null when the variable is not set.
    let found = unsafe { libc::getenv(text.as_ptr()) };
    if found.is_null() {
        return None;
    }
    Some(unsafe { std::ffi::CStr::from_ptr(found) }.to_string_lossy().into_owned())
}

/// Remove a variable the way `unsetenv` does.
///
/// `std::env::remove_var` panics on an empty name, where the system tool simply
/// fails to find one and moves on, so the call is made directly.
fn remove_variable(name: &str, program: &str, debug: bool) {
    if debug {
        eprintln!("unset:    {}", name);
    }
    let text = match CString::new(name) {
        Ok(text) => text,
        Err(_) => return,
    };
    // SAFETY: `text` is NUL-terminated and unsetenv only reads it.
    if unsafe { libc::unsetenv(text.as_ptr()) } != 0 {
        eprintln!(
            "{}: cannot unset {}: {}",
            program,
            quote(name),
            last_reason()
        );
        std::process::exit(EXIT_CANCELED);
    }
}

/// Put an entry with an empty name into the environment.
///
/// `env =x` really does put `=x` there, and glibc allows it, but musl's setenv
/// and putenv both refuse a name that is empty. So the array is grown by hand:
/// this tool has to do it because that is the observable behaviour, and the
/// command the entry is passed to reads it too.
fn set_empty_named_variable(value: &[u8], program: &str) {
    let mut entry = vec![b'='];
    entry.extend_from_slice(value);
    let text = match CString::new(entry) {
        Ok(text) => text,
        Err(_) => return,
    };
    // The array stores the pointer, so the string has to stay allocated for the
    // rest of the process.
    let pointer = text.into_raw();
    // SAFETY: environ is a NUL-terminated array of char*. It is copied into a
    // fresh allocation with room for one more slot rather than grown in place:
    // the array belongs to the C library and was not necessarily malloc'd, so
    // reallocating it would be undefined. The old array is simply abandoned.
    unsafe {
        let mut count = 0usize;
        while !(*environ.add(count)).is_null() {
            // An entry whose name is already empty is replaced where it stands,
            // which is how `env =x =y` ends up with one `=y` in the position the
            // first one took.
            if **environ.add(count) == b'=' as libc::c_char {
                *environ.add(count) = pointer;
                return;
            }
            count += 1;
        }
        let slot = std::mem::size_of::<*mut libc::c_char>();
        let grown = libc::malloc((count + 2) * slot) as *mut *mut libc::c_char;
        if grown.is_null() {
            eprintln!("{}: cannot set {}: {}", program, quote(""), last_reason());
            std::process::exit(EXIT_CANCELED);
        }
        std::ptr::copy_nonoverlapping(environ, grown, count);
        *grown.add(count) = pointer;
        *grown.add(count + 1) = std::ptr::null_mut();
        environ = grown;
    }
}

/// Set a variable the way `putenv` does, which — unlike the std wrapper — is
/// happy with an empty name.
fn set_variable(name: &[u8], value: &[u8], program: &str) {
    if name.is_empty() {
        set_empty_named_variable(value, program);
        return;
    }
    let mut entry = name.to_vec();
    entry.push(b'=');
    entry.extend_from_slice(value);
    // putenv keeps the pointer rather than copying, so the storage has to live
    // as long as the environment does: a plain leak is the right thing here.
    let text = match CString::new(entry) {
        Ok(text) => text,
        Err(_) => return,
    };
    let pointer = text.into_raw();
    // SAFETY: `pointer` came from a CString, so it is a valid NUL-terminated
    // string that stays allocated for the rest of the process.
    if unsafe { libc::putenv(pointer) } != 0 {
        eprintln!(
            "{}: cannot set {}: {}",
            program,
            quote(&String::from_utf8_lossy(name)),
            last_reason()
        );
        std::process::exit(EXIT_CANCELED);
    }
}

fn main() {
    // `env.c` opens the locale, and the quoting style of every diagnostic below
    // follows from it, so it has to happen before anything can be printed.
    open_locale();

    // Rust's runtime leaves SIGPIPE ignored, while the system tool starts from
    // whatever the shell had, which is usually the default. `--list-signal-
    // handling` would report the difference, and so would anything the command
    // inherits, so it is put back before anything else happens.
    // SAFETY: SIGPIPE can always be set to SIG_DFL.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let raw: Vec<OsString> = std::env::args_os().collect();
    let program = raw
        .first()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "env".to_string());
    let operands = raw.into_iter().skip(1).collect();
    // The parser works on text; anything it hands back as a command is turned
    // back into bytes so a name that is not valid UTF-8 still reaches execvp.
    let mut parser = Parser::new(&program, operands);

    let mut ignore_environment = false;
    let mut nul_terminate_output = false;
    let mut debug = false;
    let mut list_handling = false;
    let mut unsets: Vec<String> = Vec::new();
    let mut chdir: Option<String> = None;
    let mut actions: Vec<Action> = vec![Action::Unchanged; 65];
    let mut block: Vec<bool> = vec![false; 65];
    let mut unblock: Vec<bool> = vec![false; 65];
    let mut mask_changed = false;
    let mut refused: Option<String> = None;

    let parsed = loop {
        let request = match parser.next() {
            Ok(None) => break Ok(()),
            Ok(Some(request)) => request,
            Err(failure) => break Err(failure),
        };
        match request {
            Request::IgnoreEnvironment => ignore_environment = true,
            Request::NullTerminateOutput => nul_terminate_output = true,
            Request::Debug => debug = true,
            Request::ListSignalHandling => list_handling = true,
            Request::Help => {
                print!("{}", HELP.replacen("%s", &program, 1));
                std::process::exit(0);
            }
            Request::Version => {
                println!("env (rusttool) {}", VERSION);
                std::process::exit(0);
            }
            Request::Unset(name) => unsets.push(name),
            Request::Chdir(directory) => chdir = Some(directory),
            Request::BlockSignal(value) => {
                if let Err(operand) =
                    parse_mask_signals(&mut block, &mut unblock, value.as_deref(), Direction::Block)
                {
                    refused = Some(operand);
                    break Ok(());
                }
                mask_changed = true;
            }
            request @ (Request::DefaultSignal(_) | Request::IgnoreSignal(_)) => {
                let (value, default) = match request {
                    Request::DefaultSignal(value) => (value, true),
                    Request::IgnoreSignal(value) => (value, false),
                    _ => unreachable!("matched above"),
                };
                // The system tool runs the mask parser as well for
                // --default-signal, so resetting a handler also unblocks it.
                // --ignore-signal leaves the mask alone.
                if let Err(operand) = parse_signal_actions(&mut actions, value.as_deref(), default)
                {
                    refused = Some(operand);
                    break Ok(());
                }
                let unblocked = default.then(|| {
                    parse_mask_signals(
                        &mut block,
                        &mut unblock,
                        value.as_deref(),
                        Direction::Unblock,
                    )
                });
                if let Some(Err(operand)) = unblocked {
                    refused = Some(operand);
                    break Ok(());
                }
                if default {
                    mask_changed = true;
                }
            }
            Request::Split(text) => {
                // The expansion lines come out as they are produced, so a string
                // that fails later has already shown them.
                let mut streamed = String::new();
                let outcome = match split_string(&text, &look_up, &mut |note| {
                    if debug {
                        streamed.push_str(note);
                    }
                }) {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        eprint!("{}", streamed);
                        // The one line the system tool prints here has no Try
                        // line after it.
                        eprintln!("{}", error.message());
                        std::process::exit(EXIT_CANCELED);
                    }
                };
                if debug {
                    eprint!("{}", streamed);
                    // The count `env.c` tests is the argument count including
                    // the program name, so a -S string that made no arguments at
                    // all prints nothing and anything else does.
                    if !outcome.args.is_empty() {
                        eprintln!("split -S:  {}", quote(&text));
                        eprintln!(" into:    {}", quote(&outcome.args[0]));
                        for argument in &outcome.args[1..] {
                            eprintln!("     &    {}", quote(argument));
                        }
                    }
                }
                parser.splice(outcome.args);
            }
        }
    };

    match parsed {
        Err(failure) => {
            if !failure.0.is_empty() {
                eprintln!("{}", failure.0);
            }
            eprintln!("{}", try_help_message(&program));
            std::process::exit(EXIT_CANCELED);
        }
        Ok(()) => {}
    }
    if let Some(operand) = refused {
        eprintln!("{}", invalid_signal_message(&program, &operand));
        eprintln!("{}", try_help_message(&program));
        std::process::exit(EXIT_CANCELED);
    }

    // The parser may have rewritten the argument vector, so the command is taken
    // from its operands.
    let text_operands = parser.operands();
    // A bare "-" is the short form of -i.
    let dashed = text_operands.first().map(|word| word.as_bytes()) == Some(b"-".as_slice());

    if ignore_environment || dashed {
        if debug {
            eprintln!("cleaning environ");
        }
        for entry in environment() {
            let name = match entry.iter().position(|byte| *byte == b'=') {
                Some(at) => String::from_utf8_lossy(&entry[..at]).into_owned(),
                None => String::from_utf8_lossy(&entry).into_owned(),
            };
            remove_variable(&name, &program, false);
        }
    } else {
        // The unsets happen before the assignments, so `env -u A A=5` really does
        // end up with A set.
        for name in &unsets {
            remove_variable(name, &program, debug);
        }
    }

    // Every operand holding an '=' is an assignment, and the first one without
    // starts the command.
    let mut command_start = text_operands.len();
    for (position, word) in text_operands.iter().enumerate() {
        if position == 0 && word.as_bytes() == b"-" {
            continue;
        }
        let bytes = word.as_bytes();
        let at = match bytes.iter().position(|byte| *byte == b'=') {
            Some(at) => at,
            None => {
                command_start = position;
                break;
            }
        };
        if debug {
            eprintln!("setenv:   {}", String::from_utf8_lossy(bytes));
        }
        // An empty name is allowed: `env =x` puts `=x` in the environment.
        set_variable(&bytes[..at], &bytes[at + 1..], &program);
    }
    let operands = &text_operands[command_start..];

    let program_specified = !operands.is_empty();

    if nul_terminate_output && program_specified {
        eprintln!("{}: cannot specify --null (-0) with command", program);
        eprintln!("{}", try_help_message(&program));
        std::process::exit(EXIT_CANCELED);
    }
    if chdir.is_some() && !program_specified {
        eprintln!("{}: must specify command with --chdir (-C)", program);
        eprintln!("{}", try_help_message(&program));
        std::process::exit(EXIT_CANCELED);
    }

    if !program_specified {
        let separator = if nul_terminate_output { b'\0' } else { b'\n' };
        // `env.c` prints each entry as it stands, so an entry with no '=' in it
        // comes out without one.
        let mut out = Vec::new();
        for entry in environment() {
            out.extend_from_slice(&entry);
            out.push(separator);
        }
        let stdout = std::io::stdout();
        let mut handle = stdout.lock();
        if handle.write_all(&out).is_err() || handle.flush().is_err() {
            eprintln!("{}: write error", program);
            std::process::exit(EXIT_CANCELED);
        }
        std::process::exit(0);
    }

    reset_signal_handlers(&actions, debug, &program);
    if mask_changed {
        set_signal_proc_mask(&block, &unblock, debug, &program);
    }
    if list_handling {
        list_signal_handling();
    }

    if let Some(directory) = &chdir {
        if debug {
            eprintln!("chdir:    {}", quoteaf(directory));
        }
        let path =
            CString::new(directory.as_bytes()).unwrap_or_else(|_| CString::new("/").unwrap());
        // SAFETY: `path` is NUL-terminated and chdir only changes the cwd.
        if unsafe { libc::chdir(path.as_ptr()) } != 0 {
            eprintln!(
                "{}: cannot change directory to {}: {}",
                program,
                quoteaf(directory),
                last_reason()
            );
            std::process::exit(EXIT_CANCELED);
        }
    }

    let head = operands[0].to_string_lossy().into_owned();
    if debug {
        eprintln!("executing: {}", head);
        for (position, argument) in operands.iter().enumerate() {
            eprintln!(
                "   arg[{}]= {}",
                position,
                quote(&argument.to_string_lossy())
            );
        }
    }

    // execvp wants a NUL-terminated vector whose first entry is the program.
    let mut c_arguments: Vec<CString> = Vec::with_capacity(operands.len());
    for argument in operands {
        c_arguments.push(
            CString::new(argument.as_bytes()).unwrap_or_else(|_| CString::new("").unwrap()),
        );
    }
    let mut pointers: Vec<*const libc::c_char> =
        c_arguments.iter().map(|text| text.as_ptr()).collect();
    pointers.push(std::ptr::null());
    // SAFETY: `pointers` is a valid argv and execvp only reads it.
    unsafe { libc::execvp(c_arguments[0].as_ptr(), pointers.as_ptr()) };

    let raw = std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::ENOENT);
    eprintln!("{}: {}: {}", program, quote(&head), reason_for(raw));
    let status = if raw == libc::ENOENT {
        EXIT_ENOENT
    } else {
        EXIT_CANNOT_INVOKE
    };
    // C_ISSPACE_CHARS is " \t\n\v\f\r": the vertical tab is in it even though
    // Rust's is_ascii_whitespace leaves it out.
    if status == EXIT_ENOENT && head.bytes().any(|byte| b" \t\n\x0b\x0c\r".contains(&byte)) {
        eprintln!("{}: use -[v]S to pass options in shebang lines", program);
    }
    std::process::exit(status);
}