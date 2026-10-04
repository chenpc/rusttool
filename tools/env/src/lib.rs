//! `env(1)`: run a program in a modified environment.
//!
//! Nearly all of this tool is the `-S` splitter, a small hand-written state
//! machine in `env.c`. Three of its rules are easy to get wrong:
//!
//! * The machine starts with `sep` **set**, so the first character that is about
//!   to be appended finishes an *empty* first argument. That is why `-S "''"`
//!   yields one empty argument while `-S ''` yields none at all.
//! * `check_start_new_arg` runs **before every appended character**, not just at
//!   whitespace. The quote characters call it themselves, so `-S '""'` produces
//!   an empty argument even though neither quote contributes a byte.
//! * `${VAR}` calls it only when the variable is *set*. An unset variable
//!   contributes nothing **and does not break the argument**, so
//!   `-S '${NOSUCH}x'` is one argument, `x`.
//!
//! The `'#'` rule is a fourth one and it interacts with the third: a `#` seen
//! where a new argument would start ends the string immediately, jumping past
//! the unterminated-quote check. `-S "'#'x"` therefore succeeds and yields the
//! single argument `#x`, even though the single quote was never closed.
//!
//! The quoting in the diagnostics comes from the `quoting` crate, which also
//! carries the locale rule; see that crate for why the shape of a quoted argument
//! is not the same on every machine.
//!
//! The signal tables are gnulib's, and they are not just the `<signal.h>`
//! macros: the two signals reserved for the C library are left out, a few
//! signals have aliases, and `RTMIN+n` / `RTMAX-n` are parsed and then validated
//! against the table instead of being range-checked themselves — which is why
//! `RTMIN+30` is a perfectly good way to name signal 64.

// ---------------------------------------------------------------- signals --

/// Signals 1..=31 as gnulib's table has them: the canonical name `sig2str`
/// reports, and the aliases `str2sig` also accepts.
pub const SIGNALS_1_TO_31: [(&str, &[&str]); 31] = [
    ("HUP", &[]),
    ("INT", &[]),
    ("QUIT", &[]),
    ("ILL", &[]),
    ("TRAP", &[]),
    ("ABRT", &["IOT"]),
    ("BUS", &[]),
    ("FPE", &[]),
    ("KILL", &[]),
    ("USR1", &[]),
    ("SEGV", &[]),
    ("USR2", &[]),
    ("PIPE", &[]),
    ("ALRM", &[]),
    ("TERM", &[]),
    ("STKFLT", &[]),
    ("CHLD", &["CLD"]),
    ("CONT", &[]),
    ("STOP", &[]),
    ("TSTP", &[]),
    ("TTIN", &[]),
    ("TTOU", &[]),
    ("URG", &[]),
    ("XCPU", &[]),
    ("XFSZ", &[]),
    ("VTALRM", &[]),
    ("PROF", &[]),
    ("WINCH", &[]),
    ("POLL", &["IO"]),
    ("PWR", &[]),
    ("SYS", &[]),
];

/// First and last real-time signal. Signals 32 and 33 are reserved for the C
/// library and are in neither the name table nor the accepted set.
pub const RTMIN: i32 = 34;
pub const RTMAX: i32 = 64;

/// Whether `number` has a name in the table.
pub fn is_named_signal(number: i32) -> bool {
    (1..=31).contains(&number) || (RTMIN..=RTMAX).contains(&number)
}

/// The name `sig2str` gives `number`, or `None` when it has none.
///
/// The middle of the real-time range is named from whichever end is nearer, so
/// 34..49 read `RTMIN` .. `RTMIN+15` and 50..64 read `RTMAX-14` .. `RTMAX`.
pub fn signal_name(number: i32) -> Option<String> {
    if (1..=31).contains(&number) {
        Some(SIGNALS_1_TO_31[(number - 1) as usize].0.to_string())
    } else if number == RTMIN {
        Some("RTMIN".to_string())
    } else if number == RTMAX {
        Some("RTMAX".to_string())
    } else if (RTMIN..=RTMAX).contains(&number) {
        if number <= RTMIN + 15 {
            Some(format!("RTMIN+{}", number - RTMIN))
        } else {
            Some(format!("RTMAX-{}", RTMAX - number))
        }
    } else {
        None
    }
}

