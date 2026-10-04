//! Path logic of `mkdir(1)`: which ancestors `-p` has to create first.

/// Options that change what has to happen for one path.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    pub parents: bool,
    pub verbose: bool,
    /// An existing directory is not an error with -p.
    pub tolerate_existing: bool,
}

/// Every component of `path`, i.e. what has to exist for the leaf to exist.
/// `/` yields an empty list; `a/b` yields `["a", "a/b"]`.
pub fn components(path: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let absolute = path.starts_with('/');
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(segment);
        parts.push(if absolute { format!("/{}", current) } else { current.clone() });
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_ancestors_in_order() {
        assert_eq!(components("a/b/c"), vec!["a", "a/b", "a/b/c"]);
        assert_eq!(components("/a/b"), vec!["/a", "/a/b"]);
    }

    #[test]
    fn root_has_no_components() {
        assert!(components("/").is_empty());
    }

    #[test]
    fn dot_and_repeated_slashes_are_ignored() {
        assert_eq!(components("./a//b/"), vec!["a", "a/b"]);
    }
}
