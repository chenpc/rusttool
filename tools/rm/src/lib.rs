//! Decision logic of `rm(1)`: what to do with each operand, independent of the
//! filesystem calls that carry it out.

/// The options `rm` accepts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    pub force: bool,
    pub recursive: bool,
    pub dir: bool,
    pub interactive: bool,
    pub one_file_system: bool,
    pub preserve_root: bool,
}

/// What should happen to one operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Remove it (file, or directory with -r/-d).
    Remove,
    /// Nothing to do: it is already gone and -f was given.
    IgnoreMissing,
    /// Refuse: a directory without -r/-d.
    RefuseDirectory,
}

/// Decide what to do with `path`, given its `is_dir` and whether it exists.
pub fn decide(path: &str, exists: bool, is_dir: bool, options: &Options) -> Action {
    if !exists {
        return if options.force { Action::IgnoreMissing } else { Action::Remove };
    }
    if is_dir && !(options.recursive || options.dir) {
        return Action::RefuseDirectory;
    }
    Action::Remove
}

/// GNU messages, kept byte-compatible with coreutils.
pub fn missing_message(path: &str) -> String {
    format!("rm: cannot remove '{}': No such file or directory", path)
}

pub fn directory_message(path: &str) -> String {
    format!("rm: cannot remove '{}': Is a directory", path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_file_is_removed() {
        assert_eq!(decide("a", true, false, &Options::default()), Action::Remove);
    }

    #[test]
    fn directory_needs_recursive() {
        assert_eq!(decide("d", true, true, &Options::default()), Action::RefuseDirectory);
        assert_eq!(
            decide("d", true, true, &Options { recursive: true, ..Options::default() }),
            Action::Remove
        );
        assert_eq!(
            decide("d", true, true, &Options { dir: true, ..Options::default() }),
            Action::Remove
        );
    }

    #[test]
    fn missing_file_is_ignored_only_with_force() {
        assert_eq!(decide("x", false, false, &Options::default()), Action::Remove);
        assert_eq!(
            decide("x", false, false, &Options { force: true, ..Options::default() }),
            Action::IgnoreMissing
        );
    }

    #[test]
    fn messages_match_coreutils() {
        assert_eq!(
            missing_message("x"),
            "rm: cannot remove 'x': No such file or directory"
        );
        assert_eq!(directory_message("d"), "rm: cannot remove 'd': Is a directory");
    }
}
