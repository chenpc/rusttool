//! Decision logic of `ln(1)`, following its manual page:
//!
//! ```text
//! ln [OPTION]... [-T] TARGET LINK_NAME
//! ln [OPTION]... TARGET
//! ln [OPTION]... TARGET... DIRECTORY
//! ln [OPTION]... -t DIRECTORY TARGET...
//! ```
//!
//! The manual is explicit about the four forms, about hard links being the
//! default while -s makes symbolic ones, about -f removing an existing
//! destination and about -r rewriting the target so it is relative to the link.

use std::path::{Component, Path, PathBuf};

/// The options `ln` accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// -s/--symbolic
    pub symbolic: bool,
    /// -f/--force
    pub force: bool,
    /// -i/--interactive
    pub interactive: bool,
    /// -v/--verbose
    pub verbose: bool,
    /// -n/--no-dereference
    pub no_dereference: bool,
    /// -r/--relative
    pub relative: bool,
    /// -T/--no-target-directory
    pub no_target_directory: bool,
    /// -t/--target-directory=DIRECTORY
    pub target_directory: Option<String>,
    /// -b/--backup
    pub backup: bool,
    /// -S/--suffix=SUFFIX
    pub suffix: String,
    /// -d/-F/--directory: let the superuser try to link directories.
    pub directory: bool,
    /// -L/--logical and -P/--physical.
    pub logical: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            symbolic: false,
            force: false,
            interactive: false,
            verbose: false,
            no_dereference: false,
            relative: false,
            no_target_directory: false,
            target_directory: None,
            backup: false,
            suffix: "~".to_string(),
            directory: false,
            logical: false,
        }
    }
}

/// One link to make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub target: PathBuf,
    pub destination: PathBuf,
}

/// What the operands mean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    Links(Vec<Link>),
    Invalid(Problem),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// No operands at all.
    MissingOperand,
    /// Too many operands with -T.
    ExtraOperand(String),
    /// The last operand cannot be a directory because it exists as a file.
    TargetNotADirectory(String),
    /// -t names something that is not a directory.
    TargetDirectory(String),
}

/// Work out the links the operands describe.
pub fn plan(operands: &[String], last_is_dir: bool, options: &Options) -> Plan {
    // -t names the directory, so every operand is a target.
    if let Some(directory) = options.target_directory.clone() {
        let targets: Vec<String> = operands.to_vec();
        return links_into(&targets, &directory);
    }

    match operands.len() {
        0 => Plan::Invalid(Problem::MissingOperand),
        1 => {
            // The second form: the link goes in the current directory under the
            // target's own name, which coreutils writes with a leading "./".
            let target = &operands[0];
            let name = base_name(Path::new(target));
            Plan::Links(vec![Link {
                target: PathBuf::from(target),
                destination: PathBuf::from(".").join(name),
            }])
        }
        _ => {
            let (destination, targets) = operands.split_last().expect("at least two operands");
            if options.no_target_directory {
                if targets.len() > 1 {
                    return Plan::Invalid(Problem::ExtraOperand(destination.clone()));
                }
                return Plan::Links(vec![Link {
                    target: PathBuf::from(&targets[0]),
                    destination: PathBuf::from(destination),
                }]);
            }
            if last_is_dir {
                return links_into(targets, destination);
            }
            if targets.len() > 1 {
                // The manual's forms leave no room for that.
                return Plan::Invalid(Problem::TargetNotADirectory(destination.to_string()));
            }
            Plan::Links(vec![Link {
                target: PathBuf::from(&targets[0]),
                destination: PathBuf::from(destination),
            }])
        }
    }
}

fn links_into(targets: &[String], directory: &str) -> Plan {
    Plan::Links(
        targets
            .iter()
            .map(|target| Link {
                target: PathBuf::from(target),
                destination: PathBuf::from(directory).join(base_name(Path::new(target))),
            })
            .collect(),
    )
}

/// The name a target is linked under inside a directory.
pub fn base_name(path: &Path) -> PathBuf {
    match path.components().next_back() {
        Some(Component::Normal(name)) => PathBuf::from(name),
        _ => path.to_path_buf(),
    }
}

