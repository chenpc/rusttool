//! Decision logic of `mv(1)`, following its manual page:
//!
//! ```text
//! mv [OPTION]... [-T] SOURCE DEST
//! mv [OPTION]... SOURCE... DIRECTORY
//! mv [OPTION]... -t DIRECTORY SOURCE...
//! ```
//!
//! `mv` shares its operand rules with `cp`, and differs in what it then does:
//! a rename where the filesystem allows it, and a copy plus an unlink where it
//! does not. `--no-copy` turns that fallback off.

use std::path::{Component, Path, PathBuf};

/// `--update[=UPDATE]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Update {
    All,
    None,
    Older,
}

impl Update {
    pub fn parse(text: &str) -> Result<Update, String> {
        match text {
            "all" => Ok(Update::All),
            "none" => Ok(Update::None),
            "older" => Ok(Update::Older),
            other => Err(other.to_string()),
        }
    }
}

/// Which of -i, -f and -n was given last: the manual is explicit that only the
/// final one takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overwrite {
    /// Ask before replacing anything.
    Interactive,
    /// Replace without asking.
    Force,
    /// Leave an existing destination alone.
    NoClobber,
}

impl Overwrite {
    /// The last of -i, -f and -n on the command line wins.
    pub fn resolve<'a>(flags: impl Iterator<Item = Overwrite>) -> Overwrite {
        flags.last().unwrap_or(Overwrite::Force)
    }
}

/// The options `mv` accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub overwrite: Overwrite,
    pub verbose: bool,
    pub update: Update,
    pub target_directory: Option<String>,
    pub no_target_directory: bool,
    pub backup: Option<String>,
    pub suffix: String,
    pub strip_trailing_slashes: bool,
    /// --no-copy: fail instead of falling back to a copy when a rename cannot
    /// be done.
    pub no_copy: bool,
    pub debug: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            overwrite: Overwrite::Force,
            verbose: false,
            update: Update::All,
            target_directory: None,
            no_target_directory: false,
            backup: None,
            suffix: "~".to_string(),
            strip_trailing_slashes: false,
            no_copy: false,
            debug: false,
        }
    }
}

impl Options {
    pub fn effective(&self) -> Options {
        let mut o = self.clone();
        if o.debug {
            o.verbose = true;
        }
        o
    }

    /// -n is --update=none, and it wins over -u; -i leaves every destination in
    /// place until the user says otherwise.
    pub fn effective_update(&self) -> Update {
        match self.overwrite {
            Overwrite::NoClobber => Update::None,
            _ => self.update,
        }
    }
}

/// One source and where it lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pair {
    pub source: PathBuf,
    pub destination: PathBuf,
}

impl Pair {
    /// -v prints the same wording whether the operand was a file or a
    /// directory.
    pub fn verbose_line(&self) -> String {
        format!(
            "renamed '{}' -> '{}'",
            self.source.to_string_lossy(),
            self.destination.to_string_lossy()
        )
    }
}

/// What the operand list means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    Pairs(Vec<Pair>),
    Invalid(Problem),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    MissingOperand,
    ExtraOperand(String),
    TargetUnavailable { destination: String, reason: String },
    TargetDirectoryUnavailable { directory: String, reason: String },
    /// A source that is not there: reported per operand, so it is not fatal for
    /// the others.
    MissingSource(String),
}

