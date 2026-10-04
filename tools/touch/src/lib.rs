//! Decision and date logic of `touch(1)`, following its manual page:
//!
//! ```text
//! touch [OPTION]... FILE...
//! ```
//!
//! The manual draws a line between the two date formats: `-d/--date` takes a
//! human readable string, while `-t` takes `[[CC]YY]MMDDhhmm[.ss]`. Both are
//! parsed here, into a plain broken-down time that `main.rs` hands to
//! `mktime(3)`.

/// Which timestamps an invocation changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Which {
    pub access: bool,
    pub modification: bool,
}

impl Which {
    pub const BOTH: Which = Which {
        access: true,
        modification: true,
    };

    /// -a: only the access time.
    pub const ACCESS: Which = Which {
        access: true,
        modification: false,
    };

    /// -m: only the modification time.
    pub const MODIFICATION: Which = Which {
        access: false,
        modification: true,
    };

    /// Fold in --time=WORD. The manual lists the words as access, atime, and
    /// use (both), modify and mtime.
    pub fn with_word(&self, word: &str) -> Result<Which, String> {
        match word {
            "access" | "atime" => Ok(Which::ACCESS),
            "modify" | "mtime" => Ok(Which::MODIFICATION),
            "use" => Ok(Which::BOTH),
            other => Err(other.to_string()),
        }
    }
}

/// The options `touch` accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub which: Which,
    /// -c/--no-create: never create a missing file.
    pub no_create: bool,
    /// -h/--no-dereference: act on the link itself.
    pub no_dereference: bool,
    /// -f is accepted and ignored, as the manual says.
    pub force: bool,
    /// -r/--reference=FILE
    pub reference: Option<String>,
    /// -d/--date=STRING
    pub date: Option<String>,
    /// -t STAMP
    pub stamp: Option<String>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            which: Which::BOTH,
            no_create: false,
            no_dereference: false,
            force: false,
            reference: None,
            date: None,
            stamp: None,
        }
    }
}

/// A broken-down local time, ready for `mktime(3)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DateTime {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hour: i64,
    pub minute: i64,
    pub second: i64,
    /// Nanoseconds within the second; the manual's formats do not carry any,
    /// but `-d @1234567890.5` does.
    pub nanosecond: i64,
}

/// How a date was given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Date {
    /// The current time, as of now.
    Now,
    /// An explicit date.
    At(DateTime),
}

/// The date an invocation uses, with the manual's precedence: --reference wins
/// over --date, which wins over -t, and the current time is the fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Now,
    Reference(String),
    Parsed(DateTime),
}

/// Work out which date to use.
pub fn date_source(options: &Options, now: DateTime) -> Result<Source, String> {
    if let Some(file) = &options.reference {
        return Ok(Source::Reference(file.clone()));
    }
    if let Some(text) = &options.date {
        return parse_date(text, now).map(Source::Parsed);
    }
    if let Some(text) = &options.stamp {
        return parse_stamp(text).map(Source::Parsed);
    }
    Ok(Source::Now)
}

/// Whether the file should be created when it is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Create {
    /// Create it, empty.
    Yes,
    /// -c: leave it alone.
    No,
}

/// What should happen to one operand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Set the times.
    Set,
    /// -c and the file is not there.
    Skip,
}

/// Decide for one operand.
pub fn decide(exists: bool, options: &Options) -> Action {
    if !exists && options.no_create {
        return Action::Skip;
    }
    Action::Set
}

/// The mode a file created by touch gets: the manual's 0666, with the umask
/// applied the way open(2) would.
pub fn created_mode(umask: u32) -> u32 {
    0o666 & !umask
}

