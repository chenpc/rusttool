//! Path logic of `rmdir(1)`: `-p` also removes the parents that become empty.

/// What should happen to one operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Remove,
    /// Not empty and --ignore-fail-on-non-empty was given.
    IgnoreNotEmpty,
}

/// The paths `-p` would remove for `path`, leaf first.
pub fn chain(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = path.trim_end_matches('/').to_string();
    while !current.is_empty() && current != "/" && current != "." {
        out.push(current.clone());
        match current.rfind('/') {
            Some(0) => break,
            Some(idx) => current.truncate(idx),
            None => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_is_leaf_first() {
        assert_eq!(chain("a/b/c"), vec!["a/b/c", "a/b", "a"]);
    }

    #[test]
    fn chain_stops_at_root() {
        assert_eq!(chain("/a"), vec!["/a"]);
    }

    #[test]
    fn trailing_slash_is_ignored() {
        assert_eq!(chain("a/b/"), vec!["a/b", "a"]);
    }
}
