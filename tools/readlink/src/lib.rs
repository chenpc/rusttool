//! Decision logic of `readlink(1)`, following its manual page:
//!
//! ```text
//! readlink [OPTION]... FILE...
//! ```
//!
//! The four printing modes are the interesting part: with no mode at all the
//! link's own target is printed, while -f, -e and -m canonicalize with three
//! different rules about which components have to exist.

/// How the link's value is printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// No option: print the target of the link itself.
    Target,
    /// -f/--canonicalize: every component is resolved; all but the last must
    /// exist.
    Canonicalize,
    /// -e/--canonicalize-existing: every component is resolved and all of them
    /// must exist.
    CanonicalizeExisting,
    /// -m/--canonicalize-missing: resolve without requiring anything to exist.
    CanonicalizeMissing,
}

impl Mode {
    /// Whether every component of the name has to exist for this mode.
    pub fn requires_all_components(&self) -> bool {
        matches!(self, Mode::Canonicalize | Mode::CanonicalizeExisting)
    }

    /// Whether the final component has to exist.
    pub fn requires_last_component(&self) -> bool {
        matches!(self, Mode::CanonicalizeExisting)
    }
}

/// The options `readlink` accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub mode: Mode,
    /// -n/--no-newline
    pub no_newline: bool,
    /// -z/--zero
    pub zero: bool,
    /// -v/--verbose reports errors; -q/-s are the silent default.
    pub verbose: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            mode: Mode::Target,
            no_newline: false,
            zero: false,
            verbose: false,
        }
    }
}

impl Options {
    /// The byte that ends each line: NUL with -z, otherwise newline, unless -n
    /// removes it.
    pub fn terminator(&self) -> Option<u8> {
        if self.no_newline {
            None
        } else if self.zero {
            Some(0)
        } else {
            Some(b'\n')
        }
    }
}

/// What should happen to one operand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Print the link's target, or the canonical name.
    Print(String),
    /// The operand is not a symbolic link and no canonicalizing option was
    /// given: nothing is printed and the status is non-zero.
    NotASymbolicLink,
    /// A component that has to exist is missing.
    Missing,
}

/// Decide what to print for one operand.
///
/// `is_link` says whether the operand is a symbolic link, `target` is its
/// value, and `canonical` is the canonical name the filesystem gives (already
/// resolved as far as the mode allows).
pub fn decide(
    is_link: bool,
    target: Option<&str>,
    canonical: Option<&str>,
    mode: Mode,
) -> Action {
    match mode {
        Mode::Target => match (is_link, target) {
            (true, Some(target)) => Action::Print(target.to_string()),
            // A name that is not a link prints nothing, which is what makes
            // readlink usable in a shell test.
            _ => Action::NotASymbolicLink,
        },
        Mode::CanonicalizeExisting => match canonical {
            Some(name) => Action::Print(name.to_string()),
            None => Action::Missing,
        },
        Mode::Canonicalize | Mode::CanonicalizeMissing => match canonical {
            Some(name) => Action::Print(name.to_string()),
            // -f only needs all but the last component, so a missing tail is
            // still printed; -m never fails for that reason.
            None => Action::Missing,
        },
    }
}

/// GNU diagnostics.
pub fn invalid_option_message(letter: &str) -> String {
    format!("readlink: invalid option -- '{}'", letter)
}

pub fn unrecognized_option_message(option: &str) -> String {
    // The name arrives with its leading dashes already on it.
    format!("readlink: unrecognized option '{}'", option)
}

pub fn too_many_arguments_message() -> String {
    "readlink: extra operand".to_string()
}

pub fn missing_operand_message() -> String {
    "readlink: missing operand".to_string()
}

pub fn not_a_directory_message(path: &str) -> String {
    format!("readlink: failed to access canonical name of '{}': Not a directory", path)
}

pub fn no_such_file_message(path: &str) -> String {
    format!(
        "readlink: failed to access canonical name of '{}': No such file or directory",
        path
    )
}

pub fn try_help_message() -> String {
    "Try 'readlink --help' for more information.".to_string()
}

pub fn invalid_mode_message() -> String {
    "readlink: cannot canonicalize".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_four_modes_are_the_documented_ones() {
        assert_eq!(Mode::Target.requires_all_components(), false);
        assert_eq!(Mode::Canonicalize.requires_all_components(), true);
        assert_eq!(Mode::CanonicalizeExisting.requires_all_components(), true);
        assert_eq!(Mode::CanonicalizeMissing.requires_all_components(), false);
        assert_eq!(Mode::Canonicalize.requires_last_component(), false);
        assert_eq!(Mode::CanonicalizeExisting.requires_last_component(), true);
    }

    #[test]
    fn without_options_a_link_target_is_printed() {
        assert_eq!(
            decide(true, Some("elsewhere"), None, Mode::Target),
            Action::Print("elsewhere".to_string())
        );
    }

    #[test]
    fn without_options_a_plain_file_prints_nothing() {
        assert_eq!(decide(false, None, Some("/plain"), Mode::Target), Action::NotASymbolicLink);
    }

    #[test]
    fn canonicalizing_prints_the_canonical_name() {
        assert_eq!(
            decide(true, Some("../x"), Some("/a/x"), Mode::Canonicalize),
            Action::Print("/a/x".to_string())
        );
        assert_eq!(
            decide(false, None, Some("/a/x"), Mode::CanonicalizeMissing),
            Action::Print("/a/x".to_string())
        );
        assert_eq!(
            decide(false, None, Some("/a/x"), Mode::CanonicalizeExisting),
            Action::Print("/a/x".to_string())
        );
    }

    #[test]
    fn a_missing_canonical_name_is_reported() {
        assert_eq!(decide(true, Some("x"), None, Mode::CanonicalizeExisting), Action::Missing);
        assert_eq!(decide(true, Some("x"), None, Mode::Canonicalize), Action::Missing);
    }

    #[test]
    fn line_endings_follow_the_options() {
        assert_eq!(Options::default().terminator(), Some(b'\n'));
        let no_newline = Options {
            no_newline: true,
            ..Options::default()
        };
        assert_eq!(no_newline.terminator(), None);
        let zero = Options {
            zero: true,
            ..Options::default()
        };
        assert_eq!(zero.terminator(), Some(0));
        let both = Options {
            zero: true,
            no_newline: true,
            ..Options::default()
        };
        assert_eq!(both.terminator(), None);
    }

    #[test]
    fn messages_match_coreutils() {
        assert_eq!(
            invalid_option_message("Z"),
            "readlink: invalid option -- 'Z'"
        );
        assert_eq!(missing_operand_message(), "readlink: missing operand");
        assert_eq!(
            no_such_file_message("nope"),
            "readlink: failed to access canonical name of 'nope': No such file or directory"
        );
    }
}