/// The number `str2sig` gives `text`, or `None` when it names nothing.
///
/// A `SIG` prefix is optional and case is ignored throughout. `EXIT` resolves
/// to 0, which `env` then rejects itself.
pub fn signal_number(text: &str) -> Option<i32> {
    if text.is_empty() {
        return None;
    }
    let rest = match text.len() >= 3 && text[..3].eq_ignore_ascii_case("SIG") {
        true => &text[3..],
        false => text,
    };
    if rest.is_empty() {
        return None;
    }

    // A plain decimal number, optionally after the SIG prefix: `2`, `02`, `SIG2`.
    if rest.bytes().all(|byte| byte.is_ascii_digit()) {
        let number = rest.parse::<i32>().ok()?;
        return if is_named_signal(number) {
            Some(number)
        } else {
            None
        };
    }

    // `EXIT` is the one name outside the table that str2sig knows.
    if rest.eq_ignore_ascii_case("EXIT") {
        return Some(0);
    }

    let upper = rest.to_ascii_uppercase();
    for (index, (canonical, aliases)) in SIGNALS_1_TO_31.iter().enumerate() {
        if &upper == canonical || aliases.iter().any(|alias| &upper == alias) {
            return Some(index as i32 + 1);
        }
    }

    // `RTMIN+n` and `RTMAX-n`. Spaces may sit between the word and the sign but
    // nowhere else, and the result is only rejected when it falls off the table,
    // which is how `RTMIN+30` reaches signal 64.
    for (word, step) in [("RTMIN", 1i64), ("RTMAX", -1i64)] {
        if !upper.starts_with(word) {
            continue;
        }
        let mut at = word.len();
        while upper.as_bytes().get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        let base_number = if word == "RTMIN" { RTMIN } else { RTMAX };
        // The sign has to agree with the word: `RTMIN-1` and `RTMAX+1` are both
        // refused rather than read the other way round.
        match upper.as_bytes().get(at) {
            // The bare word names the start of the range it comes from.
            None => return Some(base_number),
            Some(b'+') if step > 0 => {}
            Some(b'-') if step < 0 => {}
            _ => continue,
        }
        let digits = &upper[at + 1..];
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let number = match digits.parse::<i64>() {
            Ok(count) => base_number as i64 + step * count,
            Err(_) => continue,
        };
        if (1..=i32::MAX as i64).contains(&number) && is_named_signal(number as i32) {
            return Some(number as i32);
        }
    }
    None
}

// --------------------------------------------------------------- the -S split --

/// Why a `-S` string was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SplitError {
    /// A quote was still open when the string ran out.
    UnterminatedQuote,
    /// The string ends with a backslash, or the backslash escapes a NUL.
    BackslashAtEnd,
    /// A backslash in front of something that is not an escape.
    InvalidSequence { character: char },
    /// `\c` inside a double-quoted string.
    ControlInsideDoubleQuotes,
    /// A `$` that does not introduce `${VARNAME}`.
    BadVarName { rest: String },
}

impl SplitError {
    /// The single line the system tool prints for this, without the Try line.
    pub fn message(&self) -> String {
        match self {
            SplitError::UnterminatedQuote => {
                "env: no terminating quote in -S string".to_string()
            }
            SplitError::BackslashAtEnd => {
                "env: invalid backslash at end of string in -S".to_string()
            }
            SplitError::InvalidSequence { character } => {
                format!("env: invalid sequence '\\{}' in -S", character)
            }
            SplitError::ControlInsideDoubleQuotes => {
                "env: '\\c' must not appear in double-quoted -S string".to_string()
            }
            SplitError::BadVarName { rest } => format!(
                "env: only ${{VARNAME}} expansion is supported, error at: {}",
                rest
            ),
        }
    }
}

/// What one `-S` string produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Split {
    /// The arguments, with every assignment kept separate.
    pub args: Vec<String>,
}

/// Where a `$` introduces `${VARNAME}`, as the index just past the `}`.
fn scan_varname(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start + 1) != Some(&b'{') {
        return None;
    }
    let first = *bytes.get(start + 2)?;
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    let mut at = start + 3;
    while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
        at += 1;
    }
    if bytes.get(at) == Some(&b'}') {
        Some(at + 1)
    } else {
        None
    }
}