/// What should happen to one link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Make it.
    Create,
    /// -f or -n: the destination is in the way and is replaced.
    Replace,
    /// -i: the user was asked and said no.
    SkipDeclined,
    /// The destination exists and nothing was asked to remove it.
    RefuseExists,
}

/// Decide what to do with one link.
///
/// `dest_is_symlink_to_dir` is what -n is about: the manual says -n treats the
/// name as a normal file when it is a symbolic link to a directory.
pub fn decide(
    dest_exists: bool,
    dest_is_symlink_to_dir: bool,
    declined: bool,
    options: &Options,
) -> Action {
    if declined {
        return Action::SkipDeclined;
    }
    if !dest_exists {
        return Action::Create;
    }
    if options.force {
        return Action::Replace;
    }
    if options.backup {
        return Action::Replace;
    }
    if options.no_dereference && dest_is_symlink_to_dir {
        return Action::Replace;
    }
    Action::RefuseExists
}

/// The target a `-s -r` link should hold: the same target written relative to
/// the directory the link goes in, as the manual describes.
pub fn relative_target(target: &str, destination: &Path) -> String {
    let target_path = Path::new(target);
    let directory = match destination.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    // Count how many components the two paths share, which is what makes the
    // path relative.
    let target_parts: Vec<String> = target_path
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let directory_parts: Vec<String> = directory
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let mut shared = 0usize;
    while shared < target_parts.len()
        && shared < directory_parts.len()
        && target_parts[shared] == directory_parts[shared]
    {
        shared += 1;
    }
    let mut out = PathBuf::new();
    for _ in shared..directory_parts.len() {
        out.push("..");
    }
    for part in &target_parts[shared..] {
        out.push(part);
    }
    if out.as_os_str().is_empty() {
        return target.to_string();
    }
    out.to_string_lossy().into_owned()
}

/// The backup name for an existing destination.
pub fn backup_name(destination: &str, suffix: &str) -> String {
    format!("{}{}", destination, suffix)
}

/// The text -v prints, which names the link before its target.
pub fn verbose_line(destination: &str, target: &str) -> String {
    format!("'{}' -> '{}'", destination, target)
}

/// GNU diagnostics.
pub fn missing_operand_message() -> String {
    "ln: missing file operand".to_string()
}

pub fn extra_operand_message(operand: &str) -> String {
    format!("ln: extra operand '{}'", operand)
}

/// -t names something that cannot be a directory.
pub fn target_directory_message(directory: &str, reason: &str) -> String {
    format!("ln: failed to access '{}': {}", directory, reason)
}

/// The last operand is not a directory, which is worded about the target.
pub fn target_not_a_directory_message(destination: &str) -> String {
    format!("ln: target '{}': Not a directory", destination)
}

pub fn file_exists_message(symbolic: bool, destination: &str) -> String {
    failed_to_create_message(symbolic, destination, "File exists")
}

pub fn cannot_stat_message(target: &str) -> String {
    format!("ln: failed to access '{}': No such file or directory", target)
}

pub fn same_file_message(target: &str, destination: &str) -> String {
    format!("ln: '{}' and '{}' are the same file", target, destination)
}

/// The wording depends on the kind of link that could not be made.
pub fn failed_to_create_message(symbolic: bool, destination: &str, reason: &str) -> String {
    let kind = if symbolic {
        "symbolic link"
    } else {
        "hard link"
    };
    format!("ln: failed to create {} '{}': {}", kind, destination, reason)
}

pub fn unrecognized_option_message(option: &str) -> String {
    format!("ln: unrecognized option '{}'", option)
}

pub fn invalid_option_message(letter: char) -> String {
    format!("ln: invalid option -- '{}'", letter)
}

pub fn requires_argument_message(name: &str) -> String {
    format!("ln: option '--{}' requires an argument", name)
}

