//! Name logic of `mktemp(1)`, following its manual page:
//!
//! ```text
//! mktemp [OPTION]... [TEMPLATE]
//! ```
//!
//! The manual is specific about the rules: the template needs at least three
//! consecutive `X`s in its last component, `--suffix` is implied when the
//! template does not end in `X`, `-t` and `--tmpdir` differ in how much of the
//! template they interpret, and files are created `u+rw`, directories `u+rwx`,
//! minus the umask.

use std::path::{Component, Path, PathBuf};

/// The options `mktemp` accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// -d/--directory
    pub directory: bool,
    /// -u/--dry-run: print a name and create nothing.
    pub dry_run: bool,
    /// -q/--quiet
    pub quiet: bool,
    /// --suffix=SUFF
    pub suffix: Option<String>,
    /// -p DIR / --tmpdir[=DIR]
    pub tmpdir: Option<Option<String>>,
    /// -t: the template is one component, relative to a directory.
    pub single_component: bool,
    pub positional: Option<String>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            directory: false,
            dry_run: false,
            quiet: false,
            suffix: None,
            tmpdir: None,
            single_component: false,
            positional: None,
        }
    }
}

/// What the operands mean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Template {
    /// No template: tmp.XXXXXXXXXX under the temporary directory.
    Implicit,
    /// An explicit template, already split into the part before the Xs, the
    /// number of Xs and the suffix that follows them.
    Explicit(Split),
}

/// A template taken apart: `--suffix` is appended to the template, and it is
/// implied when the template does not end in `X`, so the run of Xs sits between
/// a prefix and a suffix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Split {
    pub prefix: String,
    pub xs: usize,
    pub suffix: String,
}

/// Why a template cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// The template does not end in at least three X's.
    TooFewXs(String),
    /// -p was given and the template is an absolute name.
    AbsoluteWithTmpdir(String),
    /// -t was given and the template has a slash in it.
    SlashesWithDashT(String),
    /// --suffix contains a slash.
    SuffixHasSlash(String),
}

/// Work out the template the manual describes for these options.
pub fn template(options: &Options) -> Result<Template, Problem> {
    if let Some(suffix) = &options.suffix {
        if suffix.contains('/') {
            return Err(Problem::SuffixHasSlash(suffix.clone()));
        }
    }

    let raw = match &options.positional {
        None => {
            // No template at all: the documented default, and --tmpdir is
            // implied.
            return Ok(Template::Implicit);
        }
        Some(text) => text.clone(),
    };

    // --suffix is always appended; when it is not given it is implied by a
    // template that does not end in X, which leaves the run of Xs in front of
    // the implied suffix.
    let (stem, suffix) = match &options.suffix {
        Some(suffix) => (raw.clone(), suffix.clone()),
        None if raw.ends_with('X') => (raw.clone(), String::new()),
        None => match raw.rfind('X') {
            // The last run of Xs is the template; whatever follows it is the
            // implied suffix.
            Some(last) => (raw[..=last].to_string(), raw[last + 1..].to_string()),
            None => (raw.clone(), String::new()),
        },
    };
    let xs = count_trailing_xs(&stem);
    let split = Split {
        prefix: stem[..stem.len() - xs].to_string(),
        xs,
        suffix,
    };

    // The run of Xs has to be at least three long, and it sits in front of the
    // suffix, which --suffix may have appended or which a template that does
    // not end in X implies.
    if xs < 3 {
        return Err(Problem::TooFewXs(raw));
    }

    if options.single_component {
        // -t: the template is one component, so a slash in it is an error.
        if raw.contains('/') {
            return Err(Problem::SlashesWithDashT(raw));
        }
    } else if options.tmpdir.is_some() {
        // With -p the template must not be absolute, though it may contain
        // slashes.
        if Path::new(&raw).is_absolute() {
            return Err(Problem::AbsoluteWithTmpdir(raw));
        }
    }
    Ok(Template::Explicit(split))
}

/// True when the last character is an `X`.
pub fn ends_with_x(text: &str) -> bool {
    text.ends_with('X')
}

/// The number of consecutive `X`s at the end of `text`.
pub fn count_trailing_xs(text: &str) -> usize {
    text.chars().rev().take_while(|c| *c == 'X').count()
}