/// Split a `-S` string into arguments, expanding `${VARNAME}` through `lookup`.
///
/// `on_note` receives the `-v` lines as they happen rather than at the end, so a
/// string that expands a variable and then turns out to be malformed still shows
/// the expansion — which is what `env.c` does, since its `devmsg` calls happen
/// inline.
///
/// The trailing whitespace rule falls out of the state machine: separators only
/// start a new argument once another character arrives, so `-S 'A=1 '` gives one
/// argument, not two.
pub fn split_string(
    text: &str,
    lookup: &dyn Fn(&str) -> Option<String>,
    on_note: &mut dyn FnMut(&str),
) -> Result<Split, SplitError> {
    let bytes = text.as_bytes();
    let mut args: Vec<String> = Vec::new();
    let mut current: Vec<u8> = Vec::new();
    // Set at the start, so the first character appended completes an empty
    // first argument.
    let mut separator = true;
    let mut in_single = false;
    let mut in_double = false;
    let mut at = 0usize;
    let mut closed_at_hash = false;
    let mut closed_at_control = false;

    // `env.c`'s check_start_new_arg: a pending separator ends the argument in
    // progress and starts a fresh one.
    macro_rules! start_new_argument {
        () => {
            if separator {
                args.push(String::from_utf8_lossy(&current).into_owned());
                current.clear();
                separator = false;
            }
        };
    }

    'outer: while at < bytes.len() {
        let byte = bytes[at];
        match byte {
            b'\'' if !in_double => {
                in_single = !in_single;
                start_new_argument!();
                at += 1;
                continue;
            }
            b'"' if !in_single => {
                in_double = !in_double;
                start_new_argument!();
                at += 1;
                continue;
            }
            b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r' => {
                if !(in_single || in_double) {
                    separator = true;
                    // Skip the whole run, as strspn over C_ISSPACE_CHARS does.
                    at += 1;
                    while at < bytes.len() && bytes[at].is_ascii_whitespace() {
                        at += 1;
                    }
                    continue;
                }
            }
            b'#' => {
                if separator {
                    // '#' where an argument would start ends the string, and
                    // jumps past the unterminated-quote check.
                    closed_at_hash = true;
                    break;
                }
            }
            b'\\' => {
                let next = bytes.get(at + 1).copied();
                // Inside single quotes a backslash is an ordinary character
                // unless it escapes a backslash or a quote.
                if !(in_single && next != Some(b'\\') && next != Some(b'\'')) {
                    at += 1;
                    let escaped = match next {
                        Some(escaped) => escaped,
                        None => return Err(SplitError::BackslashAtEnd),
                    };
                    // An escaped character takes the same path as an ordinary
                    // one: it closes a pending separator first, so `a\#b` is one
                    // argument and not a `#` at an argument start.
                    macro_rules! push {
                        ($byte:expr) => {{
                            start_new_argument!();
                            current.push($byte);
                        }};
                    }
                    match escaped {
                        // Passed through as itself.
                        b'"' | b'#' | b'$' | b'\'' | b'\\' => push!(escaped),
                        // Outside quotes `\_` is a separator, exactly like
                        // whitespace, so it does not close an argument by itself
                        // either; inside double quotes it is a space.
                        b'_' => {
                            if in_double {
                                push!(b' ');
                            } else {
                                separator = true;
                                at += 1;
                                continue;
                            }
                        }
                        // Outside quotes `\c` ends the string outright.
                        b'c' => {
                            if in_double {
                                return Err(SplitError::ControlInsideDoubleQuotes);
                            }
                            closed_at_control = true;
                            break 'outer;
                        }
                        b'f' => push!(0x0c),
                        b'n' => push!(b'\n'),
                        b'r' => push!(b'\r'),
                        b't' => push!(b'\t'),
                        b'v' => push!(0x0b),
                        // A NUL cannot be told apart from the end of the string.
                        0 => return Err(SplitError::BackslashAtEnd),
                        other => {
                            return Err(SplitError::InvalidSequence {
                                character: other as char,
                            })
                        }
                    }
                    at += 1;
                    continue;
                }
            }
            b'$' if !in_single => {
                match scan_varname(bytes, at) {
                    None => {
                        return Err(SplitError::BadVarName {
                            rest: text[at..].to_string(),
                        })
                    }
                    Some(end) => {
                        let name = &text[at + 2..end - 1];
                        match lookup(name) {
                            Some(value) => {
                                start_new_argument!();
                                on_note(&format!(
                                    "expanding ${{{}}} into {}\n",
                                    name,
                                    quoting::quote(&value)
                                ));
                                current.extend_from_slice(value.as_bytes());
                            }
                            // An unset variable adds nothing and, crucially,
                            // does not start a new argument either.
                            None => {
                                on_note(&format!("replacing ${{{}}} with null string\n", name))
                            }
                        }
                        at = end;
                        continue;
                    }
                }
            }
            _ => {}
        }
        start_new_argument!();
        current.push(byte);
        at += 1;
    }

    // `goto eos` jumps past this check in env.c, so neither a `#` nor a `\c` at
    // an argument start can turn an open quote into an error.
    if (in_single || in_double) && !closed_at_hash && !closed_at_control {
        return Err(SplitError::UnterminatedQuote);
    }
    args.push(String::from_utf8_lossy(&current).into_owned());
    // The first check_start_new_arg closed the program-name slot rather than an
    // argument, and `build_argv` counts from one, so that leading empty entry is
    // not an argument of its own. Dropping it is what makes an empty -S string
    // produce nothing at all.
    if !args.is_empty() {
        args.remove(0);
    }
    Ok(Split { args })
}

// ------------------------------------------------------------ option parsing --

/// Whether a long option takes a value. `--block-signal` and friends take an
/// optional one, which is why `--block-signal` and `--block-signal=INT` are
/// different requests.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Arity {
    None,
    Required,
    Optional,
}

/// What each long option asks for, in the order the help lists them.
pub const LONG_OPTIONS: &[(&str, Arity)] = &[
    ("ignore-environment", Arity::None),
    ("null", Arity::None),
    ("unset", Arity::Required),
    ("chdir", Arity::Required),
    ("default-signal", Arity::Optional),
    ("ignore-signal", Arity::Optional),
    ("block-signal", Arity::Optional),
    ("list-signal-handling", Arity::None),
    ("debug", Arity::None),
    ("split-string", Arity::Required),
    ("help", Arity::None),
    ("version", Arity::None),
];

/// One thing an option asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    IgnoreEnvironment,
    NullTerminateOutput,
    Debug,
    ListSignalHandling,
    Unset(String),
    Chdir(String),
    /// A `-S` string, still to be split.
    Split(String),
    BlockSignal(Option<String>),
    DefaultSignal(Option<String>),
    IgnoreSignal(Option<String>),
    Help,
    Version,
}

/// A refusal that is followed by the Try line. An empty message means only the
/// Try line; `env.c` prints several diagnostics that way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageFailure(pub String);

impl UsageFailure {
    fn just_try_help() -> Self {
        UsageFailure(String::new())
    }
}

