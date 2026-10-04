//! Decision logic of `cp(1)`, following the GNU coreutils manual page for
//! `cp`:
//!
//! ```text
//! cp [OPTION]... [-T] SOURCE DEST
//! cp [OPTION]... SOURCE... DIRECTORY
//! cp [OPTION]... -t DIRECTORY SOURCE...
//! ```
//!
//! Everything here is pure: which (source, destination) pairs the operands
//! mean, what should happen to each pair, and the exact wording of the
//! diagnostics. The filesystem work lives in `main.rs`.

use std::path::{Component, Path, PathBuf};

/// `--sparse=WHEN` / `--reflink=WHEN`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    Always,
    Never,
    Auto,
    /// `--reflink` with no argument, which the manual defines as `always`.
    Bare,
}

impl When {
    /// Parse the argument of `--sparse` / `--reflink`. An empty or missing
    /// value is `always`, which is what the manual calls the default form.
    pub fn parse(text: &str) -> Result<When, String> {
        match text {
            "" | "always" => Ok(When::Always),
            "never" => Ok(When::Never),
            "auto" => Ok(When::Auto),
            other => Err(other.to_string()),
        }
    }
}

/// `--update[=UPDATE]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Update {
    /// Replace every existing destination (the default when -u is absent).
    All,
    /// `none`: replace nothing, and skipping is not a failure.
    None,
    /// `older`: replace only when the destination is older.
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

/// The file attributes `--preserve` and `--no-preserve` talk about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attributes {
    pub mode: bool,
    pub ownership: bool,
    pub timestamps: bool,
    pub links: bool,
    pub xattr: bool,
    pub context: bool,
}

impl Attributes {
    pub const NONE: Attributes = Attributes {
        mode: false,
        ownership: false,
        timestamps: false,
        links: false,
        xattr: false,
        context: false,
    };

    pub const ALL: Attributes = Attributes {
        mode: true,
        ownership: true,
        timestamps: true,
        links: true,
        xattr: true,
        context: true,
    };

    /// `-p`: `--preserve=mode,ownership,timestamps`.
    pub fn mode_ownership_timestamps() -> Attributes {
        Attributes {
            mode: true,
            ownership: true,
            timestamps: true,
            ..Attributes::NONE
        }
    }

    /// Parse an `ATTR_LIST`; an empty list means `all`, as documented.
    pub fn parse(text: &str) -> Result<Attributes, String> {
        if text.is_empty() {
            return Ok(Attributes::ALL);
        }
        let mut set = Attributes::NONE;
        for item in text.split(',') {
            match item {
                "mode" => set.mode = true,
                "ownership" => set.ownership = true,
                "timestamps" => set.timestamps = true,
                "links" => set.links = true,
                "xattr" => set.xattr = true,
                "context" => set.context = true,
                "all" => return Ok(Attributes::ALL),
                other => return Err(other.to_string()),
            }
        }
        Ok(set)
    }

    /// Apply `--no-preserve=ATTR_LIST` on top of what is already requested.
    pub fn without(&self, text: &str) -> Result<Attributes, String> {
        let drop = Attributes::parse(text)?;
        Ok(Attributes {
            mode: self.mode && !drop.mode,
            ownership: self.ownership && !drop.ownership,
            timestamps: self.timestamps && !drop.timestamps,
            links: self.links && !drop.links,
            xattr: self.xattr && !drop.xattr,
            context: self.context && !drop.context,
        })
    }
}

/// How symbolic links are treated. The manual lists three flags, and they act
/// on two different places:
///
/// * -P/--no-dereference, and the -d half of -a: never follow a link.
/// * -H: follow the links named on the command line, but not those inside a
///   copied tree.
/// * -L/--dereference: follow them everywhere.
///
/// A link named on the command line is followed by default, while a link found
/// inside a copied tree is copied as a link; that is what GNU cp does, and it
/// is why the two dimensions are separate fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymlinkPolicy {
    /// Follow a symlink given as a SOURCE operand.
    pub command_line: bool,
    /// Follow a symlink met while walking a directory.
    pub tree: bool,
}

impl Default for SymlinkPolicy {
    fn default() -> SymlinkPolicy {
        SymlinkPolicy {
            command_line: true,
            tree: false,
        }
    }
}

impl SymlinkPolicy {
    /// -P/--no-dereference, and -d.
    pub fn no_follow() -> SymlinkPolicy {
        SymlinkPolicy {
            command_line: false,
            tree: false,
        }
    }

