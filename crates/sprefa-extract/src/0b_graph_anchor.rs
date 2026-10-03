//! The PATH of a `PATH#NAME` graph anchor against a fact path: one rule for every arm.

use std::path::{Component, Path};

/// An absolute PATH names one file: equal after resolving both. A relative PATH
/// keeps facts whose path ends with it by whole components; `./` steps drop first.
pub(super) fn anchor_path_matches(anchor: &Path, fact: &str) -> bool {
    let fact = Path::new(fact);
    if anchor.is_absolute() {
        let resolve = |path: &Path| std::fs::canonicalize(path).or_else(|_| std::path::absolute(path));
        return matches!((resolve(anchor), resolve(fact)), (Ok(left), Ok(right)) if left == right);
    }
    let suffix: std::path::PathBuf = anchor
        .components()
        .filter(|component| !matches!(component, Component::CurDir))
        .collect();
    !suffix.as_os_str().is_empty() && fact.ends_with(&suffix)
}

#[cfg(test)]
mod tests {
    use super::anchor_path_matches;
    use std::path::Path;

    #[test]
    fn relative_forms_match_by_suffix_and_absolute_by_file() {
        let fact = "crates/x/src/atoms.rs";
        let cwd = std::env::current_dir().unwrap();
        let rows: Vec<(String, bool)> = [
            "crates/x/src/atoms.rs",
            "./crates/x/src/atoms.rs",
            "src/atoms.rs",
            "atoms.rs",
            "crates/x/src/atoms.rs/",
            "toms.rs",
            "other/atoms.rs",
            ".",
        ]
        .into_iter()
        .map(|anchor| (anchor.to_string(), anchor_path_matches(Path::new(anchor), fact)))
        .chain([(
            "<cwd>/crates/x/src/atoms.rs".to_string(),
            anchor_path_matches(&cwd.join(fact), fact),
        )])
        .collect();
        assert_eq!(
            rows,
            [
                ("crates/x/src/atoms.rs", true),
                ("./crates/x/src/atoms.rs", true),
                ("src/atoms.rs", true),
                ("atoms.rs", true),
                ("crates/x/src/atoms.rs/", true),
                ("toms.rs", false),
                ("other/atoms.rs", false),
                (".", false),
                ("<cwd>/crates/x/src/atoms.rs", true),
            ]
            .map(|(anchor, keep)| (anchor.to_string(), keep))
        );
    }
}
