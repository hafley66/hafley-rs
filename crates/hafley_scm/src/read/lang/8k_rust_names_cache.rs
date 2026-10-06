//! Owned Names answers, retained by the workspace until its VFS changes.
use super::super::rust_checker_session::CheckerWorkspace;
use super::modules::{ModulePlace, RustModuleTree, host_path};
use super::names::{Abstain, DefPlace};
use ra_ap_hir::{Crate, Semantics, attach_db};
use std::collections::HashMap;
use std::path::PathBuf;

pub(super) type Definitions = Result<Vec<DefPlace>, Abstain>;

#[derive(Default)]
pub(crate) struct NamesCache {
    pub all_places: Option<Vec<ModulePlace>>,
    pub places: HashMap<PathBuf, Vec<ModulePlace>>,
    pub declared: HashMap<PathBuf, Vec<ModulePlace>>,
    pub paths: HashMap<(PathBuf, Vec<String>, Option<u32>), Definitions>,
    pub written: HashMap<(PathBuf, u32, String), Definitions>,
    pub dependencies: HashMap<PathBuf, Vec<(String, ModulePlace)>>,
}

/// One module graph query supplies the declaration index and the full inventory.
pub(super) fn ensure_all_places(host: &RustModuleTree, workspace: &mut CheckerWorkspace) {
    if workspace.names.all_places.is_some() {
        return;
    }
    let db = workspace.host.raw_database();
    let places = attach_db(db, || {
        let mut pending = Crate::all(db)
            .into_iter()
            .map(|krate| krate.root_module(db))
            .collect::<Vec<_>>();
        let mut places = Vec::new();
        while let Some(module) = pending.pop() {
            let Some(place) = host.place_of(workspace, db, module) else {
                continue;
            };
            places.push(place);
            pending.extend(module.children(db));
        }
        places.sort();
        places.dedup();
        places
    });
    for place in &places {
        if let Some((parent, _, _)) = &place.decl {
            if parent != &place.file {
                workspace
                    .names
                    .declared
                    .entry(parent.clone())
                    .or_default()
                    .push(place.clone());
            }
        }
    }
    workspace.names.all_places = Some(places);
}

impl RustModuleTree {
    /// Resolve an ordered file batch with one lock, one attached database, and
    /// one Semantics context. Retain include!-aware RA ownership for each file.
    pub fn places_for_files(&self, files: &[PathBuf]) -> Vec<Vec<ModulePlace>> {
        let files = files.iter().map(|file| host_path(file)).collect::<Vec<_>>();
        let mut workspace = self.workspace.lock().unwrap();
        let db = workspace.host.raw_database();
        let missing = attach_db(db, || {
            let sema = Semantics::new(db);
            files
                .iter()
                .filter(|file| !workspace.names.places.contains_key(*file))
                .map(|file| {
                    let mut places = workspace
                        .vfs
                        .file_id(&super::modules::vfs_path(file))
                        .map_or_else(Vec::new, |(id, _)| {
                            sema.file_to_module_defs(ra_ap_ide::FileId::from_raw(id.index()))
                                .filter_map(|module| self.place_of(&workspace, db, module))
                                .collect::<Vec<_>>()
                        });
                    places.sort();
                    (file.clone(), places)
                })
                .collect::<Vec<_>>()
        });
        workspace.names.places.extend(missing);
        files
            .iter()
            .map(|file| workspace.names.places[file].clone())
            .collect()
    }

    pub fn declared_places(&self, file: &std::path::Path) -> Vec<ModulePlace> {
        let mut workspace = self.workspace.lock().unwrap();
        ensure_all_places(self, &mut workspace);
        workspace
            .names
            .declared
            .get(&host_path(file))
            .cloned()
            .unwrap_or_default()
    }
}
