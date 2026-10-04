//! Path trimming of `basename(1)`: strip directories, then any suffix.

/// Remove trailing slashes, then take the last component.
pub fn strip_directories(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        // "/" and "//" have basename "/"; "a//" is "a".
        return if path.is_empty() { String::new() } else { "/".to_string() };
    }
    match trimmed.rfind('/') {
        Some(idx) => trimmed[idx + 1..].to_string(),
        None => trimmed.to_string(),
    }
}

/// Remove `suffix` from the end, but only if what remains is not empty and the
/// base is not a dotfile that would become empty.
pub fn strip_suffix(base: &str, suffix: &str) -> String {
    if suffix.is_empty() || base == suffix {
        return base.to_string();
    }
    match base.strip_suffix(suffix) {
        Some(rest) if !rest.is_empty() => rest.to_string(),
        _ => base.to_string(),
    }
}

/// `basename path [suffix]`.
pub fn basename(path: &str, suffix: Option<&str>) -> String {
    let base = strip_directories(path);
    match suffix {
        Some(suffix) => strip_suffix(&base, suffix),
        None => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_directories() {
        assert_eq!(basename("/usr/bin/sort", None), "sort");
        assert_eq!(basename("sort", None), "sort");
    }

    #[test]
    fn trailing_slashes_are_ignored() {
        assert_eq!(basename("/usr/bin/", None), "bin");
        assert_eq!(basename("/", None), "/");
    }

    #[test]
    fn strips_suffix() {
        assert_eq!(basename("/usr/bin/sort.exe", Some(".exe")), "sort");
        assert_eq!(basename("/usr/bin/sort", Some(".tar.gz")), "sort");
    }

    #[test]
    fn suffix_must_actually_match() {
        assert_eq!(basename("/usr/bin/sort", Some(".txt")), "sort");
    }

    #[test]
    fn never_empties_the_name() {
        assert_eq!(strip_suffix(".bashrc", ".bashrc"), ".bashrc");
        assert_eq!(strip_suffix("x", "x"), "x");
    }
}