/// Parse `-t [[CC]YY]MMDDhhmm[.ss]`.
///
/// The manual is precise about the shape: four digits, or six when a century is
/// given, then month, day, hour and minute, with optional seconds after a dot.
pub fn parse_stamp(text: &str) -> Result<DateTime, String> {
    let bad = || format!("invalid date format '{}'", text);
    let (body, seconds) = match text.split_once('.') {
        Some((body, seconds)) => {
            if seconds.is_empty() || !seconds.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad());
            }
            (body, seconds.parse::<i64>().map_err(|_| bad())?)
        }
        None => (text, 0),
    };
    if !body.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    let digits: Vec<char> = body.chars().collect();
    // [[CC]YY]MMDDhhmm: ten digits without a century, twelve with one.
    let (year, rest) = match digits.len() {
        10 => (
            digits[0..2]
                .iter()
                .collect::<String>()
                .parse::<i64>()
                .map_err(|_| bad())?,
            &digits[2..],
        ),
        12 => (
            digits[0..4]
                .iter()
                .collect::<String>()
                .parse::<i64>()
                .map_err(|_| bad())?,
            &digits[4..],
        ),
        _ => return Err(bad()),
    };
    let number = |slice: &[char]| -> Result<i64, String> {
        slice
            .iter()
            .collect::<String>()
            .parse::<i64>()
            .map_err(|_| bad())
    };
    // A two-digit year reads as 1969 to 2068, which is the window coreutils
    // uses: 69 is 1969 and 68 is 2068.
    let year = if year < 69 { year + 2000 } else if year < 100 { year + 1900 } else { year };
    if rest.len() != 8 {
        return Err(bad());
    }
    let month = number(&rest[0..2])?;
    let day = number(&rest[2..4])?;
    let hour = number(&rest[4..6])?;
    let minute = number(&rest[6..8])?;
    Ok(DateTime {
        year,
        month,
        day,
        hour,
        minute,
        second: seconds,
        nanosecond: 0,
    })
}

/// The words `--date` understands without a full date parser. The manual calls
/// the format "mostly free" and points at the info documentation; these are the
/// spellings the documentation lists by name.
pub fn parse_date(text: &str, now: DateTime) -> Result<DateTime, String> {
    let trimmed = text.trim();
    let bad = || format!("invalid date '{}'", text);
    let trimmed = match trimmed.split_once(", ") {
        // A leading weekday name is decoration, as in the manual's own example.
        Some((weekday, rest)) if is_weekday(weekday) => rest.trim(),
        _ => trimmed,
    };
    match trimmed {
        "" => {
            // An empty string is the beginning of the day.
            return Ok(DateTime {
                hour: 0,
                minute: 0,
                second: 0,
                nanosecond: 0,
                ..now
            });
        }
        "now" | "today" => return Ok(now),
        "yesterday" => return Ok(shift_days(now, -1)),
        "tomorrow" => return Ok(shift_days(now, 1)),
        _ => {}
    }
    // @seconds[.fraction] is POSIX, and GNU accepts it.
    if let Some(rest) = trimmed.strip_prefix('@') {
        let mut nanosecond = 0i64;
        let seconds = match rest.split_once('.') {
            Some((seconds, fraction)) => {
                if fraction.is_empty() || !fraction.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(bad());
                }
                nanosecond = format!("{:0<9}", fraction)[..9].parse::<i64>().map_err(|_| bad())?;
                seconds.parse::<i64>().map_err(|_| bad())?
            }
            None => rest.parse::<i64>().map_err(|_| bad())?,
        };
        return from_epoch(seconds, nanosecond);
    }

    // "2004-02-29T16:21:42": an ISO date joined to a time by a T.
    if let Some((date, time)) = trimmed.split_once('T') {
        if let Some((year, month, day)) = split_ymd(date) {
            let (hour, minute, second, nanosecond) = parse_time(time).ok_or_else(bad)?;
            return Ok(DateTime {
                year,
                month,
                day,
                hour,
                minute,
                second,
                nanosecond,
            });
        }
    }

    // The whitespace-separated shapes: a date of one token ("2004-02-29") or
    // of three ("29 Feb 2004"), followed by an optional time. The date comes
    // first, so try the wider date before the narrower one.
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    for width in [3usize, 1] {
        if tokens.len() < width {
            continue;
        }
        let date_text = tokens[..width].join(" ");
        let calendar = match split_ymd(&date_text) {
            Some(calendar) => Some(calendar),
            None => split_day_month_year(&date_text),
        };
        let (year, month, day) = match calendar {
            Some(calendar) => calendar,
            None => continue,
        };
        let time_token = tokens.get(width).copied().filter(|token| token.contains(':'));
        let (hour, minute, second, nanosecond) = match time_token {
            Some(time) => parse_time(time).ok_or_else(bad)?,
            None => (0, 0, 0, 0),
        };
        return Ok(DateTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
            nanosecond,
        });
    }

    // A time on its own is a time of today.
    if trimmed.contains(':') {
        let (hour, minute, second, nanosecond) = parse_time(trimmed).ok_or_else(bad)?;
        return Ok(DateTime {
            hour,
            minute,
            second,
            nanosecond,
            ..now
        });
    }
    Err(bad())
}

/// The three-letter weekday names the manual's example uses.
fn is_weekday(text: &str) -> bool {
    const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let candidate = text.trim().trim_end_matches(',');
    DAYS.iter().any(|day| day.eq_ignore_ascii_case(candidate))
}