    /// -H.
    pub fn follow_command_line() -> SymlinkPolicy {
        SymlinkPolicy {
            command_line: true,
            tree: false,
        }
    }

    /// -L/--dereference.
    pub fn follow_all() -> SymlinkPolicy {
        SymlinkPolicy {
            command_line: true,
            tree: true,
        }
    }

    /// Map the flag names from the manual onto a policy.
    pub fn parse(flag: &str) -> SymlinkPolicy {
        match flag {
            "-P" | "--no-dereference" | "-d" => SymlinkPolicy::no_follow(),
            "-H" => SymlinkPolicy::follow_command_line(),
            _ => SymlinkPolicy::follow_all(),
        }
    }
}

/// The options `cp` accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub archive: bool,
    pub recursive: bool,
    pub preserve: Attributes,
    pub verbose: bool,
    pub force: bool,
    pub interactive: bool,
    pub update: Update,
    pub hard_link: bool,
    pub symbolic: bool,
    pub symlinks: SymlinkPolicy,
    /// -t DIRECTORY: copy every source into DIRECTORY.
    pub target_directory: Option<String>,
    /// -T: treat the destination as a normal file.
    pub no_target_directory: bool,
    /// --parents: use the full source name under the destination directory.
    pub parents: bool,
    /// --backup, with the control word from --backup=CONTROL when given.
    pub backup: Option<String>,
    /// -S/--suffix, defaulting to `~`.
    pub suffix: String,
    pub strip_trailing_slashes: bool,
    pub one_file_system: bool,
    pub attributes_only: bool,
    pub remove_destination: bool,
    pub sparse: When,
    pub reflink: When,
    pub copy_contents: bool,
    pub debug: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            archive: false,
            recursive: false,
            preserve: Attributes::NONE,
            verbose: false,
            force: false,
            interactive: false,
            update: Update::All,
            hard_link: false,
            symbolic: false,
symlinks: SymlinkPolicy::default(),
            target_directory: None,
            no_target_directory: false,
            parents: false,
            backup: None,
            suffix: "~".to_string(),
            strip_trailing_slashes: false,
            one_file_system: false,
            attributes_only: false,
            remove_destination: false,
            sparse: When::Auto,
            reflink: When::Bare,
            copy_contents: false,
            debug: false,
        }
    }
}

impl Options {
    /// Fold the options that mean "and also": `-a`, `-p`, `-d`, `-n` and the
    /// override rules the manual spells out.
    pub fn effective(&self) -> Options {
        let mut o = self.clone();
        if o.archive {
            // -a is -dR --preserve=all
            o.recursive = true;
            o.symlinks = SymlinkPolicy::no_follow();
            o.preserve = Attributes::ALL;
        }
        if o.debug {
            o.verbose = true;
        }
        if o.hard_link || o.symbolic {
            o.recursive = false;
        }
        // -n is equivalent to --update=none, and it overrides -u and a previous
        // -i; -f is ignored when -n is also used.
        if o.update == Update::None {
            o.interactive = false;
        }
        if o.interactive {
            o.update = Update::All;
        }
        if o.update == Update::None {
            o.force = false;
        }
        // -s/-l cannot be combined with each other; the manual leaves the
        // diagnostic to cp, which reports it once per conflicting pair.
        if o.symbolic {
            o.preserve = Attributes::NONE;
        }
        o
    }

    /// True when a symbolic link given on the command line should be followed.
    pub fn follows_command_line_symlink(&self) -> bool {
        self.effective().symlinks.command_line
    }

    /// True when a symbolic link found inside a copied tree should be
    /// followed: only -L does that.
    pub fn follows_tree_symlink(&self) -> bool {
        self.effective().symlinks.tree
    }

    /// Mark -n as given, which is what `--update=none` means.
    pub fn with_update_none(&self) -> Options {
        let mut o = self.clone();
        o.update = Update::None;
        o
    }

    /// The name of the last option that conflicts, if any of -s/-l/-a are
    /// combined with -r.
    pub fn conflicting_recursion(&self) -> Option<&'static str> {
        let o = self.effective();
        if !o.recursive {
            return None;
        }
        if o.symbolic {
            Some("--symbolic-link")
        } else if o.hard_link {
            Some("--link")
        } else {
            None
        }
    }
}

/// What the operand list means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// No source operands: standard input goes to the destination.
    Stdin(PathBuf),
    /// Copy these pairs: (source, destination, name to report).
    Pairs(Vec<Pair>),
    /// The operands are wrong; nothing is copied and cp exits non-zero.
    Invalid(Problem),
}