/// Work out the pairs for an operand list.
pub fn plan(operands: &[String], last_is_dir: bool, options: &Options) -> Plan {
    if let Some(directory) = options.target_directory.clone() {
        if options.no_target_directory {
            return Plan::Invalid(Problem::TargetDirectoryUnavailable {
                directory,
                reason: "Not a directory".to_string(),
            });
        }
        return Plan::Pairs(
            operands
                .iter()
                .map(|source| {
                    let source = normalize_source(source, options);
                    let target = PathBuf::from(&directory).join(base_name(&source));
                    Pair {
                        source,
                        destination: target,
                    }
                })
                .collect(),
        );
    }

    let (destination, sources) = match operands.split_last() {
        None => return Plan::Invalid(Problem::MissingOperand),
        Some((last, rest)) => {
            if rest.is_empty() {
                return Plan::Invalid(Problem::MissingOperand);
            }
            if options.no_target_directory {
                // -T leaves no room for the operands after the first source, so
                // the last one is the extra operand coreutils names.
                if rest.len() > 1 {
                    return Plan::Invalid(Problem::ExtraOperand(last.clone()));
                }
                (last.clone(), rest.to_vec())
            } else if last_is_dir {
                (last.clone(), rest.to_vec())
            } else if rest.len() > 1 {
                return Plan::Invalid(Problem::TargetUnavailable {
                    destination: last.clone(),
                    reason: String::new(),
                });
            } else {
                (last.clone(), rest.to_vec())
            }
        }
    };

    let into_directory = last_is_dir && !options.no_target_directory;
    let pairs = sources
        .iter()
        .map(|source| {
            let source = normalize_source(source, options);
            let target = if into_directory {
                PathBuf::from(&destination).join(base_name(&source))
            } else {
                PathBuf::from(&destination)
            };
            Pair {
                source,
                destination: target,
            }
        })
        .collect();
    Plan::Pairs(pairs)
}

pub fn normalize_source(source: &str, options: &Options) -> PathBuf {
    if options.strip_trailing_slashes {
        PathBuf::from(source.trim_end_matches('/'))
    } else {
        PathBuf::from(source)
    }
}

/// The name a source is filed under when the destination is a directory.
pub fn base_name(path: &Path) -> PathBuf {
    match path.components().next_back() {
        Some(Component::Normal(name)) => PathBuf::from(name),
        _ => path.to_path_buf(),
    }
}

/// What should happen to one pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Rename it, or copy and unlink when a rename is not possible.
    Move,
    /// Replace an existing destination, after the user agreed.
    OverwriteConfirmed,
    /// -n, and the destination is already there.
    Skip,
    /// --update=older, and the destination is at least as new.
    SkipNotNewer,
    /// -i, and the user declined.
    SkipDeclined,
    /// Renaming failed and --no-copy forbids the fallback.
    RefuseCrossDevice,
    /// A destination directory that is not empty.
    RefuseNonEmptyDirectory,
    /// The destination is a plain file and the source is a directory.
    TargetNotADirectory,
    /// The destination is a plain file and the source is a directory that
    /// still has entries in it, which coreutils words differently.
    OverwriteNonDirectory,
    /// -n or --update=none, and the destination is there.
    NotReplacing,
}

/// Decide what to do with one pair.
pub fn decide(
    source_is_dir: bool,
    dest_exists: bool,
    dest_is_dir: bool,
    dest_is_empty_dir: bool,
    same_device: bool,
    source_mtime: Option<(i64, i64)>,
    dest_mtime: Option<(i64, i64)>,
    options: &Options,
) -> Action {
    if dest_exists {
        match options.effective_update() {
            // -n reports that it left the destination alone, which is a
            // non-zero status for mv.
            Update::None => return Action::NotReplacing,
            Update::Older => {
                if let (Some(src), Some(dst)) = (source_mtime, dest_mtime) {
                    if dst >= src {
                        return Action::SkipNotNewer;
                    }
                }
            }
            Update::All => {}
        }
        // A directory cannot land on a plain file. An empty one could be
        // removed first, so the complaint is about the target; a directory
        // with entries in it cannot be, and is worded as an overwrite.
        if source_is_dir && !dest_is_dir {
            return if dest_is_empty_dir {
                Action::TargetNotADirectory
            } else {
                Action::OverwriteNonDirectory
            };
        }
        if options.overwrite == Overwrite::Interactive {
            return Action::OverwriteConfirmed;
        }
    }
    if source_is_dir && dest_is_dir && !dest_is_empty_dir {
        return Action::RefuseNonEmptyDirectory;
    }
    if !same_device && options.no_copy {
        return Action::RefuseCrossDevice;
    }
    Action::Move
}

