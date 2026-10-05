//! The Names provider beside body_edges. Module def maps only, no inference.

use super::modules::{host_path, vfs_path, ModulePlace, RustModuleTree};
use super::*;
use ra_ap_hir::{Module, ScopeDef};

pub type NamesHost = RustModuleTree;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Abstain {
    OutsideWorkspace,
    NeedsTypes,
    UnresolvedPath,
}

impl std::fmt::Display for Abstain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::OutsideWorkspace => "outside_workspace",
            Self::NeedsTypes => "needs_types",
            Self::UnresolvedPath => "unresolved_path",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DefPlace {
    pub file: PathBuf,
    pub name: String,
    pub start: u32,
    pub end: u32,
    pub module: bool,
}

pub fn module_places(host: &NamesHost, file: &Path) -> Result<Vec<ModulePlace>, Abstain> {
    let places = host.places(file);
    if places.is_empty() {
        Err(Abstain::OutsideWorkspace)
    } else {
        Ok(places)
    }
}

/// Resolve from every target that includes the file. A target that does not
/// define this spelling contributes no destination; equal coordinates dedupe.
pub fn resolve_path(
    host: &NamesHost,
    file: &Path,
    path: &[String],
) -> Result<Vec<DefPlace>, Abstain> {
    let workspace = host.workspace.lock().unwrap();
    let Some((id, _)) = workspace.vfs.file_id(&vfs_path(&host_path(file))) else {
        return Err(Abstain::OutsideWorkspace);
    };
    let db = workspace.host.raw_database();
    attach_db(db, || {
        let sema = Semantics::new(db);
        let modules: Vec<_> = sema
            .file_to_module_defs(ra_ap_ide::FileId::from_raw(id.index()))
            .collect();
        if modules.is_empty() {
            return Err(Abstain::OutsideWorkspace);
        }
        let mut places = Vec::new();
        for module in modules {
            for def in resolve_module_path(db, module, path) {
                let Some(nav) = def.try_to_nav(&sema).map(|nav| nav.call_site) else {
                    continue;
                };
                let source = match def {
                    ModuleDef::Module(module) => {
                        match module.definition_source_file_id(db).file_id() {
                            Some(file) => file.file_id(db),
                            None => continue,
                        }
                    }
                    _ => nav.file_id,
                };
                let file = workspace
                    .vfs
                    .file_path(ra_ap_vfs::FileId::from_raw(source.index()));
                let Some(file) = file.as_path() else { continue };
                let file = PathBuf::from(file.to_string());
                let Some(krate) = def.module(db).map(|module| module.krate(db)) else {
                    continue;
                };
                let crate_file = workspace
                    .vfs
                    .file_path(ra_ap_vfs::FileId::from_raw(krate.root_file(db).index()));
                if !crate_file.as_path().is_some_and(|path| {
                    host.crate_roots
                        .contains_key(&PathBuf::from(path.to_string()))
                }) {
                    continue;
                }
                let range = nav.focus_range.unwrap_or(nav.full_range);
                places.push(DefPlace {
                    file,
                    name: nav.name.to_string(),
                    start: range.start().into(),
                    end: range.end().into(),
                    module: matches!(def, ModuleDef::Module(_)),
                });
            }
        }
        places.sort();
        places.dedup();
        if places.is_empty() {
            Err(Abstain::UnresolvedPath)
        } else {
            Ok(places)
        }
    })
}

/// The engine's module resolver handles imports, glob ambiguity, visibility,
/// dependency renames and re-exports. Only explicit root keywords select the
/// starting module before asking that resolver.
fn resolve_module_path(
    db: &RootDatabase,
    mut module: Module,
    mut path: &[String],
) -> Vec<ModuleDef> {
    match path.first().map(String::as_str) {
        Some("crate") => {
            module = module.krate(db).root_module(db);
            path = &path[1..];
        }
        Some("self") => {
            path = &path[1..];
        }
        Some("super") => {
            while path.first().map(String::as_str) == Some("super") {
                let Some(parent) = module.parent(db) else {
                    return Vec::new();
                };
                module = parent;
                path = &path[1..];
            }
        }
        _ => {}
    }
    if path.is_empty() {
        return vec![ModuleDef::Module(module)];
    }
    module
        .resolve_mod_path(db, path.iter().map(|name| ra_ap_hir::Name::new_root(name)))
        .into_iter()
        .flatten()
        .map(|item| item.into_module_def())
        .collect()
}

/// Every name available in a file's module scopes. Destinations come from the
/// same resolver used for explicit paths.
pub fn scope_names(host: &NamesHost, file: &Path) -> Result<Vec<String>, Abstain> {
    let workspace = host.workspace.lock().unwrap();
    let Some((id, _)) = workspace.vfs.file_id(&vfs_path(&host_path(file))) else {
        return Err(Abstain::OutsideWorkspace);
    };
    let db = workspace.host.raw_database();
    attach_db(db, || {
        let sema = Semantics::new(db);
        let modules: Vec<_> = sema
            .file_to_module_defs(ra_ap_ide::FileId::from_raw(id.index()))
            .collect();
        if modules.is_empty() {
            return Err(Abstain::OutsideWorkspace);
        }
        let mut names: Vec<_> = modules
            .into_iter()
            .flat_map(|module| module.scope(db, None))
            .filter_map(|(name, def)| {
                matches!(def, ScopeDef::ModuleDef(_)).then(|| name.as_str().to_string())
            })
            .collect();
        names.sort();
        names.dedup();
        Ok(names)
    })
}

pub fn resolve_method(
    _host: &NamesHost,
    _file: &Path,
    _method: &str,
) -> Result<Vec<DefPlace>, Abstain> {
    Err(Abstain::NeedsTypes)
}

/// Resolve a module prefix from a known place and its inline-module chain.
/// All traversal follows rust-analyzer's children and module path resolver.
pub fn resolve_prefix(
    host: &NamesHost,
    from: &ModulePlace,
    chain: &[String],
    prefix: &[String],
) -> Result<Vec<ModulePlace>, Abstain> {
    let workspace = host.workspace.lock().unwrap();
    let db = workspace.host.raw_database();
    attach_db(db, || {
        let krate = Crate::all(db)
            .into_iter()
            .find(|krate| {
                workspace
                    .vfs
                    .file_path(ra_ap_vfs::FileId::from_raw(krate.root_file(db).index()))
                    .as_path()
                    .is_some_and(|path| PathBuf::from(path.to_string()) == from.crate_root)
            })
            .ok_or(Abstain::OutsideWorkspace)?;
        let mut module = krate.root_module(db);
        for name in from.path.iter().chain(chain) {
            module = module
                .children(db)
                .find(|child| child.name(db).is_some_and(|child| child.as_str() == name))
                .ok_or(Abstain::UnresolvedPath)?;
        }
        let mut places: Vec<_> = resolve_module_path(db, module, prefix)
            .into_iter()
            .filter_map(|def| match def {
                ModuleDef::Module(module) => host.place_of(&workspace, db, module),
                _ => None,
            })
            .collect();
        places.sort();
        places.dedup();
        if places.is_empty() {
            Err(Abstain::UnresolvedPath)
        } else {
            Ok(places)
        }
    })
}
