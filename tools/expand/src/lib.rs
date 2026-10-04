//! `expand(1)`: convert tabs to spaces.
//!
//! A tab expands to the spaces that carry the line from its current column up to
//! the next tab stop. Which stop that is comes from `-t`, and `-t` has three
//! shapes that `expand-common.c` keeps in three separate variables:
//!
//! * a single number, which becomes the spacing of every stop;
//! * a comma or blank separated list of explicit column numbers, after which
//!   any further tab is worth exactly one space;
//! * that list plus a trailing `/N` (keep spacing N after the last stop) or
//!   `+N` (each further stop N further on than the last one).
//!
//! Two rules the manual does not spell out:
//!
//! * `/` and `+` belong *in front of* the number they apply to, not after it.
//!   `-t 2/3` is an error; `-t 2,/3` and a bare `-t /3` are not.
//! * A backspace moves the column back and rewinds the tab-stop cursor, so the
//!   backspaces are echoed *and* the tab after them lands earlier.

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
                "expand: '{}' specifier not at start of number: {}",
                specifier,
                quoted(rest)
            ),
            StopError::InvalidCharacter { rest } => format!(
                "expand: tab size contains invalid character(s): {}",
                quoted(rest)
            ),
            StopError::TooLarge { digits } => {
                format!("expand: tab stop is too large {}", quoted(digits))
            }
            StopError::RepeatedSpecifier { specifier } => format!(
                "expand: '{}' specifier only allowed with the last value",
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
    /// Add one explicit stop, keeping the widest gap for the overflow check.
    pub fn add_stop(&mut self, value: u64) {
        self.stops.push(value);
    }

    /// Fold a whole `-t` value into the stop list.
    ///
    /// `,` and blanks separate the values. `/` and `+` set a flag that applies
    /// to the value that follows, so they have to come before the digits.
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
                        // coreutils keeps the digits it managed to read and
                        // carries on past them, so one `-t` can report an
                        // overflow and still complain about what follows.
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
        // A value still pending at the end is only taken when nothing went
        // wrong, because coreutils skips the whole tail once it has failed.
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
            self.add_stop(value);
        }
        None
    }

    /// Decide what the stop list means, and refuse the impossible ones.
    ///
    /// Called once the whole command line has been read, because the checks are
    /// about the finished list rather than any single `-t`.
    pub fn finalize(&mut self) -> Result<(), String> {
        let mut previous = 0u64;
        for stop in &self.stops {
            if *stop == 0 {
                return Err("expand: tab size cannot be 0".to_string());
            }
            if *stop <= previous {
                return Err("expand: tab sizes must be ascending".to_string());
            }
            previous = *stop;
        }
        if self.increment != 0 && self.extend != 0 {
            return Err("expand: '/' specifier is mutually exclusive with '+'".to_string());
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
            // One stop with no tail is just a spacing; anything more has to be
            // looked up position by position.
            1 if self.extend == 0 && self.increment == 0 => self.stops[0],
            _ => 0,
        };
        Ok(())
    }

    /// The column the next tab from `column` expands to.
    ///
    /// None means there is no stop left, which the caller turns into a single
    /// space. The cursor only moves past a stop once the column has reached it,
    /// so a stop that is still ahead is looked at again on the next call.
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

/// Expand a whole run of input into `out`.
///
/// The operands are one character stream, not one stream each: `expand.c` only
/// reaches for the next file from inside its line loop, so a file that does not
/// end in a newline carries its column and its tab-stop cursor into the next
/// one. Callers concatenate the operands and call this once.
///
/// `initial_only` is `-i`: a tab stops being converted once the line has
/// something that is not a blank on it.
pub fn expand(input: &[u8], stops: &TabStops, initial_only: bool, out: &mut Vec<u8>) {
    let mut column = 0u64;
    let mut index = 0usize;
    // Reset per line: the tab-stop cursor and the column both start over.
    let mut convert = true;
    for byte in input {
        if *byte == b'\n' {
            out.push(b'\n');
            column = 0;
            index = 0;
            convert = true;
            continue;
        }
        if convert {
            match *byte {
                b'\t' => {
                    let next = stops.next_stop(column, &mut index).unwrap_or(column + 1);
                    // The tab itself is replaced, and the spaces take its place.
                    for _ in column..next {
                        out.push(b' ');
                    }
                    column = next;
                    // The converted character is a space, which is a blank, so
                    // -i keeps converting after a tab.
                }
                0x08 => {
                    // Echoed, and it rewinds both counters.
                    column = column.saturating_sub(1);
                    index = index.saturating_sub(1);
                    out.push(0x08);
                }
                _ => {
                    column += 1;
                    out.push(*byte);
                }
            }
            convert &= !initial_only || is_blank(*byte) || *byte == b'\t';
            continue;
        }
        out.push(*byte);
    }
}