/// The last component of a path, as text.
pub fn last_component(path: &str) -> &str {
    match path.rfind('/') {
        Some(index) => &path[index + 1..],
        None => path,
    }
}

/// The directory a name is created in, following the manual's precedence:
/// -p wins, then $TMPDIR, then /tmp. With -t the -p directory still applies.
pub fn directory_for(options: &Options, environment: Option<&str>) -> PathBuf {
    match &options.tmpdir {
        Some(Some(directory)) => PathBuf::from(directory),
        // A bare --tmpdir means "use $TMPDIR", and so does -t.
        Some(None) => PathBuf::from(environment.unwrap_or("/tmp")),
        None => PathBuf::from(environment.unwrap_or("/tmp")),
    }
}

/// The default template the manual gives for the no-operand case.
pub const DEFAULT_TEMPLATE: &str = "tmp.XXXXXXXXXX";

/// Replace the Xs of a template with the given random characters.
pub fn fill(template: &str, random: &str) -> String {
    let xs = count_trailing_xs(template);
    let head = &template[..template.len() - xs];
    let take = xs.min(random.len());
    format!("{}{}", head, &random[..take])
}

/// Put the random characters where the Xs were.
pub fn fill_split(split: &Split, random: &str) -> String {
    let take = split.xs.min(random.len());
    format!(
        "{}{}{}",
        split.prefix,
        &random[..take],
        split.suffix
    )
}

/// How many characters a name needs, which is the width of the X run.
pub fn width(split: &Split) -> usize {
    split.xs
}

/// The full name to create: the template with its directory in front.
pub fn name_in(directory: &Path, template: &str, random: &str) -> PathBuf {
    // With -p or -t only the final component is created, so the template's own
    // directory part is dropped; without either, it is kept.
    directory.join(fill(last_component(template), random))
}

/// The mode of a created file: the manual says "u+rw", minus the umask.
pub fn file_mode(umask: u32) -> u32 {
    0o600 & !umask
}

/// The mode of a created directory: the manual says "u+rwx", minus the umask.
pub fn directory_mode(umask: u32) -> u32 {
    0o700 & !umask
}

/// The characters mktemp draws from.
pub const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// GNU diagnostics. coreutils quotes an operand with the typographic quotes,
/// U+2018 and U+2019.
pub const LEFT_QUOTE: &str = "\u{2018}";
pub const RIGHT_QUOTE: &str = "\u{2019}";

pub fn quoted(text: &str) -> String {
    format!("{}{}{}", LEFT_QUOTE, text, RIGHT_QUOTE)
}

pub fn too_few_xs_message(template: &str) -> String {
    format!("mktemp: too few X's in template {}", quoted(template))
}

pub fn absolute_with_tmpdir_message(template: &str) -> String {
    format!(
        "mktemp: invalid template, {}; with --tmpdir, it may not be absolute",
        quoted(template)
    )
}

pub fn slashes_with_t_message(template: &str) -> String {
    format!(
        "mktemp: invalid template, {}, contains directory separator",
        quoted(template)
    )
}

pub fn suffix_slash_message(suffix: &str) -> String {
    format!(
        "mktemp: invalid suffix {}, contains directory separator",
        quoted(suffix)
    )
}

pub fn too_many_templates_message() -> String {
    "mktemp: too many templates".to_string()
}

pub fn unrecognized_option_message(option: &str) -> String {
    format!("mktemp: unrecognized option '{}'", option)
}

pub fn invalid_option_message(letter: char) -> String {
    format!("mktemp: invalid option -- '{}'", letter)
}

pub fn requires_argument_message(letter: char) -> String {
    format!("mktemp: option requires an argument -- '{}'", letter)
}

pub fn try_help_message() -> String {
    "Try 'mktemp --help' for more information.".to_string()
}

pub fn cannot_create_message(name: &str, reason: &str) -> String {
    format!(
        "mktemp: failed to create file via template {}: {}",
        quoted(name),
        reason
    )
}