pub fn try_help_message() -> String {
    "Try 'ln --help' for more information.".to_string()
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
    fn the_first_form_names_the_link() {
        match plan_of(&["a", "b"], false, &opts()) {
            Plan::Links(links) => {
                assert_eq!(links[0].target, PathBuf::from("a"));
                assert_eq!(links[0].destination, PathBuf::from("b"));
            }
            other => panic!("expected links, got {:?}", other),
        }
    }

    #[test]
    fn one_operand_links_in_the_current_directory() {
        match plan_of(&["dir/a"], false, &opts()) {
            Plan::Links(links) => assert_eq!(links[0].destination, PathBuf::from("./a")),
            other => panic!("expected links, got {:?}", other),
        }
    }

    #[test]
    fn one_target_and_a_plain_file_is_the_link_name() {
        match plan_of(&["a", "b", "dir"], true, &opts()) {
            Plan::Links(links) => {
                assert_eq!(links[0].destination, PathBuf::from("dir/a"));
                assert_eq!(links[1].destination, PathBuf::from("dir/b"));
            }
            other => panic!("expected links, got {:?}", other),
        }
    }

    #[test]
    fn no_target_directory_allows_one_target() {
        let options = Options {
            no_target_directory: true,
            ..opts()
        };
        match plan_of(&["a", "b"], false, &options) {
            Plan::Links(links) => assert_eq!(links[0].destination, PathBuf::from("b")),
            other => panic!("expected links, got {:?}", other),
        }
        assert_eq!(
            plan_of(&["a", "b", "c"], false, &options),
            Plan::Invalid(Problem::ExtraOperand("c".into()))
        );
    }

    #[test]
    fn target_directory_option() {
        let options = Options {
            target_directory: Some("dir".into()),
            ..opts()
        };
        match plan_of(&["a", "b"], false, &options) {
            Plan::Links(links) => assert_eq!(links[1].destination, PathBuf::from("dir/b")),
            other => panic!("expected links, got {:?}", other),
        }
    }

    #[test]
    fn no_operand_is_an_error() {
        assert_eq!(plan_of(&[], false, &opts()), Plan::Invalid(Problem::MissingOperand));
    }

    #[test]
    fn an_existing_destination_needs_force_or_backup() {
        assert_eq!(decide(true, false, false, &opts()), Action::RefuseExists);
        let options = Options {
            force: true,
            ..opts()
        };
        assert_eq!(decide(true, false, false, &options), Action::Replace);
        let options = Options {
            backup: true,
            ..opts()
        };
        assert_eq!(decide(true, false, false, &options), Action::Replace);
    }

    #[test]
    fn no_dereference_replaces_a_link_to_a_directory() {
        let options = Options {
            no_dereference: true,
            ..opts()
        };
        assert_eq!(decide(true, true, false, &options), Action::Replace);
        assert_eq!(decide(true, false, false, &options), Action::RefuseExists);
    }

    #[test]
    fn a_declined_prompt_skips() {
        assert_eq!(decide(true, false, true, &opts()), Action::SkipDeclined);
    }

    #[test]
    fn relative_targets_are_rewritten() {
        // A link in the same directory keeps the target as it is.
        assert_eq!(relative_target("a/b", Path::new("c")), "a/b");
        // One directory down, the target gains a step back.
        assert_eq!(relative_target("a/b", Path::new("c/d")), "../a/b");
        // Sharing a prefix shortens the path.
        assert_eq!(relative_target("a/b/c", Path::new("a/x")), "b/c");
    }

    #[test]
    fn backup_names_use_the_suffix() {
        assert_eq!(backup_name("b", "~"), "b~");
        assert_eq!(backup_name("b", ".old"), "b.old");
    }

    #[test]
    fn messages_match_coreutils() {
        assert_eq!(missing_operand_message(), "ln: missing file operand");
        assert_eq!(
            file_exists_message(true, "b"),
            "ln: failed to create symbolic link 'b': File exists"
        );
        assert_eq!(
            file_exists_message(false, "b"),
            "ln: failed to create hard link 'b': File exists"
        );
        assert_eq!(
            target_directory_message("nodir", "No such file or directory"),
            "ln: failed to access 'nodir': No such file or directory"
        );
        assert_eq!(
            target_not_a_directory_message("b"),
            "ln: target 'b': Not a directory"
        );
        assert_eq!(verbose_line("b", "a"), "'b' -> 'a'");
        assert_eq!(invalid_option_message('Z'), "ln: invalid option -- 'Z'");
    }
}