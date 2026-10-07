//! The PATH of a `PATH#NAME` graph anchor against a fact path: one rule for every arm.

use std::path::{Component, Path};

/// Check declarations independently of edges, including declarations with no
/// callers or outgoing edges. Candidates keep the exact name and change no seed.
pub(super) fn seed_unmatched(
    files: &[std::path::PathBuf],
    seed: &str,
) -> Result<Option<sprefa_extract::FlatFact>, Box<dyn std::error::Error>> {
    let (path, name) = match seed.split_once('#') {
        Some((path, name)) => (Some(Path::new(path)), name),
        None => (None, seed),
    };
    let mask = sprefa_extract::FamilyMask {
        call: true,
        types: true,
        ..sprefa_extract::FamilyMask::NONE
    };
    let mut candidates = std::collections::BTreeSet::new();
    for file in files {
        let supplied = file.to_string_lossy();
        let content = std::fs::read(sprefa_extract::io_path(file))?;
        let Some(output) = sprefa_extract::dispatch::dispatch_uncached(&supplied, &content, mask)
        else {
            continue;
        };
        let mut names = output
            .call
            .iter()
            .flat_map(|bundle| &bundle.nodes)
            .filter_map(|node| node.name)
            .chain(
                output
                    .types
                    .iter()
                    .flat_map(|bundle| &bundle.nodes)
                    .filter_map(|node| node.name),
            );
        if names.any(|id| output.strings.lookup(id) == name) {
            if path.is_none_or(|path| anchor_path_matches(path, &supplied)) {
                return Ok(None);
            }
            candidates.insert(format!("{supplied}#{name}"));
        }
    }
    crate::ops::print_diagnostic(format_args!("seed {seed}: no_declaration"));
    for candidate in candidates.iter().take(5) {
        crate::ops::print_diagnostic(format_args!("candidate: {candidate}"));
    }
    Ok(Some(sprefa_extract::FlatFact::SeedUnmatched {
        seed: seed.to_string(),
        reason: "no_declaration".to_string(),
    }))
}

/// The start of the one type declaration named NAME in FILE; none or several give None.
pub(super) fn type_decl_start(file: &Path, name: &str) -> Option<u32> {
    let content = std::fs::read(file).ok()?;
    let mask = sprefa_extract::FamilyMask { types: true, ..sprefa_extract::FamilyMask::NONE };
    let output = sprefa_extract::dispatch::dispatch_uncached(&file.to_string_lossy(), &content, mask)?;
    let mut starts = output
        .types
        .iter()
        .flat_map(|bundle| &bundle.nodes)
        .filter(|node| node.name.is_some_and(|id| output.strings.lookup(id) == name))
        .map(|node| node.span.start);
    let start = starts.next()?;
    starts.all(|other| other == start).then_some(start)
}

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