/// The backup name for an existing destination, following --suffix and
/// SIMPLE_BACKUP_SUFFIX.
pub fn backup_name(destination: &str, options: &Options, env_suffix: Option<&str>) -> String {
    let suffix = if options.suffix != "~" {
        options.suffix.clone()
    } else {
        env_suffix.unwrap_or("~").to_string()
    };
    format!("{}{}", destination, suffix)
}

pub fn wants_backup(options: &Options, version_control: Option<&str>) -> bool {
    let control = options
        .backup
        .clone()
        .or_else(|| version_control.map(|s| s.to_string()));
    match control {
        None => false,
        Some(control) => !matches!(control.as_str(), "none" | "off"),
    }
}

/// GNU diagnostics.
pub fn missing_operand_message() -> String {
    "mv: missing file operand".to_string()
}

/// One operand and no destination: coreutils names the operand it has.
pub fn missing_destination_message(operand: &str) -> String {
    format!("mv: missing destination file operand after '{}'", operand)
}

/// -n and --update=none say what they did not do.
pub fn not_replacing_message(destination: &str) -> String {
    format!("mv: not replacing '{}'", destination)
}

/// -t pointed at something that is not there.
pub fn target_directory_missing_message(directory: &str) -> String {
    format!(
        "mv: target directory '{}': No such file or directory",
        directory
    )
}

/// A directory cannot land on a plain file.
pub fn target_not_a_directory_message(destination: &str) -> String {
    format!("mv: target '{}': Not a directory", destination)
}

/// A non-empty directory cannot replace a plain file.
pub fn overwrite_non_directory_message(destination: &str, source: &str) -> String {
    format!(
        "mv: cannot overwrite non-directory '{}' with directory '{}'",
        destination, source
    )
}

/// A plain file cannot replace a directory when -T says the destination is a
/// plain name.
pub fn overwrite_directory_message(destination: &str) -> String {
    format!(
        "mv: cannot overwrite directory '{}' with non-directory",
        destination
    )
}

pub fn unrecognized_option_message(option: &str) -> String {
    format!("mv: unrecognized option '{}'", option)
}

pub fn invalid_option_message(letter: char) -> String {
    format!("mv: invalid option -- '{}'", letter)
}

pub fn requires_argument_message(letter: char) -> String {
    format!("mv: option requires an argument -- '{}'", letter)
}

pub fn try_help_message() -> String {
    "Try 'mv --help' for more information.".to_string()
}

pub fn missing_source_message(source: &str) -> String {
    format!("mv: cannot stat '{}': No such file or directory", source)
}

pub fn target_unavailable_message(destination: &str, reason: &str) -> String {
    format!("mv: target '{}': {}", destination, reason)
}

pub fn target_directory_message(directory: &str, reason: &str) -> String {
    format!("mv: target directory '{}': {}", directory, reason)
}

pub fn extra_operand_message(operand: &str) -> String {
    format!("mv: extra operand '{}'", operand)
}

pub fn cannot_move_message(source: &str, destination: &str, reason: &str) -> String {
    format!("mv: cannot move '{}' to '{}': {}", source, destination, reason)
}

pub fn cannot_overwrite_directory_message(destination: &str) -> String {
    format!("mv: cannot move '{}' to a directory", destination)
}

pub fn directory_not_empty_message(destination: &str) -> String {
    format!("mv: cannot move '{}' to a non-empty directory", destination)
}

pub fn same_file_message(source: &str, destination: &str) -> String {
    format!("mv: '{}' and '{}' are the same file", source, destination)
}

pub fn cross_device_message(source: &str, destination: &str) -> String {
    format!(
        "mv: cannot move '{}' to '{}': Invalid cross-device link",
        source, destination
    )
}

pub fn remove_error(path: &str, reason: &str) -> String {
    format!("mv: cannot remove '{}': {}", path, reason)
}

pub fn prompt_message(destination: &str) -> String {
    format!("mv: overwrite '{}'? ", destination)
}

