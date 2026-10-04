//! Path trimming of `dirname(1)`: everything up to the last slash.

/// GNU rules: `/` -> `/`, `a` -> `.`, `/a` -> `/`, `a/b` -> `a`.
pub fn dirname(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        // "/" and "//" are their own dirname.
        return "/".to_string();
    }
    match trimmed.rfind('/') {
        None => ".".to_string(),
        Some(0) => "/".to_string(),
        Some(idx) => trimmed[..idx].to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_cases() {
        assert_eq!(dirname("/usr/bin/sort"), "/usr/bin");
        assert_eq!(dirname("sort"), ".");
        assert_eq!(dirname("/sort"), "/");
        assert_eq!(dirname("/"), "/");
    }

    #[test]
    fn trailing_slashes() {
        assert_eq!(dirname("/usr/bin/"), "/usr");
        assert_eq!(dirname("a/b/"), "a");
    }

    #[test]
    fn relative_double() {
        assert_eq!(dirname("a/b/c"), "a/b");
    }

    #[test]
    fn empty_is_a_dot() {
        assert_eq!(dirname(""), ".");
    }
}
