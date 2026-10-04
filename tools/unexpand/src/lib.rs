//! `unexpand(1)`: convert spaces to tabs.
//!
//! The hard part is not the substitution but the bookkeeping: a run of blanks
//! is only turned into a tab once it is known that the run reaches a tab stop,
//! so the blanks are held until something non-blank proves the answer.
//! `unexpand.c` keeps them in `pending_blank` with two flags:
//!
//! * `prev_blank` — whether the previous character was a blank. It starts
//!   **true**, so a run of leading blanks is treated as if the line had been
//!   preceded by one.
//! * `one_blank_before_tab_stop` — whether the run began one column before a
//!   stop. When it did, and the run ends up longer than one, the first blank
//!   still becomes a tab and only the tail is written out.
//!
//! That second flag is why `printf 'a        b' | unexpand -a` gives `a<TAB> b`
//! rather than `a<TAB>`: the blank that landed on column 8 was the run's
//! *eighth*, and the one before it cannot be folded into the tab that replaced
//! it.
//!
//! Backspaces move the column back, and the operands are one stream, so a file
//! without a trailing newline carries its column and its convert flag into the
//! next one.
//!
//! The tab-stop grammar is the one `expand` uses and lives in `expand-common.c`;
//! this is a second copy of it, kept separate because every crate here stands on
//! its own. The two should end up sharing one module.

/// Which columns the tabs stop at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabStops {
    /// Zero when the explicit `stops` decide, non-zero when every stop is that
    /// many columns apart.
    pub size: u64,
    /// Spacing to keep after the last explicit stop, from `/N`.
    pub extend: u64,
    /// Increment for each stop past the last one, from `+N`.
    pub increment: u64,
    /// The explicit column numbers.
    pub stops: Vec<u64>,
}

impl Default for TabStops {
    /// The manual's default of eight columns.
    fn default() -> Self {
        TabStops {
            size: 8,
            extend: 0,
            increment: 0,
            stops: Vec::new(),
        }
    }
}

/// Why a `-t` value was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StopError {
    /// A `/` or `+` turned up after a number had already started.
    MisplacedSpecifier { specifier: char, rest: String },
    /// A character that can be neither a digit nor a separator.
    InvalidCharacter { rest: String },
    /// A number too big for `uintmax_t`.
    TooLarge { digits: String },
    /// A second `/` or `+`, which only the last value may carry.
    RepeatedSpecifier { specifier: char },
}

impl StopError {
    pub fn message(&self) -> String {
        match self {
            StopError::MisplacedSpecifier { specifier, rest } => format!(
                "unexpand: '{}' specifier not at start of number: {}",
                specifier,
                quoted(rest)
            ),
            StopError::InvalidCharacter { rest } => format!(
                "unexpand: tab size contains invalid character(s): {}",
                quoted(rest)
            ),
            StopError::TooLarge { digits } => {
                format!("unexpand: tab stop is too large {}", quoted(digits))
            }
            StopError::RepeatedSpecifier { specifier } => format!(
                "unexpand: '{}' specifier only allowed with the last value",
                specifier
            ),
        }
    }
}

/// coreutils wraps a diagnostic value in typographic quotes.
fn quoted(text: &str) -> String {
    format!("\u{2018}{}\u{2019}", text)
}