/// Whether the manual's "non blanks" includes this byte.
fn is_blank(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

/// The diagnostics coreutils prints.
pub fn unrecognized_option_message(name: &str) -> String {
    format!("expand: unrecognized option '--{}'", name)
}

pub fn invalid_option_message(letter: char) -> String {
    format!("expand: invalid option -- '{}'", letter)
}

pub fn requires_argument_message(name: &str) -> String {
    format!("expand: option '--{}' requires an argument", name)
}

pub fn short_requires_argument_message(letter: char) -> String {
    format!("expand: option requires an argument -- '{}'", letter)
}

pub fn cannot_open_message(name: &str, reason: &str) -> String {
    format!("expand: {}: {}", name, reason)
}

pub fn try_help_message() -> String {
    "Try 'expand --help' for more information.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &str, stops: &TabStops, initial_only: bool) -> String {
        let mut out = Vec::new();
        expand(input.as_bytes(), stops, initial_only, &mut out);
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
    fn the_default_is_every_eight_columns() {
        let stops = TabStops::default();
        assert_eq!(stops.size, 8);
        assert_eq!(run("a\tb\n", &stops, false), "a       b\n");
        assert_eq!(run("\tc\n", &stops, false), "        c\n");
        assert_eq!(run("ab\tc\td\n", &stops, false), "ab      c       d\n");
    }

    #[test]
    fn a_single_stop_is_a_spacing() {
        assert_eq!(run("a\tb\n", &stops_of("4"), false), "a   b\n");
        assert_eq!(run("a\tb\n", &stops_of("1"), false), "a b\n");
        // A tab already sitting on a stop still moves on to the next one.
        assert_eq!(run("abcd\te\n", &stops_of("4"), false), "abcd    e\n");
    }

    #[test]
    fn an_explicit_list_is_consulted_position_by_position() {
        let stops = stops_of("2,6");
        assert_eq!(run("a\tb\n", &stops, false), "a b\n");
        assert_eq!(run("\tc\n", &stops, false), "  c\n");
        assert_eq!(run("ab\tc\td\n", &stops, false), "ab    c d\n");
    }

    #[test]
    fn a_tab_past_the_last_stop_is_worth_one_space() {
        let stops = stops_of("2,4");
        assert_eq!(run("abcdef\tz\n", &stops, false), "abcdef z\n");
    }

    #[test]
    fn a_slash_keeps_a_spacing_after_the_last_stop() {
        let stops = stops_of("2,4,/3");
        assert_eq!(run("a\tb\n", &stops, false), "a b\n");
        // Past column 4 the spacing of 3 takes over.
        assert_eq!(run("abcd\te\n", &stops, false), "abcd  e\n");
    }

    #[test]
    fn a_plus_moves_each_further_stop_on_by_one_increment() {
        let stops = stops_of("2,+3");
        assert_eq!(run("a\tb\n", &stops, false), "a b\n");
        assert_eq!(run("ab\tc\n", &stops, false), "ab   c\n");
    }

    #[test]
    fn a_specifier_alone_becomes_the_spacing() {
        assert_eq!(run("a\tb\n", &stops_of("/3"), false), "a  b\n");
        assert_eq!(run("a\tb\n", &stops_of("+2"), false), "a b\n");
    }

    #[test]
    fn a_specifier_has_to_come_before_its_number() {
        let mut stops = TabStops {
            size: 0,
            ..TabStops::default()
        };
        assert_eq!(
            stops.parse("2/3"),
            vec![StopError::MisplacedSpecifier {
                specifier: '/',
                rest: "/3".to_string()
            }]
        );
        assert!(stops.parse("2,/3").is_empty());
    }

    #[test]
    fn blanks_separate_the_list_too() {
        let stops = stops_of("2 6");
        assert_eq!(run("ab\tc\n", &stops, false), "ab    c\n");
    }

    #[test]
    fn a_trailing_separator_is_harmless() {
        assert_eq!(run("a\tb\n", &stops_of("4,"), false), "a   b\n");
        assert_eq!(run("a\tb\n", &stops_of(""), false), "a       b\n");
    }

    #[test]
    fn an_invalid_character_is_reported_with_the_rest_of_the_value() {
        let mut stops = TabStops {
            size: 0,
            ..TabStops::default()
        };
        assert_eq!(
            stops.parse("4x,2"),
            vec![StopError::InvalidCharacter {
                rest: "x,2".to_string()
            }]
        );
        // A misplaced specifier is reported and the scan carries on, so the
        // invalid character behind it is reported as well.
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
    fn a_number_too_big_is_reported_on_its_own() {
        let mut stops = TabStops {
            size: 0,
            ..TabStops::default()
        };
        assert_eq!(
            stops.parse("1,99999999999999999999"),
            vec![StopError::TooLarge {
                digits: "99999999999999999999".to_string()
            }]
        );
    }

    #[test]
    fn a_specifier_may_only_appear_once() {
        let mut stops = TabStops {
            size: 0,
            ..TabStops::default()
        };
        assert!(stops.parse("/3").is_empty());
        assert_eq!(
            stops.parse("/4"),
            vec![StopError::RepeatedSpecifier { specifier: '/' }]
        );
    }

    #[test]
    fn slash_and_plus_cannot_both_be_given() {
        let mut stops = TabStops {
            size: 0,
            ..TabStops::default()
        };
        assert!(stops.parse("/3").is_empty());
        assert!(stops.parse("+1").is_empty());
        assert_eq!(
            stops.finalize(),
            Err("expand: '/' specifier is mutually exclusive with '+'".to_string())
        );
    }

    #[test]
    fn stops_must_be_nonzero_and_rising() {
        let mut stops = TabStops {
            size: 0,
            ..TabStops::default()
        };
        assert!(stops.parse("0,4").is_empty());
        assert_eq!(stops.finalize(), Err("expand: tab size cannot be 0".to_string()));

        let mut stops = TabStops {
            size: 0,
            ..TabStops::default()
        };
        assert!(stops.parse("4,2").is_empty());
        assert_eq!(stops.finalize(), Err("expand: tab sizes must be ascending".to_string()));
    }

    #[test]
    fn several_options_accumulate_into_one_list() {
        let mut stops = TabStops {
            size: 0,
            ..TabStops::default()
        };
        assert!(stops.parse("2").is_empty());
        assert!(stops.parse("4").is_empty());
        stops.finalize().unwrap();
        assert_eq!(stops.stops, vec![2, 4]);
        assert_eq!(stops.size, 0);
    }

    #[test]
    fn initial_only_stops_at_the_first_non_blank() {
        let stops = TabStops::default();
        assert_eq!(run("a\tb\n", &stops, true), "a\tb\n");
        // Leading blanks, tabs included, are still converted.
        assert_eq!(run("\tc\n", &stops, true), "        c\n");
        assert_eq!(run("  \tz\n", &stops, true), "        z\n");
        assert_eq!(run("ab\tc\td\n", &stops, true), "ab\tc\td\n");
    }

    #[test]
    fn a_backspace_rewinds_the_column_and_the_stop_cursor() {
        let stops = TabStops::default();
        // Three characters put the column at 3, the two backspaces take it back
        // to 1, and the tab then runs from 1 to 8: seven spaces.
        assert_eq!(run("abc\x08\x08\td\n", &stops, false), "abc\x08\x08       d\n");
        // -i has already stopped converting by then, so the tab survives.
        assert_eq!(run("abc\x08\x08\td\n", &stops, true), "abc\x08\x08\td\n");
    }

    #[test]
    fn backspaces_at_the_start_of_a_line_do_not_underflow() {
        let stops = TabStops::default();
        assert_eq!(run("\x08\ta\n", &stops, false), "\x08        a\n");
    }

    #[test]
    fn the_column_and_the_stops_start_over_on_every_line() {
        let stops = TabStops::default();
        assert_eq!(run("a\tb\nc\td\n", &stops, false), "a       b\nc       d\n");
    }

    #[test]
    fn a_missing_final_newline_is_not_added() {
        let stops = TabStops::default();
        assert_eq!(run("a\tb", &stops, false), "a       b");
    }

    #[test]
    fn the_column_carries_over_into_the_next_file() {
        // The operands are one stream, so a first file without a trailing
        // newline leaves the column where it got to: four characters and a
        // backspace put it at 4, and the tab in the next file runs from 10 to
        // 16.
        let stops = TabStops::default();
        let mut input = Vec::new();
        input.extend_from_slice(b"0\x08zZ Z\x080");
        input.extend_from_slice(b"-zZ0ba\t\x08\n");
        let mut out = Vec::new();
        expand(&input, &stops, false, &mut out);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "0\x08zZ Z\x080-zZ0ba      \x08\n"
        );
        // With a newline in between the column starts over, so the same tab
        // runs from 6 to 8 instead of from 10 to 16.
        let mut out = Vec::new();
        expand(b"abcd\n-zZ0ba\t\n", &stops, false, &mut out);
        assert_eq!(String::from_utf8(out).unwrap(), "abcd\n-zZ0ba  \n");
    }

    #[test]
    fn initial_only_starts_over_on_every_line() {
        // The leading tab of each line is still converted, because the line
        // starts out converting again; the tab after the "c" is not.
        let stops = TabStops::default();
        assert_eq!(
            run("\ta\tb\n\tx\ty\n", &stops, true),
            "        a\tb\n        x\ty\n"
        );
        // A tab on the very next line is converted even though the previous
        // line stopped converting part way through.
        assert_eq!(
            run("a\tb\n\tc\n", &stops, true),
            "a\tb\n        c\n"
        );
    }

    #[test]
    fn stop_error_messages_match_coreutils() {
        assert_eq!(
            StopError::MisplacedSpecifier {
                specifier: '/',
                rest: "/2,+3".to_string()
            }
            .message(),
            "expand: '/' specifier not at start of number: \u{2018}/2,+3\u{2019}"
        );
        assert_eq!(
            StopError::InvalidCharacter {
                rest: "x,2".to_string()
            }
            .message(),
            "expand: tab size contains invalid character(s): \u{2018}x,2\u{2019}"
        );
        assert_eq!(
            StopError::TooLarge {
                digits: "99999999999999999999".to_string()
            }
            .message(),
            "expand: tab stop is too large \u{2018}99999999999999999999\u{2019}"
        );
        assert_eq!(
            StopError::RepeatedSpecifier { specifier: '/' }.message(),
            "expand: '/' specifier only allowed with the last value"
        );
    }

    #[test]
    fn messages_match_coreutils() {
        assert_eq!(
            unrecognized_option_message("nonsense"),
            "expand: unrecognized option '--nonsense'"
        );
        assert_eq!(invalid_option_message('Q'), "expand: invalid option -- 'Q'");
        assert_eq!(
            requires_argument_message("tabs"),
            "expand: option '--tabs' requires an argument"
        );
        assert_eq!(
            short_requires_argument_message('t'),
            "expand: option requires an argument -- 't'"
        );
        assert_eq!(
            cannot_open_message("nope", "No such file or directory"),
            "expand: nope: No such file or directory"
        );
        assert_eq!(try_help_message(), "Try 'expand --help' for more information.");
    }
}