/// One source and where it lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pair {
    pub source: PathBuf,
    pub destination: PathBuf,
    /// The source as the user wrote it, which the diagnostics quote.
    pub spelled: String,
}

impl Pair {
    pub fn source_name(&self) -> String {
        self.source.to_string_lossy().into_owned()
    }

    pub fn destination_name(&self) -> String {
        self.destination.to_string_lossy().into_owned()
    }

    /// The text `-v` prints: `source -> destination`.
    pub fn verbose_line(&self) -> String {
        format!("'{}' -> '{}'", self.source_name(), self.destination_name())
    }
}

/// Something wrong with the operands themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// The destination cannot hold more than one source. The wording carries
    /// the reason the destination was unusable, which is what coreutils
    /// reports: "No such file or directory" when it is missing, "Not a
    /// directory" when it is a plain file.
    TargetUnavailable { destination: String, reason: String },
    /// The same, for the directory named by -t.
    TargetDirectoryUnavailable { directory: String, reason: String },
    /// No operands at all.
    MissingOperand,
    /// -T leaves no room for a second source, so the rest are extra.
    ExtraOperand(String),
    /// --parents needs an existing directory to copy the name into.
    ParentsNeedsDirectory,
}

/// The `-n` warning: the manual notes that -n is not portable and points at
/// the spelling that is.
pub const NO_CLOBBER_WARNING: &str =
    "cp: warning: behavior of -n is non-portable and may change in future; use --update=none instead";