pub fn invalid_update_message(value: &str) -> String {
    format!("mv: invalid argument '{}' for '--update' option", value)
}

pub fn target_reason(exists: bool, is_dir: bool) -> &'static str {
    if exists && !is_dir {
        "Not a directory"
    } else {
        "No such file or directory"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Options {
        Options::default()
    }

    fn plan_of(operands: &[&str], last_is_dir: bool, options: &Options) -> Plan {
        let owned: Vec<String> = operands.iter().map(|s| s.to_string()).collect();
        plan(&owned, last_is_dir, options)
    }

    #[test]
    fn single_source_single_name() {
        match plan_of(&["a", "b"], false, &opts()) {
            Plan::Pairs(pairs) => {
                assert_eq!(pairs[0].source, PathBuf::from("a"));
                assert_eq!(pairs[0].destination, PathBuf::from("b"));
            }
            other => panic!("expected pairs, got {:?}", other),
        }
    }

    #[test]
    fn several_sources_need_a_directory() {
        assert_eq!(
            plan_of(&["a", "b", "c"], false, &opts()),
            Plan::Invalid(Problem::TargetUnavailable {
                destination: "c".to_string(),
                reason: String::new(),
            })
        );
    }

    #[test]
    fn sources_land_in_the_directory() {
        match plan_of(&["a", "b", "target"], true, &opts()) {
            Plan::Pairs(pairs) => {
                assert_eq!(pairs[0].destination, PathBuf::from("target/a"));
                assert_eq!(pairs[1].destination, PathBuf::from("target/b"));
            }
            other => panic!("expected pairs, got {:?}", other),
        }
    }

    #[test]
    fn target_directory_option() {
        let options = Options { target_directory: Some("d".into()), ..opts() };
        match plan_of(&["a", "b"], false, &options) {
            Plan::Pairs(pairs) => assert_eq!(pairs[1].destination, PathBuf::from("d/b")),
            other => panic!("expected pairs, got {:?}", other),
        }
    }

    #[test]
    fn no_target_directory_allows_one_source_only() {
        let options = Options { no_target_directory: true, ..opts() };
        assert_eq!(
            plan_of(&["a", "b", "c"], true, &options),
            Plan::Invalid(Problem::ExtraOperand("c".to_string()))
        );
        match plan_of(&["a", "b"], true, &options) {
            Plan::Pairs(pairs) => assert_eq!(pairs[0].destination, PathBuf::from("b")),
            other => panic!("expected pairs, got {:?}", other),
        }
    }

    #[test]
    fn missing_operands_are_reported() {
        assert_eq!(plan_of(&[], false, &opts()), Plan::Invalid(Problem::MissingOperand));
        assert_eq!(plan_of(&["a"], false, &opts()), Plan::Invalid(Problem::MissingOperand));
    }

    #[test]
    fn only_the_last_of_i_f_n_counts() {
        assert_eq!(
            Overwrite::resolve([Overwrite::Interactive, Overwrite::Force].into_iter()),
            Overwrite::Force
        );
        assert_eq!(
            Overwrite::resolve([Overwrite::Force, Overwrite::NoClobber].into_iter()),
            Overwrite::NoClobber
        );
        assert_eq!(Overwrite::resolve([].into_iter()), Overwrite::Force);
    }

    #[test]
    fn update_none_reports_that_nothing_was_replaced() {
        let options = Options {
            overwrite: Overwrite::NoClobber,
            ..opts()
        };
        assert_eq!(
            decide(false, true, false, false, true, None, None, &options),
            Action::NotReplacing
        );
    }

    #[test]
    fn update_older_skips_a_newer_destination() {
        let options = Options {
            update: Update::Older,
            ..opts()
        };
        assert_eq!(
            decide(false, true, false, false, true, Some((1, 0)), Some((2, 0)), &options),
            Action::SkipNotNewer
        );
        assert_eq!(
            decide(false, true, false, false, true, Some((3, 0)), Some((2, 0)), &options),
            Action::Move
        );
    }

    #[test]
    fn interactive_asks_before_replacing() {
        let options = Options {
            overwrite: Overwrite::Interactive,
            ..opts()
        };
        assert_eq!(
            decide(false, true, false, false, true, None, None, &options),
            Action::OverwriteConfirmed
        );
    }

    #[test]
    fn a_directory_over_a_file_is_refused() {
        // An empty source directory could be replaced after removing the file,
        // so the complaint is about the target.
        assert_eq!(
            decide(true, true, false, true, true, None, None, &opts()),
            Action::TargetNotADirectory
        );
        // One with entries in it cannot be, so the wording is about the
        // overwrite.
        assert_eq!(
            decide(true, true, false, false, true, None, None, &opts()),
            Action::OverwriteNonDirectory
        );
    }

    #[test]
    fn no_clobber_says_it_is_not_replacing() {
        let options = Options {
            overwrite: Overwrite::NoClobber,
            ..opts()
        };
        assert_eq!(
            decide(false, true, false, false, true, None, None, &options),
            Action::NotReplacing
        );
    }

    #[test]
    fn a_non_empty_destination_directory_is_refused() {
        assert_eq!(
            decide(true, true, true, false, true, None, None, &opts()),
            Action::RefuseNonEmptyDirectory
        );
    }

    #[test]
    fn an_empty_destination_directory_is_fine() {
        assert_eq!(
            decide(true, true, true, true, true, None, None, &opts()),
            Action::Move
        );
    }

    #[test]
    fn no_copy_refuses_a_cross_device_move() {
        let options = Options { no_copy: true, ..opts() };
        assert_eq!(
            decide(false, false, false, false, false, None, None, &options),
            Action::RefuseCrossDevice
        );
        assert_eq!(
            decide(false, false, false, false, true, None, None, &options),
            Action::Move
        );
    }

    #[test]
    fn backup_names_use_the_suffix() {
        assert_eq!(backup_name("b", &opts(), None), "b~");
        assert_eq!(backup_name("b", &opts(), Some(".old")), "b.old");
        assert!(wants_backup(&opts(), Some("simple")));
        assert!(!wants_backup(
            &Options {
                backup: Some("none".into()),
                ..opts()
            },
            None
        ));
    }

    #[test]
    fn messages_match_coreutils() {
        assert_eq!(
            missing_destination_message("a"),
            "mv: missing destination file operand after 'a'"
        );
        assert_eq!(not_replacing_message("b"), "mv: not replacing 'b'");
        assert_eq!(
            target_directory_missing_message("nodir"),
            "mv: target directory 'nodir': No such file or directory"
        );
        assert_eq!(
            target_not_a_directory_message("plain"),
            "mv: target 'plain': Not a directory"
        );
        assert_eq!(
            overwrite_non_directory_message("plain", "tree"),
            "mv: cannot overwrite non-directory 'plain' with directory 'tree'"
        );
        assert_eq!(
            overwrite_directory_message("dest"),
            "mv: cannot overwrite directory 'dest' with non-directory"
        );
        assert_eq!(invalid_option_message('Z'), "mv: invalid option -- 'Z'");
        assert_eq!(missing_source_message("a"), "mv: cannot stat 'a': No such file or directory");
        assert_eq!(
            cannot_move_message("a", "b", "Permission denied"),
            "mv: cannot move 'a' to 'b': Permission denied"
        );
        assert_eq!(
            directory_not_empty_message("d"),
            "mv: cannot move 'd' to a non-empty directory"
        );
        assert_eq!(
            cross_device_message("a", "b"),
            "mv: cannot move 'a' to 'b': Invalid cross-device link"
        );
        assert_eq!(prompt_message("b"), "mv: overwrite 'b'? ");
    }

    #[test]
    fn verbose_lines_follow_coreutils() {
        let pair = Pair {
            source: PathBuf::from("a"),
            destination: PathBuf::from("b"),
        };
        assert_eq!(pair.verbose_line(), "renamed 'a' -> 'b'");
    }
}