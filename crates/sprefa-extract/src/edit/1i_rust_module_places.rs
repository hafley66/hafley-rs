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

    /// Spell an out-of-line declaration against the Names module directory.
    /// `declarer` is its pre-edit file; `lands_at` is that file after the batch.
    /// Preserve other attributes and the written visibility while re-aiming or
    /// removing `#[path]`. A rename changes the identifier and keeps the file.
    pub fn declaration(
        &self,
        declarer: &str,
        lands_at: &str,
        chain: &[String],
        name: &str,
        target: &str,
        written: &str,
    ) -> Option<String> {
        use crate::lang::rust::syn_span;
        use crate::move_cx::relative_between;
        use syn::spanned::Spanned;

        let module: syn::ItemMod = syn::parse_str(written).ok()?;
        if module.content.is_some() {
            return None;
        }
        let directory = self.directory(declarer)?;
        let old_file = Path::new(declarer);
        let new_file = Path::new(lands_at);
        let old_parent = old_file.parent()?.to_str()?;
        let new_parent = new_file.parent()?.to_str()?;
        let directory = if directory == old_file.with_extension("").to_str()? {
            new_file.with_extension("").to_str()?.to_string()
        } else {
            let suffix = Path::new(&directory).strip_prefix(old_parent).ok()?;
            Path::new(new_parent).join(suffix).to_str()?.to_string()
        };
        let base = chain
            .iter()
            .fold(PathBuf::from(directory), |dir, block| dir.join(block));
        let natural = [
            base.join(format!("{}.rs", name.trim_start_matches("r#"))),
            base.join(name.trim_start_matches("r#")).join("mod.rs"),
        ]
        .contains(&PathBuf::from(target));
        let aim = (!natural).then(|| {
            let base = if chain.is_empty() {
                Path::new(new_parent)
            } else {
                base.as_path()
            };
            format!("{:?}", relative_between(base.to_str().unwrap(), target))
        });
        let mut edits = Vec::new();
        let ident = syn_span(module.ident.span());
        edits.push((ident.start as usize, ident.end() as usize, name.to_string()));
        let attr = module
            .attrs
            .iter()
            .find(|attr| attr.path().is_ident("path"));
        match (attr, &aim) {
            (Some(attr), Some(aim)) => {
                let syn::Meta::NameValue(pair) = &attr.meta else {
                    return None;
                };
                let span = syn_span(pair.value.span());
                edits.push((span.start as usize, span.end() as usize, aim.clone()));
            }
            (Some(attr), None) => {
                let span = syn_span(attr.span());
                let end = written[span.end() as usize..]
                    .char_indices()
                    .find(|(_, ch)| !ch.is_whitespace())
                    .map_or(written.len(), |(at, _)| span.end() as usize + at);
                edits.push((span.start as usize, end, String::new()));
            }
            _ => {}
        }
        edits.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
        let mut declaration = written.to_string();
        for (start, end, text) in edits {
            declaration.replace_range(start..end, &text);
        }
        if attr.is_none() {
            if let Some(aim) = aim {
                declaration = format!("#[path = {aim}] {declaration}");
            }
        }
        Some(declaration)
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
