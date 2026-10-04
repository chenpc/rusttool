//! Decision logic of `tee(1)`, following its manual page:
//!
//! ```text
//! tee [OPTION]... [FILE]...
//! ```
//!
//! The interesting part is `--output-error=MODE` and `-p`: they decide what
//! happens when one of the outputs cannot be written, and a pipe that goes
//! away is the case that matters in practice.

/// `--output-error[=MODE]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputError {
    /// Diagnose errors writing to any output.
    Warn,
    /// Diagnose errors writing to any output that is not a pipe.
    WarnNoPipe,
    /// Exit on error writing to any output.
    Exit,
    /// Exit on error writing to any output that is not a pipe.
    ExitNoPipe,
}

impl OutputError {
    /// The documented spellings, plus the bare `--output-error` form, which is
    /// `warn`.
    pub fn parse(text: &str) -> Result<OutputError, String> {
        match text {
            "" | "warn" => Ok(OutputError::Warn),
            "warn-nopipe" => Ok(OutputError::WarnNoPipe),
            "exit" => Ok(OutputError::Exit),
            "exit-nopipe" => Ok(OutputError::ExitNoPipe),
            other => Err(other.to_string()),
        }
    }

    /// Whether a write error to `is_pipe` should end the program.
    pub fn should_exit(&self, is_pipe: bool) -> bool {
        match self {
            OutputError::Warn => false,
            OutputError::WarnNoPipe => false,
            OutputError::Exit => true,
            OutputError::ExitNoPipe => !is_pipe,
        }
    }

    /// Whether a write error to `is_pipe` should be reported at all.
    pub fn should_warn(&self, is_pipe: bool) -> bool {
        match self {
            OutputError::Warn | OutputError::Exit => true,
            OutputError::WarnNoPipe | OutputError::ExitNoPipe => !is_pipe,
        }
    }
}

/// The options `tee` accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// -a/--append
    pub append: bool,
    /// -i/--ignore-interrupts
    pub ignore_interrupts: bool,
    /// -p: the manual's "more appropriate MODE with pipes", whose default
    /// output-error mode is warn-nopipe.
    pub pipe_mode: bool,
    pub output_error: OutputError,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            append: false,
            ignore_interrupts: false,
            pipe_mode: false,
            output_error: OutputError::Warn,
        }
    }
}

impl Options {
    /// -p changes the default mode; an explicit --output-error still wins.
    pub fn effective(&self) -> Options {
        let mut o = self.clone();
        if o.pipe_mode && o.output_error == OutputError::Warn {
            o.output_error = OutputError::WarnNoPipe;
        }
        o
    }
}

/// What should happen after a failed write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reaction {
    /// Say something and carry on.
    Warn,
    /// Say something and stop.
    WarnAndExit,
    /// Say nothing; this is the silent default for a pipe.
    Ignore,
}

/// Decide how to react to a write error on one output.
pub fn react(is_pipe: bool, options: &Options) -> Reaction {
    let options = options.effective();
    if !options.output_error.should_warn(is_pipe) {
        return Reaction::Ignore;
    }
    if options.output_error.should_exit(is_pipe) {
        Reaction::WarnAndExit
    } else {
        Reaction::Warn
    }
}

/// Whether one more output still has to be written, given that `remaining` of
/// them are left. Standard output is always written, so it is never counted
/// here.
pub fn keep_going(remaining_after_failure: usize) -> bool {
    remaining_after_failure > 0
}

/// The mode a file tee creates gets: 0666 with the umask applied, which is what
/// open(2) does for a file that does not exist yet.
pub fn created_mode(umask: u32) -> u32 {
    0o666 & !umask
}

/// GNU diagnostics.
pub fn write_error(file: &str, reason: &str) -> String {
    format!("tee: {}: {}", file, reason)
}

pub fn stdout_error(reason: &str) -> String {
    format!("tee: standard output: {}", reason)
}

