//! Workspace package discovery. `TsResolver` and `TsModuleIndex` both read
//! packages through `discover`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// The packages above a run's paths, keyed by manifest `name`.
#[derive(Default)]
pub(crate) struct Packages {
    /// Manifest `name` -> the real package directory.
    pub by_name: BTreeMap<String, PathBuf>,
    /// Real package directory -> the spelling the run supplied for it.
    pub written: HashMap<PathBuf, PathBuf>,
}

/// Every `package.json` in an ancestor directory of a supplied `(real, written)`
/// path. Paths are visited sorted; the first manifest per name wins.
pub(crate) fn discover<'a>(paths: impl IntoIterator<Item = (&'a Path, &'a Path)>) -> Packages {
    let mut paths: Vec<_> = paths.into_iter().collect();
    paths.sort();
    let mut packages = Packages::default();
    let mut visited = HashSet::new();
    for (real, written) in paths {
        for (directory, spelled) in real.ancestors().skip(1).zip(written.ancestors().skip(1)) {
            if !visited.insert(directory.to_path_buf()) {
                continue;
            }
            let _probe = tracing::trace_span!("ts.packages.directory").entered();
            let Some(manifest) =
                std::fs::File::open(crate::read::io_path(&directory.join("package.json")))
                    .ok()
                    .and_then(|file| serde_json::from_reader::<_, serde_json::Value>(file).ok())
            else {
                continue;
            };
            let Some(name) = manifest.get("name").and_then(serde_json::Value::as_str) else {
                continue;
            };
            packages
                .written
                .insert(directory.to_path_buf(), spelled.to_path_buf());
            packages
                .by_name
                .entry(name.to_string())
                .or_insert_with(|| directory.to_path_buf());
        }
    }
    packages
}

/// The npm package name a bare specifier starts with: one segment, two when scoped.
pub(crate) fn package_name(module: &str) -> Option<&str> {
    let mut parts = module.split('/');
    let first = parts.next().filter(|first| !first.is_empty() && !first.starts_with('.'))?;
    let len = match first.starts_with('@') {
        true => first.len() + 1 + parts.next()?.len(),
        false => first.len(),
    };
    Some(&module[..len])
}