/// A getopt with `env.c`'s leading `+`: the first operand ends the option scan,
/// so nothing is permuted to the front and `env A=1 -i cmd` really does look for
/// a program called `-i`.
///
/// The arguments are held as `OsString` and only borrowed as text when a word
/// has to be looked at, because an operand that is not valid UTF-8 still has to
/// reach `execvp` unchanged.
///
/// The state lives here rather than in the caller because a `-S` string rewrites
/// the argument vector and resets the scan, which cannot be expressed as a plain
/// loop over a slice.
pub struct Parser {
    program: String,
    arguments: Vec<std::ffi::OsString>,
    scan: usize,
    /// The letters of a short bundle still to come, and how many are done. The
    /// argv slot is only counted once the whole bundle is spent.
    bundle: Option<(Vec<char>, usize)>,
}

impl Parser {
    pub fn new(program: &str, arguments: Vec<std::ffi::OsString>) -> Self {
        Parser {
            program: program.to_string(),
            arguments,
            scan: 0,
            bundle: None,
        }
    }

    /// The operands left once option parsing stopped, `--` included.
    pub fn operands(&self) -> &[std::ffi::OsString] {
        &self.arguments[self.scan.min(self.arguments.len())..]
    }

    /// The next thing to do, or `None` once the operands start.
    pub fn next(&mut self) -> Result<Option<Request>, UsageFailure> {
        if let Some((letters, at)) = self.bundle.as_mut() {
            while *at < letters.len() {
                let letter = letters[*at];
                *at += 1;
                match letter {
                    'i' => return Ok(Some(Request::IgnoreEnvironment)),
                    '0' => return Ok(Some(Request::NullTerminateOutput)),
                    'v' => return Ok(Some(Request::Debug)),
                    'u' | 'C' | 'S' => {
                        // The rest of the bundle is the value; failing that the
                        // next word is.
                        let value = if *at < letters.len() {
                            let value: String = letters[*at..].iter().collect();
                            *at = letters.len();
                            (value, 1)
                        } else {
                            match self.arguments.get(self.scan + 1) {
                                Some(next) => {
                                    (next.to_string_lossy().into_owned(), 2)
                                }
                                None => {
                                    self.bundle = None;
                                    return Err(UsageFailure(requires_argument_message(
                                        &self.program, letter,
                                    )));
                                }
                            }
                        };
                        // Past the bundle, and past the value word when one came
                        // from the next slot.
                        let (value, consumed) = value;
                        self.scan += consumed;
                        self.scan = self.scan.min(self.arguments.len());
                        self.bundle = None;
                        return Ok(Some(match letter {
                            'u' => Request::Unset(value),
                            'C' => Request::Chdir(value),
                            _ => Request::Split(value),
                        }));
                    }
                    // The undocumented space options, which catch a shebang
                    // line like "#!/usr/bin/env -i command".
                    ' ' | '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' => {
                        self.bundle = None;
                        eprintln!("{}", invalid_option_message(&self.program, letter));
                        eprintln!(
                            "{}: use -[v]S to pass options in shebang lines",
                            self.program
                        );
                        return Err(UsageFailure::just_try_help());
                    }
                    other => {
                        self.bundle = None;
                        return Err(UsageFailure(invalid_option_message(&self.program, other)));
                    }
                }
            }
            self.bundle = None;
            self.scan += 1;
            return self.next();
        }

        let argument = match self.arguments.get(self.scan) {
            Some(argument) => argument.to_string_lossy().into_owned(),
            None => return Ok(None),
        };
        if argument == "--" {
            self.scan += 1;
            return Ok(None);
        }
        // A lone "-", and anything not starting with a dash, is an operand.
        if !argument.starts_with('-') || argument.len() < 2 {
            return Ok(None);
        }
        match argument.strip_prefix("--") {
            Some(long) => self.long(long),
            None => {
                self.bundle = Some((argument.chars().skip(1).collect(), 0));
                self.next()
            }
        }
    }

    /// Put a `-S` string's arguments in place of the word that carried it, then
    /// start the scan again: the split arguments may themselves be options.
    pub fn splice(&mut self, arguments: Vec<String>) {
        let rest = self.arguments.split_off(self.scan.min(self.arguments.len()));
        let mut spliced: Vec<std::ffi::OsString> =
            arguments.into_iter().map(std::ffi::OsString::from).collect();
        spliced.extend(rest);
        self.arguments = spliced;
        self.scan = 0;
        self.bundle = None;
    }

