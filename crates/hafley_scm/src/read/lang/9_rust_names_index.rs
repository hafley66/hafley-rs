//! Corpus joins over the shared Names provider. No layout or receiver resolver.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "rust-checker")]
use super::rust_checker::{module_places, module_tree_for_workspace_files, NamesHost};
use super::rust_module_facts::{
    ImportRow, ModuleCallTarget, ResolvedImport, ResolvedImportKind, RustModuleFacts,
};
use crate::read::types::{ContentId, DefIndex, UnresolvedReason};
use crate::span::Span;

#[derive(Default)]
pub struct RustNamesIndex {
    pub context_failures: Vec<(String, String, String)>,
    receivers: super::fast_receivers::FastReceivers,
    facts: HashMap<String, RustModuleFacts>,
    blobs: HashMap<String, ContentId>,
    paths: HashMap<PathBuf, String>,
    source_paths: HashMap<String, PathBuf>,
    defs: DefIndex,
    collapsed: BTreeSet<(ContentId, Span)>,
    aliases: BTreeSet<(ContentId, Span)>,
    #[cfg(feature = "rust-checker")]
    hosts: HashMap<String, Arc<NamesHost>>,
    #[cfg(feature = "rust-checker")]
    unique_hosts: Vec<Arc<NamesHost>>,
    #[cfg(feature = "rust-checker")]
    workspaces: Vec<Arc<super::rust_workspace::RustWorkspace>>,
}

fn absolute(path: &str) -> PathBuf {
    let path = crate::read::io_path(Path::new(path));
    path.canonicalize().unwrap_or(path)
}

impl RustNamesIndex {
    pub fn build(
        files: Vec<(String, RustModuleFacts)>,
        corpus: &[(String, ContentId)],
        defs: &DefIndex,
    ) -> Self {
        Self::build_in(Path::new(""), files, corpus, defs)
    }