impl TabStops {
    /// Fold a whole `-t` value into the stop list.
    ///
    /// Returns every complaint, not just the first: a specifier in the wrong
    /// place is reported and the scan carries on, so `-t 4+x` produces both the
    /// misplaced `+` and the invalid `x`.
    pub fn parse(&mut self, text: &str) -> Vec<StopError> {
        let bytes = text.as_bytes();
        let mut errors: Vec<StopError> = Vec::new();
        let mut index = 0usize;
        let mut have_value = false;
        let mut value: u64 = 0;
        let mut extend = false;
        let mut increment = false;
        let mut number_start = 0usize;

        while index < bytes.len() {
            let byte = bytes[index];
            if byte == b',' || byte == b' ' || byte == b'\t' {
                if have_value {
                    if let Some(error) = self.commit(value, extend, increment) {
                        errors.push(error);
                        break;
                    }
                    have_value = false;
                }
                index += 1;
                continue;
            }
            if byte == b'/' || byte == b'+' {
                let specifier = byte as char;
                if have_value {
                    errors.push(StopError::MisplacedSpecifier {
                        specifier,
                        rest: text[index..].to_string(),
                    });
                }
                extend = specifier == '/';
                increment = specifier == '+';
                index += 1;
                continue;
            }
            if byte.is_ascii_digit() {
                if !have_value {
                    value = 0;
                    have_value = true;
                    number_start = index;
                }
                match value
                    .checked_mul(10)
                    .and_then(|scaled| scaled.checked_add(u64::from(byte - b'0')))
                {
                    Some(next) => value = next,
                    None => {
                        index += 1;
                        while index < bytes.len() && bytes[index].is_ascii_digit() {
                            index += 1;
                        }
                        errors.push(StopError::TooLarge {
                            digits: text[number_start..index].to_string(),
                        });
                        have_value = false;
                        continue;
                    }
                }
                index += 1;
                continue;
            }
            errors.push(StopError::InvalidCharacter {
                rest: text[index..].to_string(),
            });
            break;
        }
        if errors.is_empty() && have_value {
            if let Some(error) = self.commit(value, extend, increment) {
                errors.push(error);
            }
        }
        errors
    }

    fn commit(&mut self, value: u64, extend: bool, increment: bool) -> Option<StopError> {
        if extend {
            if self.extend != 0 {
                return Some(StopError::RepeatedSpecifier { specifier: '/' });
            }
            self.extend = value;
        } else if increment {
            if self.increment != 0 {
                return Some(StopError::RepeatedSpecifier { specifier: '+' });
            }
            self.increment = value;
        } else {
            self.stops.push(value);
        }
        None
    }

    /// Decide what the stop list means, and refuse the impossible ones.
    pub fn finalize(&mut self) -> Result<(), String> {
        let mut previous = 0u64;
        for stop in &self.stops {
            if *stop == 0 {
                return Err("unexpand: tab size cannot be 0".to_string());
            }
            if *stop <= previous {
                return Err("unexpand: tab sizes must be ascending".to_string());
            }
            previous = *stop;
        }
        if self.increment != 0 && self.extend != 0 {
            return Err("unexpand: '/' specifier is mutually exclusive with '+'".to_string());
        }
        self.size = match self.stops.len() {
            0 => {
                if self.extend != 0 {
                    self.extend
                } else if self.increment != 0 {
                    self.increment
                } else {
                    8
                }
            }
            1 if self.extend == 0 && self.increment == 0 => self.stops[0],
            _ => 0,
        };
        Ok(())
    }

    /// The column the next tab from `column` expands to.
    ///
    /// None means there is no stop left, which the caller reads as "stop
    /// converting this line". The cursor only moves past a stop once the column
    /// has reached it, so a stop that is still ahead is looked at again on the
    /// next call.
    pub fn next_stop(&self, column: u64, index: &mut usize) -> Option<u64> {
        if self.size != 0 {
            return Some(column + (self.size - column % self.size));
        }
        while *index < self.stops.len() {
            let stop = self.stops[*index];
            if column < stop {
                return Some(stop);
            }
            *index += 1;
        }
        if self.extend != 0 {
            return Some(column + (self.extend - column % self.extend));
        }
        if self.increment != 0 {
            let last = self.stops[self.stops.len() - 1];
            return Some(column + (self.increment - ((column - last) % self.increment)));
        }
        None
    }
}

/// What the run converts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// The default: only the blanks at the start of a line.
    FirstOnly,
    /// `-a`: every blank in the line.
    All,
    /// `-t` without `--first-only`, which turns `-a` on by itself.
    AllFromTabs,
}

impl Scope {
    /// Whether a non-blank ends the conversion for the rest of the line.
    fn converts_whole_line(self) -> bool {
        !matches!(self, Scope::FirstOnly)
    }
}

/// The state of the line being converted.
struct Line {
    column: u64,
    next_tab_column: u64,
    tab_index: usize,
    one_blank_before_tab_stop: bool,
    /// Starts true, so a leading run of blanks counts as if the line had been
    /// preceded by one.
    prev_blank: bool,
    pending: Vec<u8>,
    convert: bool,
}

impl Line {
    fn new() -> Line {
        Line {
            column: 0,
            next_tab_column: 0,
            tab_index: 0,
            one_blank_before_tab_stop: false,
            prev_blank: true,
            pending: Vec::new(),
            convert: true,
        }
    }
}