    fn long(&mut self, long: &str) -> Result<Option<Request>, UsageFailure> {
        let (name, inline) = match long.split_once('=') {
            Some((name, value)) => (name, Some(value.to_string())),
            None => (long, None),
        };
        // Long options may be abbreviated, as long as exactly one matches.
        let matches: Vec<&(&str, Arity)> = LONG_OPTIONS
            .iter()
            .filter(|(option, _)| option.starts_with(name))
            .collect();
        let (option, arity) = match matches.as_slice() {
            [] => {
                return Err(UsageFailure(unrecognized_option_message(
                    &self.program, name,
                )))
            }
            [only] => **only,
            _ => {
                let names: Vec<&str> = matches.iter().map(|entry| entry.0).collect();
                return Err(UsageFailure(ambiguous_option_message(
                    &self.program, name, &names,
                )));
            }
        };
        self.scan += 1;
        let mut value = inline;
        match arity {
            Arity::None => {
                if value.is_some() {
                    return Err(UsageFailure(long_rejects_argument_message(
                        &self.program, option,
                    )));
                }
            }
            Arity::Required => {
                if value.is_none() {
                    match self.arguments.get(self.scan) {
                        Some(next) => {
                            value = Some(next.to_string_lossy().into_owned());
                            self.scan += 1;
                        }
                        None => {
                            return Err(UsageFailure(long_requires_argument_message(
                                &self.program, option,
                            )))
                        }
                    }
                }
            }
            // An optional value is only ever attached with '='; a following
            // word is the next operand.
            Arity::Optional => {}
        }
        Ok(Some(match option {
            "ignore-environment" => Request::IgnoreEnvironment,
            "null" => Request::NullTerminateOutput,
            "debug" => Request::Debug,
            "list-signal-handling" => Request::ListSignalHandling,
            "help" => Request::Help,
            "version" => Request::Version,
            "unset" => Request::Unset(value.unwrap_or_default()),
            "chdir" => Request::Chdir(value.unwrap_or_default()),
            "split-string" => Request::Split(value.unwrap_or_default()),
            "block-signal" => Request::BlockSignal(value),
            "default-signal" => Request::DefaultSignal(value),
            _ => Request::IgnoreSignal(value),
        }))
    }
}

// ------------------------------------------------------- signal dispositions --

/// What a signal should end up handled like.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Unchanged,
    Default,
    DefaultNoerr,
    Ignore,
    IgnoreNoerr,
}

/// `strtok` on `,`: runs of commas separate, and empty fields vanish.
fn strtok_commas(text: &str) -> Vec<&str> {
    text.split(',').filter(|field| !field.is_empty()).collect()
}

/// Look one signal operand up, rejecting signal 0 the way `env.c` does.
///
/// `Err` carries the offending operand, which is what the diagnostic quotes.
fn resolve(operand: &str) -> Result<i32, String> {
    match signal_number(operand) {
        // `operand2sig` accepts EXIT as 0 and `env` refuses it, as it refuses
        // anything the table does not name.
        Some(0) | None => Err(operand.to_string()),
        Some(number) => Ok(number),
    }
}

/// Record what `--default-signal` / `--ignore-signal` asked for.
///
/// Without a value every named signal is set, and failures are ignored, because
/// KILL and STOP cannot be touched.
pub fn parse_signal_actions(
    actions: &mut [Action],
    value: Option<&str>,
    default: bool,
) -> Result<(), String> {
    match value {
        None => {
            let action = if default {
                Action::DefaultNoerr
            } else {
                Action::IgnoreNoerr
            };
            for number in 1..actions.len() {
                if is_named_signal(number as i32) {
                    actions[number] = action;
                }
            }
        }
        Some(text) => {
            let action = if default { Action::Default } else { Action::Ignore };
            for operand in strtok_commas(text) {
                let number = resolve(operand)?;
                if number >= 1 && (number as usize) < actions.len() {
                    actions[number as usize] = action;
                }
            }
        }
    }
    Ok(())
}

/// Which way `--block-signal` and `--default-signal` push a signal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    Block,
    Unblock,
}

