//! Path matching utilities for probe-rust.
//!
//! Provides utilities for matching file paths with fuzzy/flexible
//! matching strategies, essential because different tools may report
//! paths in different formats.

use std::path::Path;

/// Extract the "src/..." suffix from a path for normalized matching.
pub fn extract_src_suffix(path: &str) -> &str {
    if let Some(pos) = path.find("/src/") {
        return &path[pos + 1..];
    }
    path
}

/// Check if two paths match using suffix comparison.
pub fn paths_match_by_suffix(path1: &str, path2: &str) -> bool {
    path1.ends_with(path2) || path2.ends_with(path1)
}

/// Match score for path comparison (higher is better).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PathMatchScore {
    None = 0,
    FilenameOnly = 1,
    Suffix = 2,
    Exact = 3,
}

/// Calculate the match score between two paths.
pub fn calculate_path_match_score(query: &str, candidate: &str) -> PathMatchScore {
    let query_path = Path::new(query);
    let candidate_path = Path::new(candidate);

    if query_path == candidate_path {
        return PathMatchScore::Exact;
    }

    if paths_match_by_suffix(query, candidate) {
        return PathMatchScore::Suffix;
    }

    if query_path.file_name() == candidate_path.file_name() {
        return PathMatchScore::FilenameOnly;
    }

    PathMatchScore::None
}

/// A helper for efficiently looking up paths from a known set.
#[derive(Debug, Clone)]
pub struct PathMatcher {
    known_paths: Vec<String>,
}

impl PathMatcher {
    pub fn new(paths: Vec<String>) -> Self {
        Self { known_paths: paths }
    }

    /// Find the best matching known path for the given query.
    pub fn find_best_match(&self, query: &str) -> Option<&String> {
        let mut best_match: Option<&String> = None;
        let mut best_score = PathMatchScore::None;

        for candidate in &self.known_paths {
            let score = calculate_path_match_score(query, candidate);

            if score == PathMatchScore::Exact {
                return Some(candidate);
            }

            if score > best_score {
                best_match = Some(candidate);
                best_score = score;
            }
        }

        if best_score > PathMatchScore::None {
            best_match
        } else {
            None
        }
    }

    pub fn known_paths(&self) -> &[String] {
        &self.known_paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_src_suffix() {
        assert_eq!(
            extract_src_suffix("/home/user/project/src/lib.rs"),
            "src/lib.rs"
        );
        assert_eq!(extract_src_suffix("src/lib.rs"), "src/lib.rs");
        assert_eq!(extract_src_suffix("lib.rs"), "lib.rs");
    }

    #[test]
    fn test_paths_match_by_suffix() {
        assert!(paths_match_by_suffix("/project/src/lib.rs", "src/lib.rs"));
        assert!(paths_match_by_suffix("src/lib.rs", "/project/src/lib.rs"));
        assert!(!paths_match_by_suffix("/project/src/lib.rs", "src/main.rs"));
    }

    #[test]
    fn test_calculate_path_match_score() {
        assert_eq!(
            calculate_path_match_score("src/lib.rs", "src/lib.rs"),
            PathMatchScore::Exact
        );
        assert_eq!(
            calculate_path_match_score("/project/src/lib.rs", "src/lib.rs"),
            PathMatchScore::Suffix
        );
        assert_eq!(
            calculate_path_match_score("/other/lib.rs", "src/lib.rs"),
            PathMatchScore::FilenameOnly
        );
        assert_eq!(
            calculate_path_match_score("/other/main.rs", "src/lib.rs"),
            PathMatchScore::None
        );
    }

    #[test]
    fn test_path_matcher() {
        let paths = vec![
            "src/lemmas/field_lemmas/constants_lemmas.rs".to_string(),
            "src/lemmas/edwards_lemmas/constants_lemmas.rs".to_string(),
        ];
        let matcher = PathMatcher::new(paths);

        let result = matcher.find_best_match("src/lemmas/edwards_lemmas/constants_lemmas.rs");
        assert_eq!(
            result,
            Some(&"src/lemmas/edwards_lemmas/constants_lemmas.rs".to_string())
        );

        let result = matcher.find_best_match("edwards_lemmas/constants_lemmas.rs");
        assert_eq!(
            result,
            Some(&"src/lemmas/edwards_lemmas/constants_lemmas.rs".to_string())
        );

        let result = matcher.find_best_match("constants_lemmas.rs");
        assert!(result.is_some());
    }
}