/// Convert a whole run of input into `out`.
///
/// Callers concatenate the operands and call this once, because `unexpand.c`
/// reaches for the next file from inside its line loop.
pub fn unexpand(input: &[u8], stops: &TabStops, scope: Scope, out: &mut Vec<u8>) {
    let whole_line = scope.converts_whole_line();
    let mut state = Line::new();
    for byte in input {
        // A newline is not a blank, so it goes through the same path as any
        // other non-blank: it flushes the blanks that were waiting, and only
        // then ends the line.
        let blank = is_blank(*byte);
        // The blank that reaches a stop is replaced by a tab, and it is the
        // character that goes out in its place, so the substitution has to
        // reach the tail of the loop.
        let mut emitted = *byte;
        if state.convert {
            if blank {
                let last_tab = match stops.next_stop(state.column, &mut state.tab_index) {
                    Some(column) => {
                        state.next_tab_column = column;
                        false
                    }
                    None => true,
                };
                if last_tab {
                    // Nothing left to aim at, so the rest of the line is text.
                    state.convert = false;
                }
                if state.convert {
                    if *byte == b'\t' {
                        // An input tab jumps straight to the stop and takes over
                        // any blanks that were waiting.
                        state.column = state.next_tab_column;
                        if !state.pending.is_empty() {
                            state.pending[0] = b'\t';
                        }
                    } else {
                        state.column += 1;
                        if !(state.prev_blank && state.column == state.next_tab_column) {
                            // Not yet known whether this run reaches the stop.
                            if state.column == state.next_tab_column {
                                state.one_blank_before_tab_stop = true;
                            }
                            state.pending.push(*byte);
                            state.prev_blank = true;
                            continue;
                        }
                        // coreutils writes into a preallocated buffer, so slot
                        // zero always exists even when the count is zero.
                        poke(&mut state.pending, b'\t');
                        emitted = b'\t';
                    }
                    // A single blank just before a stop stays a blank, so only
                    // that one survives -- and when it did, the tab that
                    // replaces this blank is a *second* one.
                    state
                        .pending
                        .truncate(usize::from(state.one_blank_before_tab_stop));
                }
            } else if *byte == 0x08 {
                state.column = state.column.saturating_sub(1);
                state.next_tab_column = state.column;
                state.tab_index = state.tab_index.saturating_sub(1);
            } else {
                state.column += 1;
            }

            if !state.pending.is_empty() {
                if state.pending.len() > 1 && state.one_blank_before_tab_stop {
                    state.pending[0] = b'\t';
                }
                out.extend_from_slice(&state.pending);
                state.pending.clear();
                state.one_blank_before_tab_stop = false;
            }
            state.prev_blank = blank;
            state.convert &= whole_line || blank;
        }
        out.push(emitted);
        if *byte == b'\n' {
            // End of line: the column, the stop cursor and the convert flag all
            // start over, exactly as coreutils does at the top of its loop.
            state = Line::new();
        }
    }
    // End of input arrives as one last non-blank character, which is what makes
    // a run of blanks at the very end of the last file come out.
    if !state.pending.is_empty() {
        if state.pending.len() > 1 && state.one_blank_before_tab_stop {
            state.pending[0] = b'\t';
        }
        out.extend_from_slice(&state.pending);
        state.pending.clear();
    }
}