/// `29 Feb 2004`: a day, a month name and a year.
fn split_day_month_year(text: &str) -> Option<(i64, i64, i64)> {
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() != 3 {
        return None;
    }
    let day = parts[0].parse::<i64>().ok()?;
    let month = month_number(parts[1])?;
    let year = parts[2].parse::<i64>().ok()?;
    if !(1..=31).contains(&day) {
        return None;
    }
    Some((year, month, day))
}

fn month_number(name: &str) -> Option<i64> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let candidate = &name[..name.len().min(3)];
    MONTHS
        .iter()
        .position(|month| month.eq_ignore_ascii_case(candidate))
        .map(|index| index as i64 + 1)
}

fn split_ymd(text: &str) -> Option<(i64, i64, i64)> {
    let parts: Vec<&str> = text.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year = parts[0].parse::<i64>().ok()?;
    let month = parts[1].parse::<i64>().ok()?;
    let day = parts[2].parse::<i64>().ok()?;
    if parts.iter().any(|part| part.len() != 2) && parts[0].len() != 4 {
        return None;
    }
    // Out-of-range fields are not a date.
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some((year, month, day))
}

fn parse_time(text: &str) -> Option<(i64, i64, i64, i64)> {
    // A trailing timezone offset is accepted and dropped, since the times this
    // program writes are local times.
    let text = match text.find(['+', '-']) {
        Some(index) if index > 0 => &text[..index],
        _ => text,
    };
    let parts: Vec<&str> = text.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let hour = parts[0].parse::<i64>().ok()?;
    let minute = parts[1].parse::<i64>().ok()?;
    if !(0..=23).contains(&hour) || !(0..=59).contains(&minute) {
        return None;
    }
    let (second, nanosecond) = match parts.get(2) {
        None => (0, 0),
        Some(text) => match text.split_once('.') {
            Some((whole, fraction)) => {
                let nanosecond =
                    format!("{:0<9}", fraction)[..9].parse::<i64>().ok()?;
                (whole.parse::<i64>().ok()?, nanosecond)
            }
            None => (text.parse::<i64>().ok()?, 0),
        },
    };
    Some((hour, minute, second, nanosecond))
}

/// Turn seconds since the epoch into a broken-down local time.
pub fn from_epoch(seconds: i64, nanosecond: i64) -> Result<DateTime, String> {
    let time = seconds as libc::time_t;
    let mut parts: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: localtime_r writes into the struct we own and returns a pointer
    // to it, or null when the conversion fails.
    let result = unsafe { libc::localtime_r(&time, &mut parts) };
    if result.is_null() {
        return Err(format!("invalid date '@{}'", seconds));
    }
    Ok(DateTime {
        year: parts.tm_year as i64 + 1900,
        month: parts.tm_mon as i64 + 1,
        day: parts.tm_mday as i64,
        hour: parts.tm_hour as i64,
        minute: parts.tm_min as i64,
        second: parts.tm_sec as i64,
        nanosecond,
    })
}

/// Turn a broken-down local time into seconds since the epoch.
pub fn to_epoch(date: DateTime) -> i64 {
    let mut parts: libc::tm = unsafe { std::mem::zeroed() };
    parts.tm_year = (date.year - 1900) as i32;
    parts.tm_mon = (date.month - 1) as i32;
    parts.tm_mday = date.day as i32;
    parts.tm_hour = date.hour as i32;
    parts.tm_min = date.minute as i32;
    parts.tm_sec = date.second as i32;
    parts.tm_isdst = -1;
    // SAFETY: mktime only reads the struct we filled in.
    unsafe { libc::mktime(&mut parts) as i64 }
}

/// Move a date by whole days, which is what "yesterday" and "tomorrow" mean.
pub fn shift_days(date: DateTime, days: i64) -> DateTime {
    let seconds = to_epoch(date) + days * 86_400;
    from_epoch(seconds, date.nanosecond).unwrap_or(date)
}

/// The time right now, in the same broken-down shape.
pub fn now() -> DateTime {
    // SAFETY: time() takes a pointer it may write through, and null is allowed.
    let seconds = unsafe { libc::time(std::ptr::null_mut()) };
    from_epoch(seconds as i64, 0).unwrap_or_default()
}

/// GNU diagnostics. The quotes coreutils puts around an argument are the
/// typographic ones.
pub const LEFT_QUOTE: &str = "\u{2018}";
pub const RIGHT_QUOTE: &str = "\u{2019}";