/// The invalid MODE diagnostic, with the list of what would have been accepted,
/// which is how coreutils words it.
pub fn invalid_mode_lines(mode: &str) -> Vec<String> {
    let quoted = |text: &str| format!("\u{2018}{}\u{2019}", text);
    vec![
        format!("tee: invalid argument {} for {}", quoted(mode), quoted("--output-error")),
        "Valid arguments are:".to_string(),
        format!("  - {}", quoted("warn")),
        format!("  - {}", quoted("warn-nopipe")),
        format!("  - {}", quoted("exit")),
        format!("  - {}", quoted("exit-nopipe")),
        try_help_message(),
    ]
}

pub fn unrecognized_option_message(option: &str) -> String {
    format!("tee: unrecognized option '{}'", option)
}

pub fn invalid_option_message(letter: char) -> String {
    format!("tee: invalid option -- '{}'", letter)
}

pub fn try_help_message() -> String {
    "Try 'tee --help' for more information.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_documented_modes_parse() {
        assert_eq!(OutputError::parse("").unwrap(), OutputError::Warn);
        assert_eq!(OutputError::parse("warn").unwrap(), OutputError::Warn);
        assert_eq!(
            OutputError::parse("warn-nopipe").unwrap(),
            OutputError::WarnNoPipe
        );
        assert_eq!(OutputError::parse("exit").unwrap(), OutputError::Exit);
        assert_eq!(
            OutputError::parse("exit-nopipe").unwrap(),
            OutputError::ExitNoPipe
        );
        assert!(OutputError::parse("maybe").is_err());
    }

    #[test]
    fn warn_never_exits() {
        assert!(!OutputError::Warn.should_exit(false));
        assert!(!OutputError::Warn.should_exit(true));
        assert!(OutputError::Warn.should_warn(false));
        assert!(OutputError::Warn.should_warn(true));
    }

    #[test]
    fn the_nopipe_modes_ignore_pipes() {
        assert!(!OutputError::WarnNoPipe.should_warn(true));
        assert!(OutputError::WarnNoPipe.should_warn(false));
        assert!(!OutputError::ExitNoPipe.should_exit(true));
        assert!(OutputError::ExitNoPipe.should_exit(false));
    }

    #[test]
    fn exit_always_stops() {
        assert!(OutputError::Exit.should_exit(false));
        assert!(OutputError::Exit.should_exit(true));
    }

    #[test]
    fn pipe_mode_defaults_to_warn_nopipe() {
        let options = Options {
            pipe_mode: true,
            ..Options::default()
        }
        .effective();
        assert_eq!(options.output_error, OutputError::WarnNoPipe);
    }

    #[test]
    fn an_explicit_mode_beats_pipe_mode() {
        let options = Options {
            pipe_mode: true,
            output_error: OutputError::Exit,
            ..Options::default()
        }
        .effective();
        assert_eq!(options.output_error, OutputError::Exit);
    }

    #[test]
    fn reactions_follow_the_mode() {
        let options = Options::default();
        assert_eq!(react(false, &options), Reaction::Warn);
        assert_eq!(react(true, &options), Reaction::Warn);

        let options = Options {
            output_error: OutputError::WarnNoPipe,
            ..Options::default()
        };
        assert_eq!(react(true, &options), Reaction::Ignore);
        assert_eq!(react(false, &options), Reaction::Warn);

        let options = Options {
            output_error: OutputError::Exit,
            ..Options::default()
        };
        assert_eq!(react(true, &options), Reaction::WarnAndExit);
    }

    #[test]
    fn the_last_output_still_finishes() {
        assert!(keep_going(1));
        assert!(!keep_going(0));
    }

    #[test]
    fn created_files_get_0666_minus_the_umask() {
        assert_eq!(created_mode(0o022), 0o644);
    }

    #[test]
    fn messages_match_coreutils() {
        assert_eq!(
            write_error("out", "Permission denied"),
            "tee: out: Permission denied"
        );
        assert_eq!(
            stdout_error("Broken pipe"),
            "tee: standard output: Broken pipe"
        );
        assert_eq!(invalid_option_message('Z'), "tee: invalid option -- 'Z'");
        assert_eq!(invalid_mode_lines("maybe").len(), 7);
        assert_eq!(invalid_mode_lines("maybe")[6], try_help_message());
        assert_eq!(
            invalid_mode_lines("maybe")[0],
            "tee: invalid argument \u{2018}maybe\u{2019} for \u{2018}--output-error\u{2019}"
        );
    }
}