/// Whether the manual's "blank" includes this byte.
fn is_blank(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

/// Put `byte` in the first pending slot, creating it when the run is empty.
fn poke(pending: &mut Vec<u8>, byte: u8) {
    match pending.first_mut() {
        Some(slot) => *slot = byte,
        None => pending.push(byte),
    }
}

/// The diagnostics coreutils prints.
pub fn unrecognized_option_message(name: &str) -> String {
    format!("unexpand: unrecognized option '--{}'", name)
}

pub fn invalid_option_message(letter: char) -> String {
    format!("unexpand: invalid option -- '{}'", letter)
}

pub fn requires_argument_message(name: &str) -> String {
    format!("unexpand: option '--{}' requires an argument", name)
}

pub fn short_requires_argument_message(letter: char) -> String {
    format!("unexpand: option requires an argument -- '{}'", letter)
}

pub fn cannot_open_message(name: &str, reason: &str) -> String {
    format!("unexpand: {}: {}", name, reason)
}

pub fn try_help_message() -> String {
    "Try 'unexpand --help' for more information.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &str, stops: &TabStops, scope: Scope) -> String {
        let mut out = Vec::new();
        unexpand(input.as_bytes(), stops, scope, &mut out);
        String::from_utf8(out).unwrap()
    }

    fn stops_of(text: &str) -> TabStops {
        let mut stops = TabStops {
            size: 0,
            ..TabStops::default()
        };
        assert!(stops.parse(text).is_empty(), "{}", text);
        stops.finalize().unwrap();
        stops
    }

    #[test]
    fn every_case_agrees_with_gnu() {
        // One test for the whole grid, so a new case here cannot be split off
        // from the expectations that GNU produced for it.
        let eight = TabStops::default();
        let four = stops_of("4");
        let two = stops_of("2");
        let two_six = stops_of("2,6");
        let cases: &[(&str, &TabStops, Scope, &str)] = &[
            ("        a", &eight, Scope::FirstOnly, "\ta"),
            ("  b", &eight, Scope::FirstOnly, "  b"),
            ("  \tc", &eight, Scope::FirstOnly, "\tc"),
            ("\tc", &eight, Scope::FirstOnly, "\tc"),
            ("a  \nb  \n", &eight, Scope::FirstOnly, "a  \nb  \n"),
            ("        a\n        b\n", &eight, Scope::FirstOnly, "\ta\n\tb\n"),
            ("    a", &eight, Scope::FirstOnly, "    a"),
            ("a        b", &eight, Scope::All, "a\t b"),
            ("a \n", &eight, Scope::All, "a \n"),
            ("a  \n", &eight, Scope::All, "a  \n"),
            ("a        b\na \n", &eight, Scope::All, "a\t b\na \n"),
            ("a \n  b\n", &eight, Scope::All, "a \n  b\n"),
            ("a ", &eight, Scope::All, "a "),
            ("a  ", &eight, Scope::All, "a  "),
            ("a  ", &eight, Scope::FirstOnly, "a  "),
            ("        a", &eight, Scope::FirstOnly, "\ta"),
            ("        a", &eight, Scope::All, "\ta"),
            ("a       b", &eight, Scope::All, "a\tb"),
            ("a   b", &eight, Scope::All, "a   b"),
            ("a    b", &eight, Scope::All, "a    b"),
            ("  a  b", &eight, Scope::All, "  a  b"),
            (" \ta", &eight, Scope::All, "\ta"),
            ("\t a", &eight, Scope::All, "\t a"),
            ("a\t b", &eight, Scope::All, "a\t b"),
            ("a       \x08b", &eight, Scope::All, "a\t\x08b"),
            ("a       \x08\x08b", &eight, Scope::All, "a\t\x08\x08b"),
            ("\x08  a", &eight, Scope::All, "\x08  a"),
            ("        a", &four, Scope::FirstOnly, "\t\ta"),
            ("  b", &two, Scope::FirstOnly, "\tb"),
            ("        a", &two, Scope::FirstOnly, "\t\t\t\ta"),
            ("        a", &two_six, Scope::FirstOnly, "\t\t  a"),
            ("a  b", &two, Scope::All, "a\t b"),
            ("a   b", &two, Scope::All, "a\t\tb"),
            ("a    b", &two, Scope::All, "a\t\t b"),
            ("a        b", &two, Scope::All, "a\t\t\t\t b"),
            ("a b", &two, Scope::All, "a b"),
            (" b", &two, Scope::All, " b"),
            ("  a        b", &two, Scope::All, "\ta\t\t\t\t b"),
        ];
        for (input, stops, scope, expected) in cases {
            assert_eq!(
                &run(input, stops, *scope),
                *expected,
                "{:?} {:?}",
                input,
                scope
            );
        }
    }
    #[test]
    fn the_stop_cursor_only_moves_past_a_stop_the_column_reached() {
        // The stop at 2 is still ahead of column 0, so looking at it must not
        // consume it: a backspace can bring the column back to it.
        let stops = stops_of("2,6");
        let mut index = 0usize;
        assert_eq!(stops.next_stop(0, &mut index), Some(2));
        assert_eq!(index, 0, "a stop that is still ahead is not consumed");
        assert_eq!(stops.next_stop(1, &mut index), Some(2));
        assert_eq!(stops.next_stop(2, &mut index), Some(6));
        assert_eq!(index, 1);
        assert_eq!(stops.next_stop(6, &mut index), None);
    }

    #[test]
    fn a_single_spacing_never_consults_the_cursor() {
        let stops = stops_of("2");
        let mut index = 0usize;
        assert_eq!(stops.next_stop(0, &mut index), Some(2));
        assert_eq!(stops.next_stop(5, &mut index), Some(6));
        assert_eq!(index, 0);
    }

    #[test]
    fn the_end_of_input_acts_as_one_last_non_blank() {
        // A run of blanks at the very end of the last file still comes out,
        // because end of input is handled as a character and flushes them.
        let stops = TabStops::default();
        assert_eq!(run("a  ", &stops, Scope::All), "a  ");
        assert_eq!(run("        ", &stops, Scope::All), "\t");
        // A run that was already folded away has nothing left to flush.
        assert_eq!(run("a\t ", &stops, Scope::All), "a\t ");
    }

    #[test]
    fn only_a_newline_starts_a_new_line() {
        // Treating every non-blank as an end of line throws the column away in
        // the middle of a run, which loses a tab.
        let stops = TabStops::default();
        assert_eq!(run("a        b", &stops, Scope::All), "a\t b");
        assert_eq!(run("a  \n  b\n", &stops, Scope::FirstOnly), "a  \n  b\n");
        assert_eq!(run("        a\n        b\n", &stops, Scope::FirstOnly), "\ta\n\tb\n");
    }

    #[test]
    fn the_state_carries_over_into_the_next_file() {
        // The first file ends on a non-blank, so by default the rest is text
        // even though the next file starts with blanks.
        let stops = TabStops::default();
        let mut input = Vec::new();
        input.extend_from_slice(b"  a");
        input.extend_from_slice(b"  b\n");
        let mut out = Vec::new();
        unexpand(&input, &stops, Scope::FirstOnly, &mut out);
        assert_eq!(String::from_utf8(out).unwrap(), "  a  b\n");
        // With -a the blanks in the second file are folded in as well, though
        // two blanks still do not reach the stop at eight.
        let mut out = Vec::new();
        unexpand(&input, &stops, Scope::All, &mut out);
        assert_eq!(String::from_utf8(out).unwrap(), "  a  b\n");

        // A first file that ends on blanks leaves the run open into the next
        // one, so the eight blanks only appear once the two are put together.
        let mut input = Vec::new();
        input.extend_from_slice(b"      ");
        input.extend_from_slice(b"  a\n");
        let mut out = Vec::new();
        unexpand(&input, &stops, Scope::FirstOnly, &mut out);
        assert_eq!(String::from_utf8(out).unwrap(), "\ta\n");
    }

    #[test]
    fn stop_errors_match_coreutils() {
        assert_eq!(
            StopError::InvalidCharacter {
                rest: "x,2".to_string()
            }
            .message(),
            "unexpand: tab size contains invalid character(s): \u{2018}x,2\u{2019}"
        );
        assert_eq!(
            StopError::TooLarge {
                digits: "99999999999999999999".to_string()
            }
            .message(),
            "unexpand: tab stop is too large \u{2018}99999999999999999999\u{2019}"
        );
        let mut stops = TabStops {
            size: 0,
            ..TabStops::default()
        };
        assert_eq!(
            stops.parse("4+x"),
            vec![
                StopError::MisplacedSpecifier {
                    specifier: '+',
                    rest: "+x".to_string()
                },
                StopError::InvalidCharacter {
                    rest: "x".to_string()
                },
            ]
        );
    }

    #[test]
    fn messages_match_coreutils() {
        assert_eq!(
            unrecognized_option_message("nonsense"),
            "unexpand: unrecognized option '--nonsense'"
        );
        assert_eq!(invalid_option_message('Q'), "unexpand: invalid option -- 'Q'");
        assert_eq!(
            requires_argument_message("tabs"),
            "unexpand: option '--tabs' requires an argument"
        );
        assert_eq!(
            short_requires_argument_message('t'),
            "unexpand: option requires an argument -- 't'"
        );
        assert_eq!(
            cannot_open_message("nope", "No such file or directory"),
            "unexpand: nope: No such file or directory"
        );
        assert_eq!(try_help_message(), "Try 'unexpand --help' for more information.");
    }
}