/// Work out the pairs for an operand list.
///
/// `operands` is the positional arguments in order. `last_is_dir` says whether
/// the final operand is an existing directory, which is what makes it a
/// directory to copy *into* rather than the name of the copy.
pub fn plan(operands: &[String], last_is_dir: bool, options: &Options) -> Plan {
    let options = options.effective();

    // `-t DIRECTORY` names the destination up front, so every operand is a
    // source.
    if let Some(directory) = options.target_directory.clone() {
        if options.no_target_directory {
            return Plan::Invalid(Problem::TargetDirectoryUnavailable {
                directory,
                reason: "Not a directory".to_string(),
            });
        }
        return sources_into(&options, &operands, PathBuf::from(directory), true);
    }

    // Otherwise the last operand is the destination, and everything before it
    // is a source.
    let (destination, sources) = match operands.split_last() {
        None => return Plan::Invalid(Problem::MissingOperand),
        Some((last, rest)) => {
            if rest.is_empty() {
                // A lone operand is a source with no destination to copy it to.
                return Plan::Invalid(Problem::MissingOperand);
            }
            // -T makes the destination a plain file, so it can hold exactly one
            // source and anything after it is simply not addressable.
            if options.no_target_directory {
                if rest.len() > 1 {
                    return Plan::Invalid(Problem::ExtraOperand(last.clone()));
                }
                (last.clone(), rest.to_vec())
            } else if last_is_dir {
                // An existing directory destination takes every source.
                (last.clone(), rest.to_vec())
            } else if rest.len() > 1 {
                // Why the destination is unusable is decided by the caller,
                // which is the only side that can look at the filesystem.
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
    sources_into(&options, &sources, PathBuf::from(destination), into_directory)
}

/// Build the pairs for `sources`, which all land in `destination`.
fn sources_into(
    options: &Options,
    sources: &[String],
    destination: PathBuf,
    into_directory: bool,
) -> Plan {
    if sources.is_empty() {
        // No source at all: standard input is copied to the destination.
        return Plan::Stdin(destination);
    }
    let mut pairs = Vec::new();
    for raw in sources {
        // The spelling is kept for the diagnostics, because
        // --strip-trailing-slashes changes the path but not what the user
        // wrote.
        let spelled = raw.clone();
        let source = normalize_source(&raw, options);
        let target = if !into_directory {
            destination.clone()
        } else if options.parents {
            // --parents keeps the whole source name under DIRECTORY.
            destination.join(strip_leading_slashes(&source))
        } else {
            destination.join(base_name(&source))
        };
        pairs.push(Pair {
            spelled,
            source,
            destination: target,
        });
    }
    Plan::Pairs(pairs)
}

/// Apply `--strip-trailing-slashes`; without it a trailing slash is kept so
/// that `cp dir/ file` still refuses a directory.
pub fn normalize_source(source: &str, options: &Options) -> PathBuf {
    if options.strip_trailing_slashes {
        PathBuf::from(strip_trailing_slashes(source))
    } else {
        PathBuf::from(source)
    }
}

fn strip_trailing_slashes(source: &str) -> String {
    let trimmed = source.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

fn strip_leading_slashes(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The name a source is filed under when the destination is a directory: its
/// last component, or the whole path for `.`, `..` and `/`.
pub fn base_name(path: &Path) -> PathBuf {
    match path.components().next_back() {
        Some(Component::Normal(name)) => PathBuf::from(name),
        _ => path.to_path_buf(),
    }
}

/// What should happen to one pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Copy the bytes.
    Copy,
    /// `--attributes-only`: copy the metadata, leave the data alone.
    AttributesOnly,
    /// A symbolic link at the destination pointing at the source.
    Symlink,
    /// A hard link to the source inode.
    HardLink,
    /// `-n`, and the destination is already there.
    Skip,
    /// `--update=older`, and the destination is at least as new.
    SkipNotNewer,
    /// `-i`, and the user said no.
    SkipDeclined,
    /// A directory without -r.
    RefuseDirectory,
    /// The source is a directory and the destination is an existing file.
    DestinationIsFile,
}

/// Decide what to do with one pair, from what the filesystem says about it.
///
/// `source_is_dir`, `dest_exists`, `dest_is_dir` describe both ends;
/// `source_mtime` and `dest_mtime` are `(seconds, nanoseconds)` pairs that
/// feed `--update=older`.
pub fn decide(
    source_is_dir: bool,
    source_is_symlink: bool,
    dest_exists: bool,
    dest_is_dir: bool,
    source_mtime: Option<(i64, i64)>,
    dest_mtime: Option<(i64, i64)>,
    options: &Options,
) -> Action {
    let o = options.effective();

    if o.symbolic {
        return Action::Symlink;
    }
    if o.hard_link {
        if source_is_dir {
            return Action::RefuseDirectory;
        }
        return Action::HardLink;
    }
    if source_is_dir && !o.recursive {
        return Action::RefuseDirectory;
    }
    if source_is_dir && dest_exists && !dest_is_dir {
        return Action::DestinationIsFile;
    }
    if !source_is_dir && dest_exists && dest_is_dir {
        return Action::DestinationIsFile;
    }
    if dest_exists {
        match o.update {
            Update::None => return Action::Skip,
            Update::Older => {
                if let (Some(src), Some(dst)) = (source_mtime, dest_mtime) {
                    if dst >= src {
                        return Action::SkipNotNewer;
                    }
                }
            }
            Update::All => {}
        }
    }
    if o.attributes_only && !source_is_symlink {
        return Action::AttributesOnly;
    }
    Action::Copy
}

/// Whether an existing destination may be removed before writing (`-f`, or
/// `--remove-destination`), as opposed to being opened for writing.
pub fn remove_destination_first(options: &Options) -> bool {
    options.effective().remove_destination
}

/// The backup name for an existing destination: the destination plus the
/// suffix, which defaults to `~` and can come from `-S` or
/// `SIMPLE_BACKUP_SUFFIX`.
pub fn backup_name(destination: &str, options: &Options, env_suffix: Option<&str>) -> String {
    let suffix = if options.suffix != "~" {
        options.suffix.clone()
    } else {
        env_suffix.unwrap_or("~").to_string()
    };
    format!("{}{}", destination, suffix)
}

/// Whether a backup should be made, following `--backup[=CONTROL]` and the
/// `none`/`off` values that switch it off again.
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

/// Whether a symbolic link source should be recorded as a link rather than
/// followed, given what the manual says about -d/-a/-P/-L/-H.
pub fn treat_source_as_link(source_is_symlink: bool, on_command_line: bool, options: &Options) -> bool {
    if !source_is_symlink {
        return false;
    }
    if on_command_line {
        !options.follows_command_line_symlink()
    } else {
        !options.follows_tree_symlink()
    }
}

/// The mode a plain copy gives a new destination: readable and writable by
/// everyone, minus the umask.
pub fn plain_mode(umask: u32) -> u32 {
    0o666 & !umask
}

/// The mode a preserving copy gives the destination: the source's mode, setuid
/// and setgid bits included.
pub fn preserved_mode(source_mode: u32) -> u32 {
    source_mode & 0o7777
}

/// The mode a plain copy gives a new destination: the source's mode without the
/// setuid and setgid bits, which is what coreutils does even without -p.
pub fn destination_mode(source_mode: u32) -> u32 {
    source_mode & 0o0777
}

/// GNU diagnostics, kept byte-compatible with coreutils.
pub fn refuse_directory_message(source: &str) -> String {
    format!("cp: -r not specified; omitting directory '{}'", source)
}

pub fn not_a_directory_message(dest: &str) -> String {
    format!("cp: target '{}' is not a directory", dest)
}

/// The message coreutils uses when the destination cannot be created at all,
/// which is what a missing parent directory produces.
pub fn target_unavailable_message(dest: &str, reason: &str) -> String {
    format!("cp: target '{}': {}", dest, reason)
}

/// The -t spelling of the same problem.
pub fn target_directory_message(directory: &str, reason: &str) -> String {
    format!("cp: target directory '{}': {}", directory, reason)
}

/// The reason a destination cannot be a directory, from what stat says about
/// it. An existing plain file gives ENOTDIR, a missing path gives ENOENT.
pub fn target_reason(exists: bool, is_dir: bool) -> &'static str {
    if exists && !is_dir {
        "Not a directory"
    } else {
        "No such file or directory"
    }
}



pub fn extra_operand_message(operand: &str) -> String {
    format!("cp: extra operand '{}'", operand)
}

pub fn parents_needs_directory_message() -> String {
    "cp: with --parents, the destination must be a directory".to_string()
}

/// The "Try ..." line that follows a usage error.
pub fn try_help_message() -> String {
    "Try 'cp --help' for more information.".to_string()
}

pub fn unrecognized_option_message(option: &str) -> String {
    format!("cp: unrecognized option '{}'", option)
}

pub fn invalid_option_message(letter: char) -> String {
    format!("cp: invalid option -- '{}'", letter)
}

pub fn requires_argument_message(name: &str) -> String {
    format!("cp: option '--{}' requires an argument", name)
}

pub fn cannot_overwrite_directory_message(dest: &str) -> String {
    format!("cp: cannot overwrite directory '{}' with non-directory", dest)
}

pub fn cannot_overwrite_file_message(dest: &str) -> String {
    format!("cp: cannot overwrite non-directory '{}' with directory", dest)
}

pub fn same_file_message(source: &str, dest: &str) -> String {
    format!("cp: '{}' and '{}' are the same file", source, dest)
}

pub fn missing_operand_message() -> String {
    "cp: missing file operand".to_string()
}

/// One operand and no destination: coreutils names the operand it has.
pub fn missing_destination_message(operand: &str) -> String {
    format!(
        "cp: missing destination file operand after '{}'",
        operand
    )
}

pub fn stat_error(path: &str, reason: &str) -> String {
    format!("cp: cannot stat '{}': {}", path, reason)
}

/// The same complaint without a reason, which is what a missing source gets.
pub fn cannot_stat_message(path: &str) -> String {
    format!("cp: cannot stat '{}': No such file or directory", path)
}

/// A source that is a directory where a file was needed.
pub fn is_a_directory_message(path: &str) -> String {
    format!("cp: cannot open '{}' for reading: Is a directory", path)
}

pub fn open_error(path: &str, reason: &str) -> String {
    format!("cp: cannot open '{}' for reading: {}", path, reason)
}

pub fn create_error(path: &str, reason: &str) -> String {
    format!("cp: cannot create regular file '{}': {}", path, reason)
}

pub fn directory_create_error(path: &str, reason: &str) -> String {
    format!("cp: cannot create directory '{}': {}", path, reason)
}

pub fn preserving_times_error(path: &str, reason: &str) -> String {
    format!("cp: preserving times for '{}': {}", path, reason)
}

pub fn setting_permissions_error(path: &str, reason: &str) -> String {
    format!("cp: preserving permissions for '{}': {}", path, reason)
}

pub fn hard_link_error(source: &str, dest: &str, reason: &str) -> String {
    format!("cp: cannot create hard link to '{}': '{}' {}", source, dest, reason)
}

pub fn symbolic_link_error(dest: &str, reason: &str) -> String {
    format!("cp: cannot create symbolic link '{}': {}", dest, reason)
}

pub fn unlink_error(path: &str, reason: &str) -> String {
    format!("cp: cannot remove '{}': {}", path, reason)
}

pub fn prompt_message(destination: &str) -> String {
    format!("cp: overwrite '{}'? ", destination)
}

pub fn recursive_conflict_message(flag: &str) -> String {
    format!("cp: cannot both copy a directory and {}", flag)
}

pub fn invalid_update_message(value: &str) -> String {
    format!("cp: invalid argument '{}' for '--update' option", value)
}

pub fn invalid_attribute_message(value: &str) -> String {
    format!("cp: invalid attribute name '{}'", value)
}

pub fn invalid_when_message(value: &str) -> String {
    format!("cp: invalid argument '{}' for '--sparse' option", value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
    fn single_source_into_directory() {
        match plan_of(&["dir/a", "target"], true, &opts()) {
            Plan::Pairs(pairs) => assert_eq!(pairs[0].destination, PathBuf::from("target/a")),
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
    fn several_sources_land_in_the_directory() {
        match plan_of(&["a", "b", "target"], true, &opts()) {
            Plan::Pairs(pairs) => {
                assert_eq!(pairs[0].destination, PathBuf::from("target/a"));
                assert_eq!(pairs[1].destination, PathBuf::from("target/b"));
            }
            other => panic!("expected pairs, got {:?}", other),
        }
    }

    #[test]
    fn target_directory_option_takes_every_source() {
        let options = Options { target_directory: Some("target".into()), ..opts() };
        match plan_of(&["a", "b"], false, &options) {
            Plan::Pairs(pairs) => {
                assert_eq!(pairs[0].destination, PathBuf::from("target/a"));
                assert_eq!(pairs[1].destination, PathBuf::from("target/b"));
            }
            other => panic!("expected pairs, got {:?}", other),
        }
    }

    #[test]
    fn no_target_directory_makes_a_directory_destination_a_name() {
        // -T treats DEST as a normal file, so the directory-looking name is
        // simply the destination and a single source is fine.
        let options = Options { no_target_directory: true, ..opts() };
        match plan_of(&["a", "b"], true, &options) {
            Plan::Pairs(pairs) => assert_eq!(pairs[0].destination, PathBuf::from("b")),
            other => panic!("expected pairs, got {:?}", other),
        }
    }

    #[test]
    fn no_target_directory_refuses_two_sources() {
        let options = Options { no_target_directory: true, ..opts() };
        assert_eq!(
            plan_of(&["a", "b", "c"], true, &options),
            Plan::Invalid(Problem::ExtraOperand("c".to_string()))
        );
    }

    #[test]
    fn no_operand_is_invalid() {
        assert_eq!(plan_of(&[], false, &opts()), Plan::Invalid(Problem::MissingOperand));
    }

    #[test]
    fn one_operand_without_directory_is_invalid() {
        assert_eq!(plan_of(&["a"], false, &opts()), Plan::Invalid(Problem::MissingOperand));
    }

    #[test]
    fn no_source_with_a_directory_copies_stdin() {
        let options = Options { target_directory: Some("target".into()), ..opts() };
        assert_eq!(plan_of(&[], true, &options), Plan::Stdin(PathBuf::from("target")));
    }

    #[test]
    fn parents_keeps_the_whole_source_name() {
        let options = Options {
            parents: true,
            target_directory: Some("target".into()),
            ..opts()
        };
        match plan_of(&["a/b/c"], false, &options) {
            Plan::Pairs(pairs) => assert_eq!(pairs[0].destination, PathBuf::from("target/a/b/c")),
            other => panic!("expected pairs, got {:?}", other),
        }
    }

    #[test]
    fn parents_drops_the_leading_slash_of_an_absolute_source() {
        let options = Options {
            parents: true,
            target_directory: Some("target".into()),
            ..opts()
        };
        match plan_of(&["/a/b"], false, &options) {
            Plan::Pairs(pairs) => assert_eq!(pairs[0].destination, PathBuf::from("target/a/b")),
            other => panic!("expected pairs, got {:?}", other),
        }
    }

    #[test]
    fn strip_trailing_slashes_is_opt_in() {
        let options = opts();
        assert_eq!(
            normalize_source("dir/", &options),
            PathBuf::from("dir/")
        );
        let options = Options { strip_trailing_slashes: true, ..opts() };
        assert_eq!(normalize_source("dir/", &options), PathBuf::from("dir"));
    }

    #[test]
    fn archive_expands_to_d_r_preserve_all() {
        let options = Options { archive: true, ..opts() }.effective();
        assert!(options.recursive);
        assert_eq!(options.symlinks, SymlinkPolicy::no_follow());
        assert_eq!(options.preserve, Attributes::ALL);
    }

    #[test]
    fn no_clobber_beats_update_and_interactive() {
        // -n is --update=none: it overrides -u and a previous -i, and -f is
        // ignored when -n is used.
        let options = Options {
            interactive: true,
            update: Update::Older,
            force: true,
            ..opts()
        };
        let effective = options.with_update_none().effective();
        assert!(!effective.interactive);
        assert!(!effective.force);
        assert_eq!(effective.update, Update::None);
    }

    #[test]
    fn interactive_beats_no_clobber() {
        // -i overrides a previous -n, so the last one on the command line wins;
        // modelling that as "interactive set after update" is enough here.
        let options = Options { interactive: true, ..opts() };
        let effective = options.effective();
        assert_eq!(effective.update, Update::All);
    }

    #[test]
    fn attribute_lists_parse() {
        assert_eq!(Attributes::parse("").unwrap(), Attributes::ALL);
        assert_eq!(Attributes::parse("all").unwrap(), Attributes::ALL);
        let subset = Attributes::parse("mode,timestamps").unwrap();
        assert!(subset.mode && subset.timestamps && !subset.ownership);
        assert_eq!(Attributes::parse("bogus").unwrap_err(), "bogus");
    }

    #[test]
    fn no_preserve_removes_attributes() {
        let all = Attributes::ALL;
        let reduced = all.without("timestamps").unwrap();
        assert!(reduced.mode && !reduced.timestamps);
    }

    #[test]
    fn when_values_parse() {
        assert_eq!(When::parse("").unwrap(), When::Always);
        assert_eq!(When::parse("never").unwrap(), When::Never);
        assert_eq!(When::parse("auto").unwrap(), When::Auto);
        assert!(When::parse("maybe").is_err());
    }

    #[test]
    fn update_values_parse() {
        assert_eq!(Update::parse("all").unwrap(), Update::All);
        assert_eq!(Update::parse("none").unwrap(), Update::None);
        assert_eq!(Update::parse("older").unwrap(), Update::Older);
        assert!(Update::parse("newer").is_err());
    }

    #[test]
    fn directory_without_recursion_is_refused() {
        assert_eq!(
            decide(true, false, false, false, None, None, &opts()),
            Action::RefuseDirectory
        );
    }

    #[test]
    fn directory_with_recursion_is_copied() {
        let options = Options { recursive: true, ..opts() };
        assert_eq!(decide(true, false, false, false, None, None, &options), Action::Copy);
    }

    #[test]
    fn directory_over_a_file_is_refused() {
        let options = Options { recursive: true, ..opts() };
        assert_eq!(
            decide(true, false, true, false, None, None, &options),
            Action::DestinationIsFile
        );
    }

    #[test]
    fn file_over_a_directory_is_refused() {
        assert_eq!(
            decide(false, false, true, true, None, None, &opts()),
            Action::DestinationIsFile
        );
    }

    #[test]
    fn no_clobber_skips_an_existing_destination() {
        let options = Options { update: Update::None, ..opts() };
        assert_eq!(decide(false, false, true, false, None, None, &options), Action::Skip);
    }

    #[test]
    fn update_older_skips_a_newer_destination() {
        let options = Options { update: Update::Older, ..opts() };
        assert_eq!(
            decide(false, false, true, false, Some((100, 0)), Some((200, 0)), &options),
            Action::SkipNotNewer
        );
        assert_eq!(
            decide(false, false, true, false, Some((300, 0)), Some((200, 0)), &options),
            Action::Copy
        );
    }

    #[test]
    fn update_all_replaces_whatever_is_there() {
        assert_eq!(
            decide(false, false, true, false, Some((100, 0)), Some((200, 0)), &opts()),
            Action::Copy
        );
    }

    #[test]
    fn attributes_only_does_not_copy_data() {
        let options = Options { attributes_only: true, ..opts() };
        assert_eq!(decide(false, false, false, false, None, None, &options), Action::AttributesOnly);
    }

    #[test]
    fn symbolic_link_option_wins_over_copying() {
        let options = Options { symbolic: true, ..opts() };
        assert_eq!(decide(false, false, false, false, None, None, &options), Action::Symlink);
    }

    #[test]
    fn link_option_makes_a_hard_link() {
        let options = Options { hard_link: true, ..opts() };
        assert_eq!(decide(false, false, false, false, None, None, &options), Action::HardLink);
        assert_eq!(decide(true, false, false, false, None, None, &options), Action::RefuseDirectory);
    }

    #[test]
    fn command_line_symlink_following_follows_the_manual() {
        // Default: a symlink named on the command line is followed.
        assert!(opts().follows_command_line_symlink());
        // -P/-d/-a: not followed.
        let no_deref = Options { symlinks: SymlinkPolicy::no_follow(), ..opts() };
        assert!(!no_deref.follows_command_line_symlink());
        // -H: followed again.
        let follow = Options { symlinks: SymlinkPolicy::follow_command_line(), ..opts() };
        assert!(follow.follows_command_line_symlink());
        // Inside a tree only -L follows, which is the other half of the rule:
        // the default copies such a link as a link.
        assert!(!opts().follows_tree_symlink());
        assert!(!no_deref.follows_tree_symlink());
        assert!(!follow.follows_tree_symlink());
        assert!(Options { symlinks: SymlinkPolicy::follow_all(), ..opts() }.follows_tree_symlink());
    }

    #[test]
    fn links_in_a_tree_are_copied_as_links_unless_dereference() {
        let no_deref = Options { symlinks: SymlinkPolicy::no_follow(), ..opts() };
        assert!(treat_source_as_link(true, false, &no_deref));
        // GNU copies a symlink met inside a tree as a link by default.
        assert!(treat_source_as_link(true, false, &opts()));
        let deref = Options { symlinks: SymlinkPolicy::follow_all(), ..opts() };
        assert!(!treat_source_as_link(true, false, &deref));
        // On the command line, -P keeps the link while the default follows it.
        assert!(treat_source_as_link(true, true, &no_deref));
        assert!(!treat_source_as_link(true, true, &opts()));
    }

    #[test]
    fn backup_names_use_the_suffix() {
        assert_eq!(backup_name("b", &opts(), None), "b~");
        assert_eq!(backup_name("b", &opts(), Some(".bak")), "b.bak");
        let options = Options { suffix: ".old".into(), ..opts() };
        assert_eq!(backup_name("b", &options, Some(".bak")), "b.old");
    }

    #[test]
    fn backup_can_be_switched_off_again() {
        let options = Options { backup: Some("none".into()), ..opts() };
        assert!(!wants_backup(&options, None));
        assert!(wants_backup(&opts(), Some("simple")));
    }

    #[test]
    fn modes_match_what_the_manual_implies() {
        assert_eq!(plain_mode(0o022), 0o644);
        assert_eq!(preserved_mode(0o4755), 0o4755);
    }

    #[test]
    fn the_target_directory_message_names_the_directory() {
        assert_eq!(
            target_directory_message("afile", "Not a directory"),
            "cp: target directory 'afile': Not a directory"
        );
    }

    #[test]
    fn messages_match_coreutils() {
        assert_eq!(
            refuse_directory_message("dir"),
            "cp: -r not specified; omitting directory 'dir'"
        );
        assert_eq!(not_a_directory_message("out"), "cp: target 'out' is not a directory");
        assert_eq!(
            target_unavailable_message("out", "No such file or directory"),
            "cp: target 'out': No such file or directory"
        );
        assert_eq!(extra_operand_message("c"), "cp: extra operand 'c'");
        assert_eq!(
            parents_needs_directory_message(),
            "cp: with --parents, the destination must be a directory"
        );
        assert_eq!(try_help_message(), "Try 'cp --help' for more information.");
        assert_eq!(
            NO_CLOBBER_WARNING,
            "cp: warning: behavior of -n is non-portable and may change in future; use --update=none instead"
        );
        assert_eq!(
            cannot_overwrite_directory_message("d"),
            "cp: cannot overwrite directory 'd' with non-directory"
        );
        assert_eq!(
            same_file_message("a", "a"),
            "cp: 'a' and 'a' are the same file"
        );
        assert_eq!(stat_error("x", "No such file or directory"),
            "cp: cannot stat 'x': No such file or directory");
        assert_eq!(prompt_message("b"), "cp: overwrite 'b'? ");
        assert_eq!(
            invalid_update_message("newer"),
            "cp: invalid argument 'newer' for '--update' option"
        );
    }

    #[test]
    fn base_name_keeps_dot_and_dotdot() {
        assert_eq!(base_name(Path::new(".")), PathBuf::from("."));
        assert_eq!(base_name(Path::new("..")), PathBuf::from(".."));
        assert_eq!(base_name(Path::new("/")), PathBuf::from("/"));
        assert_eq!(base_name(Path::new("/a/b/")), PathBuf::from("b"));
    }

    #[test]
    fn verbose_text_is_source_arrow_destination() {
        let pair = Pair {
            source: PathBuf::from("a"),
            destination: PathBuf::from("b"),
            spelled: "a".to_string(),
        };
        assert_eq!(pair.verbose_line(), "'a' -> 'b'");
    }
}