//! Edit projections of the shared module provider, including proposed file homes.

use super::rust_module_tree::ModulePlace;
use hafley_scm::read::lang::rust_names_index::RustNamesIndex;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub struct RustModulePlaces {
    pub roots: BTreeSet<String>,
    pub places: Vec<ModulePlace>,
    pub provider: Result<RustNamesIndex, String>,
    root: PathBuf,
}

impl std::ops::Deref for RustModulePlaces {
    type Target = BTreeSet<String>;
    fn deref(&self) -> &Self::Target {
        &self.roots
    }
}

impl RustModulePlaces {
    pub fn open(root: &Path, texts: &[(PathBuf, String)]) -> Self {
        let provider = RustNamesIndex::open(texts);
        #[cfg(feature = "rust-checker")]
        let places = provider
            .as_ref()
            .map(RustNamesIndex::all_places)
            .unwrap_or_default();
        #[cfg(not(feature = "rust-checker"))]
        let places = Vec::new();
        let mut result = Self {
            root: root.to_path_buf(),
            roots: BTreeSet::new(),
            places,
            provider,
        };
        result.roots = result
            .places
            .iter()
            .map(|place| result.rel(&place.crate_root))
            .collect();
        result
    }

    pub fn rel(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    pub fn home(&self, rel: &str) -> Option<(String, Vec<String>)> {
        let path = self.root.join(rel);
        let homes = self.provider.as_ref().ok()?.homes(&path.to_string_lossy());
        let mut homes = match homes {
            Ok(homes) => homes
                .into_iter()
                .map(|(root, path)| (self.rel(&root), path))
                .collect::<Vec<_>>(),
            Err(_) => {
                // A destination chooses an existing module directory and a new
                // declaration name. Existing module placement is always an RA answer.
                let directory = path.parent()?;
                let mut name = path.file_stem()?.to_str()?.to_string();
                let directory = if name == "mod" {
                    name = directory.file_name()?.to_str()?.to_string();
                    directory.parent()?
                } else {
                    directory
                };
                self.places
                    .iter()
                    .filter(|place| place.directory == directory)
                    .map(|place| {
                        let mut module = place.path.clone();
                        module.push(name.clone());
                        (self.rel(&place.crate_root), module)
                    })
                    .collect()
            }
        };
        homes.sort();
        homes.dedup();
        match homes.as_slice() {
            [home] => Some(home.clone()),
            _ => None,
        }
    }

    pub fn directory(&self, rel: &str) -> Option<String> {
        let file = self.root.join(rel);
        let mut directories: Vec<_> = self
            .places
            .iter()
            .filter(|place| place.file == file)
            .filter(|place| {
                self.provider
                    .as_ref()
                    .ok()
                    .and_then(|provider| provider.homes(&file.to_string_lossy()).ok())
                    .is_some_and(|homes| {
                        homes.contains(&(place.crate_root.clone(), place.path.clone()))
                    })
            })
            .map(|place| self.rel(&place.directory))
            .collect();
        directories.sort();
        directories.dedup();
        match directories.as_slice() {
            [directory] => Some(directory.clone()),
            _ => None,
        }
    }

    pub fn files(&self, root: &str, module: &[String]) -> Vec<String> {
        let root = self.root.join(root);
        let mut files: Vec<_> = self
            .places
            .iter()
            .filter(|place| place.crate_root == root && place.path == module)
            .map(|place| self.rel(&place.file))
            .collect();
        files.sort();
        files.dedup();
        files
    }

    pub fn resolve(
        &self,
        root: &str,
        module: &[String],
        chain: &[String],
        prefix: &[String],
    ) -> Option<(String, Vec<String>)> {
        let places = self
            .provider
            .as_ref()
            .ok()?
            .resolve_module(&self.root.join(root), module, chain, prefix)
            .ok()?;
        let [place] = places.as_slice() else {
            return None;
        };
        Some((self.rel(&place.0), place.1.clone()))
    }

    pub fn declared(&self, file: &str, chain: &[String], name: &str) -> Option<String> {
        let (root, module) = self.home(file)?;
        let (root, module) = self.resolve(&root, &module, chain, &[name.to_string()])?;
        let files = self.files(&root, &module);
        let [file] = files.as_slice() else {
            return None;
        };
        Some(file.clone())
    }
}