/// Remove trailing slashes, which the manual does not allow in a template.
pub fn strip_trailing_slashes(text: &str) -> String {
    let trimmed = text.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Whether a path has a directory part at all.
pub fn has_directory_part(path: &str) -> bool {
    Path::new(path)
        .components()
        .any(|component| component != Component::CurDir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_operand_uses_the_documented_default() {
        let options = Options::default();
        assert_eq!(template(&options).unwrap(), Template::Implicit);
        assert_eq!(DEFAULT_TEMPLATE, "tmp.XXXXXXXXXX");
    }

    #[test]
    fn a_plain_template_is_accepted() {
        let options = Options {
            positional: Some("tmp.XXXXXX".into()),
            ..Options::default()
        };
        assert_eq!(
            template(&options).unwrap(),
            Template::Explicit(Split {
                prefix: "tmp.".into(),
                xs: 6,
                suffix: String::new(),
            })
        );
    }

    #[test]
    fn three_xs_are_enough() {
        let options = Options {
            positional: Some("aXXX".into()),
            ..Options::default()
        };
        assert!(template(&options).is_ok());
    }

    #[test]
    fn fewer_than_three_xs_is_an_error() {
        let options = Options {
            positional: Some("aXX".into()),
            ..Options::default()
        };
        assert_eq!(
            template(&options).unwrap_err(),
            Problem::TooFewXs("aXX".into())
        );
    }

    #[test]
    fn a_template_without_xs_is_an_error() {
        let options = Options {
            positional: Some("file.txt".into()),
            ..Options::default()
        };
        assert_eq!(
            template(&options).unwrap_err(),
            Problem::TooFewXs("file.txt".into())
        );
    }

    #[test]
    fn suffix_is_implied_when_the_template_does_not_end_in_x() {
        // "tmp.XXXXXX.txt" without --suffix: the suffix is implied, so the Xs
        // are the run in front of it.
        let options = Options {
            positional: Some("tmp.XXXXXX.txt".into()),
            ..Options::default()
        };
        assert_eq!(
            template(&options).unwrap(),
            Template::Explicit(Split {
                prefix: "tmp.".into(),
                xs: 6,
                suffix: ".txt".into(),
            })
        );
    }

    #[test]
    fn an_explicit_suffix_is_appended() {
        let options = Options {
            positional: Some("tmp.XXXXXX".into()),
            suffix: Some(".txt".into()),
            ..Options::default()
        };
        assert_eq!(
            template(&options).unwrap(),
            Template::Explicit(Split {
                prefix: "tmp.".into(),
                xs: 6,
                suffix: ".txt".into(),
            })
        );
    }

    #[test]
    fn a_suffix_with_a_slash_is_refused() {
        let options = Options {
            positional: Some("tmp.XXXXXX".into()),
            suffix: Some("a/b".into()),
            ..Options::default()
        };
        assert_eq!(template(&options).unwrap_err(), Problem::SuffixHasSlash("a/b".into()));
    }

    #[test]
    fn dash_t_forbids_slashes() {
        // The X count is checked first, so a template whose last component has
        // the Xs is refused for its slash.
        let options = Options {
            positional: Some("dir/tmp.XXXXXX".into()),
            single_component: true,
            ..Options::default()
        };
        assert_eq!(
            template(&options).unwrap_err(),
            Problem::SlashesWithDashT("dir/tmp.XXXXXX".into())
        );

        // With no Xs in the last component, that is the complaint instead.
        let options = Options {
            positional: Some("a/b".into()),
            single_component: true,
            ..Options::default()
        };
        assert_eq!(template(&options).unwrap_err(), Problem::TooFewXs("a/b".into()));
    }

    #[test]
    fn tmpdir_forbids_absolute_templates() {
        let options = Options {
            positional: Some("/tmp/tmp.XXXXXX".into()),
            tmpdir: Some(None),
            ..Options::default()
        };
        assert_eq!(
            template(&options).unwrap_err(),
            Problem::AbsoluteWithTmpdir("/tmp/tmp.XXXXXX".into())
        );
    }

    #[test]
    fn tmpdir_allows_slashes_inside_the_template() {
        let options = Options {
            positional: Some("dir/tmp.XXXXXX".into()),
            tmpdir: Some(Some("/var/tmp".into())),
            ..Options::default()
        };
        assert_eq!(
            template(&options).unwrap(),
            Template::Explicit(Split {
                prefix: "dir/tmp.".into(),
                xs: 6,
                suffix: String::new(),
            })
        );
    }

    #[test]
    fn dash_t_and_tmpdir_together_make_the_slash_the_problem() {
        // coreutils reports the slash, not a conflict between the two options.
        let options = Options {
            positional: Some("./tp.XXXXXX".into()),
            single_component: true,
            tmpdir: Some(Some("/var/tmp".into())),
            ..Options::default()
        };
        assert_eq!(
            template(&options).unwrap_err(),
            Problem::SlashesWithDashT("./tp.XXXXXX".into())
        );
    }

    #[test]
    fn the_directory_follows_the_documented_order() {
        let options = Options {
            tmpdir: Some(Some("/p".into())),
            ..Options::default()
        };
        assert_eq!(directory_for(&options, Some("/env")), PathBuf::from("/p"));

        let bare = Options {
            tmpdir: Some(None),
            ..Options::default()
        };
        assert_eq!(directory_for(&bare, Some("/env")), PathBuf::from("/env"));
        assert_eq!(directory_for(&bare, None), PathBuf::from("/tmp"));
        assert_eq!(
            directory_for(&Options::default(), None),
            PathBuf::from("/tmp")
        );
    }

    #[test]
    fn trailing_xs_are_counted() {
        assert_eq!(count_trailing_xs("tmp.XXXXXX"), 6);
        assert_eq!(count_trailing_xs("XXXX"), 4);
        assert_eq!(count_trailing_xs("XXX.txt"), 0);
        assert_eq!(count_trailing_xs(""), 0);
    }

    #[test]
    fn the_last_component_is_used_with_tmpdir() {
        assert_eq!(last_component("a/b/c"), "c");
        assert_eq!(last_component("c"), "c");
        assert_eq!(last_component("/c"), "c");
    }

    #[test]
    fn filling_replaces_only_the_xs() {
        assert_eq!(fill("tmp.XXXXXX", "abcdef"), "tmp.abcdef");
        assert_eq!(fill("XXXX", "abcdef"), "abcd");
        // With an implied suffix the Xs are not at the very end, which is what
        // fill_split is for.
        let stem: String = "aXXXXXX".to_string();
        assert_eq!(
            fill_split(
                &Split {
                    prefix: "a".into(),
                    xs: 6,
                    suffix: "b".into()
                },
                "abcdef"
            ),
            "aabcdefb"
        );
        assert_eq!(stem.len(), 7);
    }

    #[test]
    fn names_are_built_inside_the_directory() {
        assert_eq!(
            name_in(Path::new("/tmp"), "tmp.XXXXXX", "abcdef"),
            PathBuf::from("/tmp/tmp.abcdef")
        );
        // With --tmpdir only the final component is created.
        assert_eq!(
            name_in(Path::new("/var/tmp"), "dir/tmp.XXXXXX", "abcdef"),
            PathBuf::from("/var/tmp/tmp.abcdef")
        );
    }

    #[test]
    fn a_split_name_keeps_prefix_and_suffix() {
        let split = Split {
            prefix: "tmp.".into(),
            xs: 6,
            suffix: ".txt".into(),
        };
        assert_eq!(fill_split(&split, "abcdef"), "tmp.abcdef.txt");
        assert_eq!(width(&split), 6);
    }

    #[test]
    fn modes_match_the_manual() {
        // "Files are created u+rw, and directories u+rwx, minus umask".
        assert_eq!(file_mode(0o022), 0o600);
        assert_eq!(file_mode(0o077), 0o600);
        assert_eq!(directory_mode(0o022), 0o700);
    }

    #[test]
    fn messages_match_coreutils() {
        assert_eq!(
            too_few_xs_message("aXX"),
            "mktemp: too few X's in template \u{2018}aXX\u{2019}"
        );
        assert_eq!(
            suffix_slash_message("a/b"),
            "mktemp: invalid suffix \u{2018}a/b\u{2019}, contains directory separator"
        );
        assert_eq!(
            absolute_with_tmpdir_message("/a"),
            "mktemp: invalid template, \u{2018}/a\u{2019}; with --tmpdir, it may not be absolute"
        );
        assert_eq!(too_many_templates_message(), "mktemp: too many templates");
        assert_eq!(
            invalid_option_message('Z'),
            "mktemp: invalid option -- 'Z'"
        );
    }
}