/// Record what `--block-signal` asked for.
///
/// Without a value the whole side of the mask is filled. A later mention of a
/// signal wins over an earlier one, which is why both sets are touched: the
/// chosen direction is set and the other one cleared.
///
/// `--default-signal` calls this with the other direction, because the system
/// tool unblocks the signals it is about to reset.
pub fn parse_mask_signals(
    block: &mut [bool],
    unblock: &mut [bool],
    value: Option<&str>,
    direction: Direction,
) -> Result<(), String> {
    match value {
        None => {
            for number in 1..block.len() {
                block[number] = direction == Direction::Block;
                unblock[number] = direction == Direction::Unblock;
            }
        }
        Some(text) => {
            for operand in strtok_commas(text) {
                let number = resolve(operand)?;
                if number >= 1 && (number as usize) < block.len() {
                    block[number as usize] = direction == Direction::Block;
                    unblock[number as usize] = direction == Direction::Unblock;
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- messages --

pub fn invalid_option_message(program: &str, option: char) -> String {
    format!("{}: invalid option -- '{}'", program, option)
}

pub fn requires_argument_message(program: &str, option: char) -> String {
    format!("{}: option requires an argument -- '{}'", program, option)
}

pub fn unrecognized_option_message(program: &str, name: &str) -> String {
    format!("{}: unrecognized option '--{}'", program, name)
}

pub fn long_requires_argument_message(program: &str, name: &str) -> String {
    format!("{}: option '--{}' requires an argument", program, name)
}

pub fn long_rejects_argument_message(program: &str, name: &str) -> String {
    format!("{}: option '--{}' doesn't allow an argument", program, name)
}

pub fn ambiguous_option_message(program: &str, given: &str, candidates: &[&str]) -> String {
    let mut message = format!("{}: option '--{}' is ambiguous; possibilities:", program, given);
    for candidate in candidates {
        message.push_str(&format!(" '--{}'", candidate));
    }
    message
}

pub fn try_help_message(program: &str) -> String {
    format!("Try '{} --help' for more information.", program)
}

pub fn invalid_signal_message(program: &str, operand: &str) -> String {
    format!("{}: {}: invalid signal", program, quoting::quote(operand))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(text: &str) -> Result<Vec<String>, SplitError> {
        split_string(text, &|_| None, &mut |_| {}).map(|split| split.args)
    }

    fn split_with(text: &str, name: &str, value: &str) -> Result<Vec<String>, SplitError> {
        split_string(
            text,
            &|key| {
                if key == name {
                    Some(value.to_string())
                } else {
                    None
                }
            },
            &mut |_| {},
        )
        .map(|split| split.args)
    }

    #[test]
    fn split_plain_words() {
        assert_eq!(split("A=1 B=2 C=3").unwrap(), ["A=1", "B=2", "C=3"]);
        assert_eq!(split("  A=1   B=2  ").unwrap(), ["A=1", "B=2"]);
        assert_eq!(split("A=1\nB=2\tC=3").unwrap(), ["A=1", "B=2", "C=3"]);
    }

    #[test]
    fn split_empty_string_gives_no_arguments() {
        // The machine starts with the separator set, but nothing ever arrives to
        // close a first argument.
        assert!(split("").unwrap().is_empty());
        assert!(split(" ").unwrap().is_empty());
        assert_eq!(split("A=1 ").unwrap(), ["A=1"]);
    }

    #[test]
    fn split_quotes_produce_empty_arguments() {
        // A quote calls start_new_arg itself, so a quoted nothing is an argument.
        assert_eq!(split("''").unwrap(), [""]);
        assert_eq!(split("\"\"").unwrap(), [""]);
        assert_eq!(split("\"\"").unwrap().len(), 1);
        assert_eq!(split("\"").err(), Some(SplitError::UnterminatedQuote));
        assert_eq!(split("a 'b' c").unwrap(), ["a", "b", "c"]);
        assert_eq!(split("'a b'").unwrap(), ["a b"]);
        assert_eq!(split("a\"b\"c").unwrap(), ["abc"]);
        // A quote inside the other kind of quote is just a character.
        assert_eq!(split("\"'\"").unwrap(), ["'"]);
        assert_eq!(split("'\"'").unwrap(), ["\""]);
    }

    #[test]
    fn split_hash_ends_the_string_only_at_an_argument_start() {
        assert_eq!(split("A=1 B=2").unwrap(), ["A=1", "B=2"]);
        assert_eq!(split("A=1 # B=2").unwrap(), ["A=1"]);
        assert_eq!(split("a#b c").unwrap(), ["a#b", "c"]);
        // The jump for '#' skips the unterminated-quote check, so an unclosed
        // quote is not reported once a '#' has ended the string.
        assert_eq!(split("'#'x").unwrap(), ["#x"]);
        assert_eq!(split("a #").unwrap(), ["a"]);
        // Nothing was ever appended, so there is no argument at all.
        assert!(split("#").unwrap().is_empty());
    }

    #[test]
    fn split_backslash_escapes() {
        // The escape list is short and surprising: no \a, no \b, no line
        // continuation, and no way to escape a space (use quotes for that).
        assert_eq!(split("a\\ b"), Err(SplitError::InvalidSequence { character: ' ' }));
        assert_eq!(split("a\\ab"), Err(SplitError::InvalidSequence { character: 'a' }));
        assert_eq!(split("a\\bb"), Err(SplitError::InvalidSequence { character: 'b' }));
        assert_eq!(split("a\\tb").unwrap(), ["a\tb"]);
        assert_eq!(split("a\\nb").unwrap(), ["a\nb"]);
        assert_eq!(split("a\\rb").unwrap(), ["a\rb"]);
        assert_eq!(split("a\\vb").unwrap(), ["a\x0bb"]);
        assert_eq!(split("a\\fb").unwrap(), ["a\x0cb"]);
        // These five pass through as themselves.
        assert_eq!(split("a\\'b").unwrap(), ["a'b"]);
        assert_eq!(split("a\\\"b").unwrap(), ["a\"b"]);
        assert_eq!(split("a\\#b").unwrap(), ["a#b"]);
        assert_eq!(split("a\\$b").unwrap(), ["a$b"]);
        assert_eq!(split("a\\\\b").unwrap(), ["a\\b"]);
        assert_eq!(split("a\\"), Err(SplitError::BackslashAtEnd));
        assert_eq!(split("\\"), Err(SplitError::BackslashAtEnd));
    }

    #[test]
    fn split_underscore_and_c_escapes() {
        // `\_` outside quotes separates arguments like whitespace does, and
        // like whitespace it does not close an argument by itself.
        assert_eq!(split("a\\_b").unwrap(), ["a", "b"]);
        assert_eq!(split("a\\_ b").unwrap(), ["a", "b"]);
        assert_eq!(split("\\_a").unwrap(), ["a"]);
        // Inside double quotes it is a space instead.
        assert_eq!(split("\"a\\_b\"").unwrap(), ["a b"]);
        // `\c` outside quotes ends the string outright.
        assert_eq!(split("a\\cb").unwrap(), ["a"]);
        assert_eq!(split("a\\c b").unwrap(), ["a"]);
        // ...but it is refused inside double quotes.
        assert_eq!(
            split("\"a\\cb\""),
            Err(SplitError::ControlInsideDoubleQuotes)
        );
        // Inside single quotes a backslash is inert unless it escapes a
        // backslash or a quote, so `\c` does not end the string there and the
        // quote is still reported as unterminated.
        assert_eq!(split("'a\\cb"), Err(SplitError::UnterminatedQuote));
    }

    #[test]
    fn split_backslash_inside_single_quotes() {
        // Literal, unless it escapes a backslash or the closing quote; and then
        // only with the escape sequences the list actually has.
        assert_eq!(split("'a\\b'").unwrap(), ["a\\b"]);
        assert_eq!(split("'a\\ab'").unwrap(), ["a\\ab"]);
        assert_eq!(split("'a\\\\b'").unwrap(), ["a\\b"]);
        assert_eq!(split("'a\\'b'").unwrap(), ["a'b"]);
        assert_eq!(split("'a\\tb'").unwrap(), ["a\\tb"]);
    }

    #[test]
    fn split_expands_varname_only_when_set() {
        assert_eq!(split_with("${HOME}/x", "HOME", "/root").unwrap(), ["/root/x"]);
        assert_eq!(split_with("${A}", "A", "").unwrap(), [""]);
        assert_eq!(split_with("pre${A}", "A", "1").unwrap(), ["pre1"]);
        // An unset variable contributes nothing and does not split the argument.
        assert_eq!(split_with("${NOPE}x", "OTHER", "1").unwrap(), ["x"]);
        assert_eq!(split_with("${A}${B}", "A", "1").unwrap(), ["1"]);
        // Two expansions of the same variable in one argument.
        assert_eq!(split_with("${A}${A}", "A", "z").unwrap(), ["zz"]);
        // A set variable does start a new argument after a separator.
        assert_eq!(split_with("a ${A}", "A", "b").unwrap(), ["a", "b"]);
    }

    #[test]
    fn split_varname_grammar() {
        // The name must start with a letter or underscore.
        for text in ["${1x}", "${ x}", "${}", "${a", "$HOME", "${a-b}"] {
            assert_eq!(
                split(text),
                Err(SplitError::BadVarName {
                    rest: text.to_string()
                }),
                "for {text:?}"
            );
        }
        // A leading underscore is a good name, so `${_a}` is an expansion that
        // finds nothing and adds nothing.
        assert_eq!(split("${_a}").unwrap(), Vec::<String>::new());
        assert_eq!(split_with("${_a}x", "_a", "Z").unwrap(), ["Zx"]);
        // Found but unset, so the name contributes nothing and the argument
        // is whatever surrounds it.
        assert_eq!(split("${A9_z}b").unwrap(), ["b"]);
        assert_eq!(split_with("${A9_z}b", "A9_z", "V").unwrap(), ["Vb"]);
        // A dollar inside single quotes stays put.
        assert_eq!(split_with("'${A}'", "A", "1").unwrap(), ["${A}"]);
    }

    #[test]
    fn split_notes_report_expansions_as_they_happen() {
        let mut notes = Vec::new();
        let outcome = split_string(
            "${A} ${B}",
            &|key| {
                if key == "A" {
                    Some("1".to_string())
                } else {
                    None
                }
            },
            &mut |note| notes.push(note.to_string()),
        );
        assert_eq!(
            outcome.unwrap().args,
            ["1".to_string()]
        );
        // Nothing sets the locale here, so the plain quoting shape is what comes
        // out; `quote_for` covers the other one.
        assert_eq!(
            notes,
            [
                "expanding ${A} into '1'\n".to_string(),
                "replacing ${B} with null string\n".to_string()
            ]
        );

        // A string that expands and is then malformed keeps the note, because the
        // system tool has already printed it by the time it finds the problem.
        let mut notes = Vec::new();
        let outcome = split_string("${A} 'x", &|_| Some("1".to_string()), &mut |note| {
            notes.push(note.to_string())
        });
        assert_eq!(outcome, Err(SplitError::UnterminatedQuote));
        assert_eq!(notes, ["expanding ${A} into '1'\n".to_string()]);
    }

    #[test]
    fn signals_have_gnulib_names() {
        assert_eq!(signal_name(1).unwrap(), "HUP");
        assert_eq!(signal_name(9).unwrap(), "KILL");
        assert_eq!(signal_name(15).unwrap(), "TERM");
        assert_eq!(signal_name(31).unwrap(), "SYS");
        assert_eq!(signal_name(32), None);
        assert_eq!(signal_name(33), None);
        assert_eq!(signal_name(34).unwrap(), "RTMIN");
        assert_eq!(signal_name(49).unwrap(), "RTMIN+15");
        assert_eq!(signal_name(50).unwrap(), "RTMAX-14");
        assert_eq!(signal_name(64).unwrap(), "RTMAX");
        assert_eq!(signal_name(0), None);
        assert_eq!(signal_name(65), None);
    }

    #[test]
    fn signal_lookup_accepts_names_numbers_and_aliases() {
        assert_eq!(signal_number("INT"), Some(2));
        assert_eq!(signal_number("int"), Some(2));
        assert_eq!(signal_number("SIGINT"), Some(2));
        assert_eq!(signal_number("sigint"), Some(2));
        assert_eq!(signal_number("2"), Some(2));
        assert_eq!(signal_number("02"), Some(2));
        assert_eq!(signal_number("SIG2"), Some(2));
        // Aliases: IOT is ABRT, CLD is CHLD, IO is POLL.
        assert_eq!(signal_number("IOT"), Some(6));
        assert_eq!(signal_number("SIGIOT"), Some(6));
        assert_eq!(signal_number("CLD"), Some(17));
        assert_eq!(signal_number("IO"), Some(29));
        // UNUSED is not an alias here.
        assert_eq!(signal_number("UNUSED"), None);
        // The two reserved signals have no name and no number.
        assert_eq!(signal_number("32"), None);
        assert_eq!(signal_number("65"), None);
        // EXIT resolves to 0, which env rejects on its own. SIGEXIT takes
        // the same route: strip the prefix and find EXIT.
        assert_eq!(signal_number("EXIT"), Some(0));
        assert_eq!(signal_number("SIGEXIT"), Some(0));
        assert_eq!(signal_number("exit"), Some(0));
        // Junk, with and without whitespace.
        assert_eq!(signal_number(""), None);
        assert_eq!(signal_number("SIG"), None);
        assert_eq!(signal_number("INTX"), None);
        assert_eq!(signal_number("INT "), None);
        assert_eq!(signal_number(" INT"), None);
        assert_eq!(signal_number(" 2"), None);
        assert_eq!(signal_number("+2"), None);
        assert_eq!(signal_number("-2"), None);
        assert_eq!(signal_number("2x"), None);
        assert_eq!(signal_number("0x2"), None);
        assert_eq!(signal_number("999999999999999999999"), None);
    }

    #[test]
    fn signal_lookup_handles_realtime_arithmetic() {
        assert_eq!(signal_number("RTMIN"), Some(34));
        assert_eq!(signal_number("SIGRTMAX"), Some(64));
        assert_eq!(signal_number("RTMIN+0"), Some(34));
        assert_eq!(signal_number("rtmin+1"), Some(35));
        assert_eq!(signal_number("SIGRTMIN+1"), Some(35));
        // Spaces may sit before the sign and nowhere else.
        assert_eq!(signal_number("RTMIN +2"), Some(36));
        assert_eq!(signal_number("RTMIN   +2"), Some(36));
        assert_eq!(signal_number("RTMAX -3"), Some(61));
        assert_eq!(signal_number("RTMIN + 2"), None);
        assert_eq!(signal_number("RTMIN+ 2"), None);
        assert_eq!(signal_number(" RTMIN+2"), None);
        assert_eq!(signal_number("RTMIN+2 "), None);
        // The sign has to agree with the word.
        assert_eq!(signal_number("RTMIN-1"), None);
        assert_eq!(signal_number("RTMAX+1"), None);
        assert_eq!(signal_number("RTMIN++2"), None);
        assert_eq!(signal_number("RTMIN+2+3"), None);
        assert_eq!(signal_number("RTMAX-3-1"), None);
        assert_eq!(signal_number("RTMIN+2x"), None);
        // The result is validated against the table rather than the offset, so
        // arithmetic that lands back inside the range is accepted.
        assert_eq!(signal_number("RTMIN+30"), Some(64));
        assert_eq!(signal_number("RTMAX-30"), Some(34));
        assert_eq!(signal_number("RTMAX-0"), Some(64));
        assert_eq!(signal_number("RTMIN+31"), None);
        assert_eq!(signal_number("RTMIN+100"), None);
        assert_eq!(signal_number("RTMIN+1000000000000"), None);
    }

    #[test]
    fn messages_match_the_diagnostics() {
        assert_eq!(
            invalid_option_message("env", 'Q'),
            "env: invalid option -- 'Q'"
        );
        assert_eq!(
            requires_argument_message("env", 'u'),
            "env: option requires an argument -- 'u'"
        );
        assert_eq!(
            unrecognized_option_message("env", "nonsense"),
            "env: unrecognized option '--nonsense'"
        );
        assert_eq!(
            long_requires_argument_message("env", "unset"),
            "env: option '--unset' requires an argument"
        );
        assert_eq!(
            long_rejects_argument_message("env", "null"),
            "env: option '--null' doesn't allow an argument"
        );
        assert_eq!(
            ambiguous_option_message("env", "i", &["ignore-environment", "ignore-signal"]),
            "env: option '--i' is ambiguous; possibilities: \
             '--ignore-environment' '--ignore-signal'"
        );
        assert_eq!(
            try_help_message("env"),
            "Try 'env --help' for more information."
        );
        assert_eq!(
            invalid_signal_message("env", "ZZZ"),
            "env: 'ZZZ': invalid signal"
        );
        assert_eq!(
            SplitError::UnterminatedQuote.message(),
            "env: no terminating quote in -S string"
        );
        assert_eq!(
            SplitError::BackslashAtEnd.message(),
            "env: invalid backslash at end of string in -S"
        );
        assert_eq!(
            SplitError::InvalidSequence { character: 'q' }.message(),
            "env: invalid sequence '\\q' in -S"
        );
        assert_eq!(
            SplitError::BadVarName {
                rest: "${1x}".to_string()
            }
            .message(),
            "env: only ${VARNAME} expansion is supported, error at: ${1x}"
        );
    }
}