    pub fn build_in(
        root: &Path,
        files: Vec<(String, RustModuleFacts)>,
        corpus: &[(String, ContentId)],
        defs: &DefIndex,
    ) -> Self {
        let absolute = |path: &str| {
            if root.as_os_str().is_empty() {
                absolute(path)
            } else {
                let file = root.join(path);
                file.canonicalize().unwrap_or(file)
            }
        };
        let source_paths = files
            .iter()
            .map(|(path, _)| (path.clone(), absolute(path)))
            .collect();
        let mut index = Self {
            receivers: super::fast_receivers::FastReceivers::current(),
            facts: files.into_iter().collect(),
            blobs: corpus.iter().cloned().collect(),
            paths: corpus
                .iter()
                .map(|(path, _)| (absolute(path), path.clone()))
                .collect(),
            defs: defs.clone(),
            source_paths,
            ..Self::default()
        };
        let mut seen = HashMap::new();
        for (name, sites) in &index.defs.map {
            for site in sites {
                let coordinate = (site.blob.clone(), site.span);
                if seen
                    .insert(coordinate.clone(), name)
                    .is_some_and(|prior| prior != name)
                {
                    index.collapsed.insert(coordinate);
                }
            }
        }
        for (path, facts) in &index.facts {
            if let Some(blob) = index.blobs.get(path) {
                index
                    .aliases
                    .extend(facts.aliases.iter().map(|span| (blob.clone(), *span)));
            }
        }
        #[cfg(feature = "rust-checker")]
        {
            let mut manifests = HashMap::new();
            let mut workspaces = HashMap::new();
            let mut files: Vec<_> = index.facts.keys().cloned().collect();
            files.sort();
            for file in files {
                let source = index.source_paths[&file].clone();
                let manifest = super::rust_workspace::nearest_manifest(&source);
                let selected = manifest
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string())
                    .unwrap_or_default();
                let result = match manifest {
                    Ok(manifest) => manifests
                        .entry(manifest)
                        .or_insert_with(|| {
                            super::rust_workspace::discover(&source)
                                .map(Arc::new)
                                .map_err(|error| error.reason)
                        })
                        .clone(),
                    Err(error) => Err(error),
                };
                let host = result.and_then(|workspace| {
                    if !index.workspaces.iter().any(|cached| {
                        cached.metadata.workspace_root == workspace.metadata.workspace_root
                    }) {
                        index.workspaces.push(workspace.clone());
                    }
                    let root = workspace
                        .metadata
                        .workspace_root
                        .as_std_path()
                        .to_path_buf();
                    workspaces
                        .entry(root)
                        .or_insert_with(|| {
                            let files = index
                                .source_paths
                                .values()
                                .filter(|path| {
                                    path.starts_with(
                                        workspace.metadata.workspace_root.as_std_path(),
                                    )
                                })
                                .cloned()
                                .collect::<Vec<_>>();
                            module_tree_for_workspace_files(
                                &files,
                                &workspace,
                                Duration::from_secs(120),
                            )
                            .map(|host| {
                                host.places_for_files(&files);
                                Arc::new(host)
                            })
                            .map_err(|error| error.to_string())
                        })
                        .clone()
                });
                match host {
                    Ok(host) => {
                        if let Err(reason) = module_places(&host, &source) {
                            index.context_failures.push((
                                file.clone(),
                                selected,
                                reason.to_string(),
                            ));
                        }
                        if !index
                            .unique_hosts
                            .iter()
                            .any(|cached| Arc::ptr_eq(cached, &host))
                        {
                            index.unique_hosts.push(host.clone());
                        }
                        index.hosts.insert(file, host);
                    }
                    Err(reason) => index.context_failures.push((file, selected, reason)),
                }
            }
        }
        #[cfg(not(feature = "rust-checker"))]
        for file in index.facts.keys() {
            index.context_failures.push((
                file.clone(),
                String::new(),
                super::rust_checker::CheckerError::NotBuilt.to_string(),
            ));
        }
        index
    }

    pub fn context_facts(&self) -> Vec<crate::read::wire::FlatFact> {
        self.context_failures
            .iter()
            .map(
                |(path, manifest, reason)| crate::read::wire::FlatFact::FileUnresolvedRow {
                    src_path: path.clone(),
                    module: manifest.clone(),
                    reason: reason.clone(),
                },
            )
            .collect()
    }

    pub fn open(texts: &[(PathBuf, String)]) -> Result<Self, String> {
        let files = texts
            .iter()
            .map(|(path, _)| {
                (
                    path.to_string_lossy().to_string(),
                    RustModuleFacts::default(),
                )
            })
            .collect();
        let index = Self::build(files, &[], &DefIndex::default());
        index.sync(texts)?;
        Ok(index)
    }

    #[cfg(feature = "rust-checker")]
    pub fn all_places(&self) -> Vec<super::rust_checker::ModulePlace> {
        let mut seen = BTreeSet::new();
        let mut places = Vec::new();
        for host in &self.unique_hosts {
            if seen.insert(Arc::as_ptr(host) as usize) {
                places.extend(super::rust_checker::all_module_places(host));
            }
        }
        places.sort();
        places.dedup();
        places
    }

    #[cfg(feature = "rust-checker")]
    pub fn packages(&self) -> Vec<cargo_metadata::Package> {
        let mut packages = Vec::new();
        for workspace in &self.workspaces {
            packages.extend(workspace.metadata.workspace_packages().into_iter().cloned());
        }
        packages
    }

    /// Overlays are fed to the same host graph before any edit caller queries it.
    pub fn sync(&self, texts: &[(PathBuf, String)]) -> Result<(), String> {
        #[cfg(feature = "rust-checker")]
        {
            let mut seen = BTreeSet::new();
            for host in &self.unique_hosts {
                if seen.insert(host.manifest.clone()) {
                    host.sync(texts).map_err(|error| error.to_string())?;
                }
            }
            Ok(())
        }
        #[cfg(not(feature = "rust-checker"))]
        {
            let _ = texts;
            Err(super::rust_checker::CheckerError::NotBuilt.to_string())
        }
    }

    pub fn blob_of(&self, path: &str) -> Option<&ContentId> {
        self.blobs.get(path)
    }

    pub fn homes(&self, file: &str) -> Result<Vec<(PathBuf, Vec<String>)>, String> {
        #[cfg(feature = "rust-checker")]
        {
            let host = self.hosts.get(file).ok_or_else(|| {
                self.context_failures
                    .iter()
                    .find(|(path, _, _)| path == file)
                    .map(|(_, _, reason)| reason.clone())
                    .unwrap_or_else(|| "outside_workspace".into())
            })?;
            module_places(
                host,
                &self
                    .source_paths
                    .get(file)
                    .cloned()
                    .unwrap_or_else(|| absolute(file)),
            )
            .map(|places| {
                places
                    .into_iter()
                    .map(|place| (place.crate_root, place.path))
                    .collect()
            })
            .map_err(|error| error.to_string())
        }
        #[cfg(not(feature = "rust-checker"))]
        {
            let _ = file;
            Err(super::rust_checker::CheckerError::NotBuilt.to_string())
        }
    }

    pub fn resolve_module(
        &self,
        root: &Path,
        path: &[String],
        chain: &[String],
        prefix: &[String],
    ) -> Result<Vec<(PathBuf, Vec<String>)>, String> {
        #[cfg(feature = "rust-checker")]
        {
            let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
            for host in &self.unique_hosts {
                let Some(mut from) = host
                    .places(&root)
                    .into_iter()
                    .find(|place| place.crate_root == root && place.path.is_empty())
                else {
                    continue;
                };
                from.path = path.to_vec();
                return super::rust_checker::resolve_prefix(host, &from, chain, prefix)
                    .map(|places| {
                        places
                            .into_iter()
                            .map(|place| (place.crate_root, place.path))
                            .collect()
                    })
                    .map_err(|error| error.to_string());
            }
            Err("outside_workspace".into())
        }
        #[cfg(not(feature = "rust-checker"))]
        {
            let _ = (root, path, chain, prefix);
            Err(super::rust_checker::CheckerError::NotBuilt.to_string())
        }
    }

    pub fn qualified_binding(
        &self,
        from: &str,
        qualifier: &[String],
        name: &str,
    ) -> Result<Option<ResolvedImport>, UnresolvedReason> {
        let path: Vec<_> = qualifier
            .iter()
            .cloned()
            .chain(name.split("::").map(str::to_string))
            .collect();
        let mut bound = self.binding_at(from, &path, None, crate::read::shape::FamilyTag::Call)?;
        if !qualifier.is_empty()
            && bound.target_name.is_some()
            && self
                .binding_at(from, qualifier, None, crate::read::shape::FamilyTag::Call)
                .is_ok_and(|module| {
                    module.target_name.is_none() && module.target_path != bound.target_path
                })
        {
            bound.kind = ResolvedImportKind::Indirect;
            bound.hops = self.import_hops(from, qualifier, name, None);
        }
        Ok(Some(bound))
    }

    fn import_hops(
        &self,
        from: &str,
        qualifier: &[String],
        name: &str,
        offset: Option<u32>,
    ) -> u32 {
        let mut file = from.to_string();
        let mut qualifier = qualifier.to_vec();
        let mut name = name.to_string();
        let mut offset = offset;
        let mut seen = BTreeSet::new();
        let mut hops = 0;
        while !qualifier.is_empty() {
            let Ok(module) = self.binding_at(
                &file,
                &qualifier,
                offset,
                crate::read::shape::FamilyTag::Call,
            ) else {
                break;
            };
            if module.target_name.is_some()
                || !seen.insert((module.target_path.clone(), name.clone()))
            {
                break;
            }
            let Some(facts) = self.facts.get(&module.target_path) else {
                break;
            };
            let mut bindings = facts.uses.iter().filter(|binding| binding.local == name);
            let Some(binding) = bindings.next() else {
                break;
            };
            if bindings.next().is_some() {
                break;
            }
            hops += 1;
            file = module.target_path;
            qualifier = binding.qualifier.clone();
            name = binding.asked.clone();
            offset = Some(binding.offset);
        }
        hops
    }

    pub fn binding_at(
        &self,
        from: &str,
        path: &[String],
        offset: Option<u32>,
        family: crate::read::shape::FamilyTag,
    ) -> Result<ResolvedImport, UnresolvedReason> {
        #[cfg(feature = "rust-checker")]
        {
            let host = self.hosts.get(from).ok_or(UnresolvedReason::NoCorpusDef)?;
            let destinations = super::rust_checker::resolve_path_at(
                host,
                &self
                    .source_paths
                    .get(from)
                    .cloned()
                    .unwrap_or_else(|| absolute(from)),
                path,
                offset,
            )
            .map_err(|reason| match reason {
                super::rust_checker::Abstain::NeedsTypes => UnresolvedReason::NeedsTypes,
                super::rust_checker::Abstain::OutsideWorkspace => UnresolvedReason::External,
                super::rust_checker::Abstain::NotInModuleTree => UnresolvedReason::NoCorpusDef,
                super::rust_checker::Abstain::UnresolvedPath => UnresolvedReason::NoCorpusDef,
                super::rust_checker::Abstain::Ambiguous => UnresolvedReason::Ambiguous,
            })?;
            self.join_destinations(from, path, family, destinations)
        }
        #[cfg(not(feature = "rust-checker"))]
        {
            let _ = (from, path, offset, family);
            Err(UnresolvedReason::NoCorpusDef)
        }
    }

    pub fn method_binding_at(
        &self,
        from: &str,
        method: &str,
        offset: u32,
    ) -> Result<ResolvedImport, UnresolvedReason> {
        if self.receivers != super::fast_receivers::FastReceivers::Written {
            return Err(UnresolvedReason::NeedsTypes);
        }
        #[cfg(feature = "rust-checker")]
        {
            let host = self.hosts.get(from).ok_or(UnresolvedReason::NeedsTypes)?;
            let file = self
                .source_paths
                .get(from)
                .cloned()
                .unwrap_or_else(|| absolute(from));
            let destinations =
                super::rust_checker::resolve_written_method(host, &file, offset, method)
                    .map_err(|_| UnresolvedReason::NeedsTypes)?;
            self.join_destinations(
                from,
                &[method.to_string()],
                crate::read::shape::FamilyTag::Call,
                destinations,
            )
            .map_err(|_| UnresolvedReason::NeedsTypes)
        }
        #[cfg(not(feature = "rust-checker"))]
        {
            let _ = (from, method, offset);
            Err(UnresolvedReason::NeedsTypes)
        }
    }

    #[cfg(feature = "rust-checker")]
    fn join_destinations(
        &self,
        from: &str,
        path: &[String],
        family: crate::read::shape::FamilyTag,
        destinations: Vec<super::rust_checker::DefPlace>,
    ) -> Result<ResolvedImport, UnresolvedReason> {
        let name = path.last().ok_or(UnresolvedReason::NoCorpusDef)?;
        let mut joined = Vec::new();
        for def in destinations {
            let Some(target_path) = self.paths.get(&def.file) else {
                continue;
            };
            let Some(blob) = self.blobs.get(target_path) else {
                continue;
            };
            let span = if def.module {
                Span::anchor(0)
            } else {
                let blobs = HashMap::from([(
                    def.file.to_str().ok_or(UnresolvedReason::NoCorpusDef)?,
                    blob,
                )]);
                match super::answer_of(
                    (
                        def.file.to_str().ok_or(UnresolvedReason::NoCorpusDef)?,
                        &def.name,
                        def.start,
                    ),
                    if family == crate::read::shape::FamilyTag::Type {
                        &[crate::read::shape::FamilyTag::Type]
                    } else {
                        super::CALL_FACETS
                    },
                    &blobs,
                    &self.defs,
                ) {
                    Some(super::CheckerAnswer::Corpus(_, span)) => span,
                    _ => continue,
                }
            };
            joined.push(ResolvedImport {
                local: name.to_string(),
                name: if def.module {
                    "*".into()
                } else {
                    name.to_string()
                },
                target_path: target_path.clone(),
                target_blob: blob.clone(),
                target_span: span,
                target_name: (!def.module).then_some(def.name),
                kind: if def.module {
                    ResolvedImportKind::Namespace
                } else if path.len() == 1
                    && target_path != from
                    && !self.facts.get(from).is_some_and(|facts| {
                        facts.uses.iter().any(|binding| binding.local == path[0])
                    })
                {
                    ResolvedImportKind::Star
                } else {
                    ResolvedImportKind::Local
                },
                hops: 0,
            });
        }
        joined
            .sort_by(|a, b| (&a.target_path, a.target_span).cmp(&(&b.target_path, b.target_span)));
        joined.dedup();
        match joined.as_slice() {
            [one] => Ok(one.clone()),
            [] => Err(UnresolvedReason::NoCorpusDef),
            _ => Err(UnresolvedReason::Ambiguous),
        }
    }

    pub fn target(&self, from: &str, name: &str) -> Option<(ContentId, Span)> {
        let bound = self.qualified_binding(from, &[], name).ok()??;
        bound
            .target_name
            .map(|_| (bound.target_blob, bound.target_span))
    }

    pub fn module_call(&self, from: &str, qualifier: &[String], callee: &str) -> ModuleCallTarget {
        match self.qualified_binding(from, qualifier, callee) {
            Ok(Some(bound)) if bound.target_name.is_some() => {
                ModuleCallTarget::Target(bound.target_blob, bound.target_span)
            }
            _ if self
                .external_type_crate(
                    from,
                    &qualifier
                        .iter()
                        .cloned()
                        .chain([callee.to_string()])
                        .collect::<Vec<_>>()
                        .join("::"),
                )
                .is_some() =>
            {
                ModuleCallTarget::External
            }
            _ => ModuleCallTarget::Miss,
        }
    }

    pub fn declaring_files(&self, file: &str) -> Vec<String> {
        #[cfg(feature = "rust-checker")]
        {
            let mut files: Vec<_> = self
                .hosts
                .get(file)
                .and_then(|host| {
                    module_places(
                        host,
                        &self
                            .source_paths
                            .get(file)
                            .cloned()
                            .unwrap_or_else(|| absolute(file)),
                    )
                    .ok()
                })
                .into_iter()
                .flatten()
                .filter_map(|place| place.decl)
                .filter_map(|(path, _, _)| self.paths.get(&path).cloned())
                .collect();
            files.sort();
            files.dedup();
            files
        }
        #[cfg(not(feature = "rust-checker"))]
        {
            let _ = file;
            Vec::new()
        }
    }

    pub fn bindings(&self, file: &str) -> Vec<ImportRow> {
        let Some(facts) = self.facts.get(file) else {
            return Vec::new();
        };
        let names: Vec<(String, String, ResolvedImportKind, Option<u32>)> = facts
            .uses
            .iter()
            .map(|binding| {
                (
                    binding.local.clone(),
                    binding.asked.clone(),
                    ResolvedImportKind::Local,
                    Some(binding.offset),
                )
            })
            .collect();
        let mut rows = Vec::new();
        for (local, asked, mut kind, offset) in names {
            let mut hops = 0;
            if let Ok(bound) = self.binding_at(
                file,
                &[local.clone()],
                offset,
                crate::read::shape::FamilyTag::Call,
            ) {
                if kind == ResolvedImportKind::Star && bound.target_path == file {
                    continue;
                }
                if kind == ResolvedImportKind::Local {
                    if let Some(binding) = facts
                        .uses
                        .iter()
                        .find(|binding| binding.local == local && Some(binding.offset) == offset)
                    {
                        if !binding.qualifier.is_empty()
                            && self
                                .binding_at(
                                    file,
                                    &binding.qualifier,
                                    offset,
                                    crate::read::shape::FamilyTag::Call,
                                )
                                .is_ok_and(|module| {
                                    module.target_name.is_none()
                                        && module.target_path != bound.target_path
                                })
                        {
                            kind = ResolvedImportKind::Indirect;
                            hops =
                                self.import_hops(file, &binding.qualifier, &binding.asked, offset);
                        }
                    }
                }
                rows.push(ImportRow {
                    local,
                    name: asked,
                    target_path: bound.target_path,
                    target_name: bound.target_name,
                    kind,
                    hops,
                });
            }
        }
        #[cfg(feature = "rust-checker")]
        if let Some(host) = self.hosts.get(file) {
            for place in host.declared_places(&self.source_paths[file]) {
                if let Some(target_path) = self.paths.get(&place.file) {
                    rows.push(ImportRow {
                        local: String::new(),
                        name: place.path.last().cloned().unwrap_or_default(),
                        target_path: target_path.clone(),
                        target_name: None,
                        kind: ResolvedImportKind::Module,
                        hops: 1,
                    });
                }
            }
            for (name, place) in
                super::rust_checker::dependency_places(host, &self.source_paths[file])
            {
                if let Some(target_path) = self.paths.get(&place.file) {
                    rows.push(ImportRow {
                        local: String::new(),
                        name,
                        target_path: target_path.clone(),
                        target_name: None,
                        kind: ResolvedImportKind::Module,
                        hops: 0,
                    });
                }
            }
        }
        rows.sort_by(|a, b| {
            (&a.local, &a.target_path, &a.target_name).cmp(&(
                &b.local,
                &b.target_path,
                &b.target_name,
            ))
        });
        rows.dedup_by(|a, b| {
            a.local == b.local && a.target_path == b.target_path && a.target_name == b.target_name
        });
        rows
    }

    pub fn external_type_crate(&self, from: &str, name: &str) -> Option<String> {
        #[cfg(feature = "rust-checker")]
        {
            let host = self.hosts.get(from)?;
            let explicit = name.split("::").map(str::to_string).collect::<Vec<_>>();
            let head = explicit.first()?;
            let binding = self
                .facts
                .get(from)?
                .uses
                .iter()
                .find(|binding| &binding.local == head);
            let path = match binding {
                Some(binding) => binding
                    .qualifier
                    .iter()
                    .cloned()
                    .chain([binding.asked.clone()])
                    .chain(explicit[1..].iter().cloned())
                    .collect(),
                None => explicit,
            };
            matches!(
                super::rust_checker::resolve_path(
                    host,
                    &self
                        .source_paths
                        .get(from)
                        .cloned()
                        .unwrap_or_else(|| absolute(from)),
                    &path
                ),
                Err(super::rust_checker::Abstain::OutsideWorkspace)
            )
            .then(|| path.first().cloned())
            .flatten()
        }
        #[cfg(not(feature = "rust-checker"))]
        {
            let _ = (from, name);
            None
        }
    }
    pub fn is_collapsed(&self, blob: &ContentId, span: Span) -> bool {
        self.collapsed.contains(&(blob.clone(), span))
    }
    pub fn is_alias(&self, blob: &ContentId, span: Span) -> bool {
        self.aliases.contains(&(blob.clone(), span))
    }
}