pub fn quoted(text: &str) -> String {
    format!("{}{}{}", LEFT_QUOTE, text, RIGHT_QUOTE)
}

pub fn missing_operand_message() -> String {
    "touch: missing file operand".to_string()
}

pub fn invalid_date_message(text: &str) -> String {
    format!("touch: invalid date format {}", quoted(text))
}

pub fn invalid_stamp_message(text: &str) -> String {
    format!("touch: invalid date format {}", quoted(text))
}

pub fn cannot_touch_message(path: &str, reason: &str) -> String {
    format!("touch: cannot touch '{}': {}", path, reason)
}

pub fn reference_error_message(path: &str) -> String {
    format!(
        "touch: failed to get attributes of '{}': No such file or directory",
        path
    )
}

/// The invalid --time word diagnostic, with the list coreutils prints.
pub fn invalid_time_word_lines(word: &str) -> Vec<String> {
    vec![
        format!("touch: invalid argument {} for {}", quoted(word), quoted("--time")),
        "Valid arguments are:".to_string(),
        format!(
            "  - {}, {}, {}",
            quoted("atime"),
            quoted("access"),
            quoted("use")
        ),
        format!("  - {}, {}", quoted("mtime"), quoted("modify")),
        try_help_message(),
    ]
}

pub fn try_help_message() -> String {
    "Try 'touch --help' for more information.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DateTime {
        DateTime {
            year: 2024,
            month: 3,
            day: 15,
            hour: 12,
            minute: 30,
            second: 45,
            nanosecond: 0,
        }
    }

    #[test]
    fn both_times_by_default() {
        assert_eq!(Options::default().which, Which::BOTH);
    }

    #[test]
    fn access_and_modification_flags() {
        assert_eq!(Which::ACCESS.access, true);
        assert_eq!(Which::ACCESS.modification, false);
        assert_eq!(Which::MODIFICATION.modification, true);
        assert_eq!(Which::MODIFICATION.access, false);
    }

    #[test]
    fn time_words_follow_the_manual() {
        assert_eq!(Which::BOTH.with_word("access").unwrap(), Which::ACCESS);
        assert_eq!(Which::BOTH.with_word("atime").unwrap(), Which::ACCESS);
        assert_eq!(Which::BOTH.with_word("modify").unwrap(), Which::MODIFICATION);
        assert_eq!(Which::BOTH.with_word("mtime").unwrap(), Which::MODIFICATION);
        assert_eq!(Which::ACCESS.with_word("use").unwrap(), Which::BOTH);
        // The word "use:" with its colon is not one of them.
        assert!(Which::BOTH.with_word("use:").is_err());
        assert!(Which::BOTH.with_word("birth").is_err());
    }

    #[test]
    fn stamp_with_two_digit_year() {
        // POSIX reads 11 as 2011 and 96 as 1996.
        assert_eq!(parse_stamp("1102030405").unwrap().year, 2011);
        assert_eq!(parse_stamp("9602030405").unwrap().year, 1996);
        assert_eq!(parse_stamp("6902030405").unwrap().year, 1969);
        assert_eq!(parse_stamp("6802030405").unwrap().year, 2068);
        assert_eq!(parse_stamp("2403151230").unwrap().year, 2024);
    }

    #[test]
    fn stamp_with_four_digit_year_and_seconds() {
        assert_eq!(
            parse_stamp("202403151230.45").unwrap(),
            DateTime {
                year: 2024,
                month: 3,
                day: 15,
                hour: 12,
                minute: 30,
                second: 45,
                nanosecond: 0
            }
        );
    }

    #[test]
    fn stamp_rejects_other_shapes() {
        assert!(parse_stamp("2024-03-15").is_err());
        assert!(parse_stamp("2403").is_err());
        assert!(parse_stamp("2403151230.").is_err());
        assert!(parse_stamp("2403151230.xx").is_err());
    }

    #[test]
    fn date_words_the_manual_names() {
        let now = sample();
        assert_eq!(parse_date("now", now).unwrap(), now);
        assert_eq!(parse_date("today", now).unwrap(), now);
        assert_eq!(parse_date("2024-03-14", now).unwrap().day, 14);
        assert_eq!(parse_date("2024-03-14", now).unwrap().month, 3);
        assert_eq!(parse_date("2024-03-14", now).unwrap().year, 2024);
    }

    #[test]
    fn empty_date_is_the_start_of_the_day() {
        let now = sample();
        let parsed = parse_date("", now).unwrap();
        assert_eq!((parsed.hour, parsed.minute, parsed.second), (0, 0, 0));
        assert_eq!(parsed.day, now.day);
    }

    #[test]
    fn date_with_a_time_and_a_separator() {
        assert_eq!(
            parse_date("2004-02-29 16:21:42", sample()).unwrap(),
            DateTime {
                year: 2004,
                month: 2,
                day: 29,
                hour: 16,
                minute: 21,
                second: 42,
                nanosecond: 0
            }
        );
        assert_eq!(
            parse_date("2004-02-29T16:21:42", sample()).unwrap().hour,
            16
        );
    }

    #[test]
    fn fractional_seconds_are_read() {
        let parsed = parse_date("2004-02-29 16:21:42.5", sample()).unwrap();
        assert_eq!(parsed.nanosecond, 500_000_000);
    }

    #[test]
    fn time_only_is_today() {
        let parsed = parse_date("07:08", sample()).unwrap();
        assert_eq!((parsed.hour, parsed.minute), (7, 8));
        assert_eq!(parsed.day, sample().day);
    }

    #[test]
    fn epoch_seconds_are_accepted() {
        let parsed = parse_date("@0", sample()).unwrap();
        assert_eq!(to_epoch(parsed), 0);
    }

    #[test]
    fn relative_day_words_move_by_whole_days() {
        let now = sample();
        let yesterday = parse_date("yesterday", now).unwrap();
        assert_eq!(to_epoch(now) - to_epoch(yesterday), 86_400);
        let tomorrow = parse_date("tomorrow", now).unwrap();
        assert_eq!(to_epoch(tomorrow) - to_epoch(now), 86_400);
    }

    #[test]
    fn a_leading_weekday_is_decoration() {
        let parsed = parse_date("Sun, 29 Feb 2004 16:21:42 -0800", sample()).unwrap();
        assert_eq!((parsed.year, parsed.month, parsed.day), (2004, 2, 29));
        assert_eq!(parsed.hour, 16);
    }

    #[test]
    fn nonsense_dates_are_refused() {
        assert!(parse_date("not a date", sample()).is_err());
        assert!(parse_date("2024-13-45", sample()).is_err());
    }

    #[test]
    fn epoch_round_trip() {
        let date = DateTime {
            year: 2001,
            month: 2,
            day: 3,
            hour: 4,
            minute: 5,
            second: 6,
            nanosecond: 0,
        };
        let seconds = to_epoch(date);
        let back = from_epoch(seconds, 0).unwrap();
        assert_eq!((back.year, back.month, back.day), (2001, 2, 3));
        assert_eq!((back.hour, back.minute, back.second), (4, 5, 6));
    }

    #[test]
    fn date_sources_follow_the_precedence() {
        let now = sample();
        assert_eq!(date_source(&Options::default(), now).unwrap(), Source::Now);

        let options = Options {
            date: Some("2024-01-02".into()),
            ..Options::default()
        };
        assert_eq!(
            date_source(&options, now).unwrap(),
            Source::Parsed(DateTime {
                year: 2024,
                month: 1,
                day: 2,
                hour: 0,
                minute: 0,
                second: 0,
                nanosecond: 0
            })
        );

        // --reference beats --date.
        let options = Options {
            reference: Some("other".into()),
            date: Some("2024-01-02".into()),
            ..Options::default()
        };
        assert_eq!(
            date_source(&options, now).unwrap(),
            Source::Reference("other".into())
        );
    }

    #[test]
    fn no_create_skips_a_missing_file() {
        let options = Options {
            no_create: true,
            ..Options::default()
        };
        assert_eq!(decide(false, &options), Action::Skip);
        assert_eq!(decide(true, &options), Action::Set);
        assert_eq!(decide(false, &Options::default()), Action::Set);
    }

    #[test]
    fn created_files_get_0666_minus_the_umask() {
        assert_eq!(created_mode(0o022), 0o644);
        assert_eq!(created_mode(0o002), 0o664);
    }

    #[test]
    fn messages_match_coreutils() {
        assert_eq!(missing_operand_message(), "touch: missing file operand");
        assert_eq!(
            invalid_stamp_message("2024-01-01"),
            "touch: invalid date format \u{2018}2024-01-01\u{2019}"
        );
        assert_eq!(
            cannot_touch_message("x", "No such file or directory"),
            "touch: cannot touch 'x': No such file or directory"
        );
        assert_eq!(
            reference_error_message("nope"),
            "touch: failed to get attributes of 'nope': No such file or directory"
        );
        assert_eq!(invalid_time_word_lines("birth").len(), 5);
    }
}