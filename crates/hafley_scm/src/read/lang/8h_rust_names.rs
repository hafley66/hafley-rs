//! The Names provider beside body_edges. Module def maps only, no inference.

use super::modules::{ModulePlace, RustModuleTree, host_path, vfs_path};
use super::*;
use ra_ap_hir::{AsAssocItem, AssocItem, HasVisibility, Module, ScopeDef};

pub type NamesHost = RustModuleTree;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Abstain {
    OutsideWorkspace,
    NeedsTypes,
    UnresolvedPath,
    Ambiguous,
}

impl std::fmt::Display for Abstain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::OutsideWorkspace => "outside_workspace",
            Self::NeedsTypes => "needs_types",
            Self::UnresolvedPath => "unresolved_path",
            Self::Ambiguous => "ambiguous",
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
    resolve_path_at(host, file, path, None)
}

/// Resolve at an inline module or lexical scope without receiver inference.
pub fn resolve_path_at(
    host: &NamesHost,
    file: &Path,
    path: &[String],
    offset: Option<u32>,
) -> Result<Vec<DefPlace>, Abstain> {
    let mut workspace = host.workspace.lock().unwrap();
    let key = (host_path(file), path.to_vec(), offset);
    if let Some(answer) = workspace.names.paths.get(&key) {
        return answer.clone();
    }
    let Some((id, _)) = workspace.vfs.file_id(&vfs_path(&host_path(file))) else {
        return Err(Abstain::OutsideWorkspace);
    };
    let db = workspace.host.raw_database();
    let answer = attach_db(db, || {
        let sema = Semantics::new(db);
        let modules: Vec<_> = sema
            .file_to_module_defs(ra_ap_ide::FileId::from_raw(id.index()))
            .collect();
        if modules.is_empty() {
            return Err(Abstain::OutsideWorkspace);
        }
        let parsed = sema.parse_guess_edition(ra_ap_ide::FileId::from_raw(id.index()));
        let node = offset
            .filter(|offset| *offset <= u32::from(parsed.syntax().text_range().end()))
            .and_then(|offset| {
                parsed
                    .syntax()
                    .token_at_offset(ra_ap_syntax::TextSize::from(offset))
                    .right_biased()
            })
            .and_then(|token| token.parent());
        // A closure invocation has no written module path. The extractor's
        // nested call spelling must not resolve the outer closure invocation.
        if offset.is_some_and(|offset| {
            node.as_ref().is_some_and(|node| {
                node.ancestors()
                    .find_map(ast::CallExpr::cast)
                    .is_some_and(|call| {
                        call.expr().is_some_and(|callee| {
                            callee
                                .syntax()
                                .text_range()
                                .contains(ra_ap_syntax::TextSize::from(offset))
                                && !matches!(callee, ast::Expr::PathExpr(_))
                        })
                    })
            })
        }) {
            return Err(Abstain::NeedsTypes);
        }
        let mut chain = node
            .as_ref()
            .map(|node| {
                node.ancestors()
                    .filter_map(ast::Module::cast)
                    .filter_map(|module| module.name().map(|name| name.text().to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        chain.reverse();
        let scoped = node
            .as_ref()
            .and_then(|node| sema.scope(node))
            .or_else(|| sema.scope(parsed.syntax()));
        let syntax = ra_ap_syntax::SourceFile::parse(
            &format!("use {};", path.join("::")),
            ra_ap_ide::Edition::CURRENT,
        )
        .tree();
        let written = syntax.syntax().descendants().find_map(ast::Path::cast);
        let lexical = scoped
            .as_ref()
            .zip(written.as_ref())
            .and_then(|(scope, path)| scope.speculative_resolve(path));
        if lexical.as_ref().is_some_and(|resolution| {
            !matches!(
                resolution,
                PathResolution::Def(_) | PathResolution::SelfType(_)
            )
        }) {
            return Err(Abstain::NeedsTypes);
        }
        let self_implementation = node
            .as_ref()
            .and_then(|node| node.ancestors().find_map(ast::Impl::cast))
            .and_then(|implementation| sema.to_def(&implementation));
        let mut places = Vec::new();
        let mut external = false;
        let single = modules.len() == 1;
        for mut module in modules {
            let mut reached = true;
            for name in &chain {
                match module
                    .children(db)
                    .find(|child| child.name(db).is_some_and(|child| child.as_str() == name))
                {
                    Some(child) => module = child,
                    None => {
                        reached = false;
                        break;
                    }
                }
            }
            if !reached {
                continue;
            }
            if single
                && node.is_some()
                && path.len() == 1
                && lexical.is_none()
                && !resolve_module_path(db, module, path).is_empty()
            {
                return Err(Abstain::NeedsTypes);
            }
            if path.len() == 1 && ambiguous_glob(db, &sema, module, &path[0]) {
                return Err(Abstain::Ambiguous);
            }
            let root = workspace.vfs.file_path(ra_ap_vfs::FileId::from_raw(
                module.krate(db).root_file(db).index(),
            ));
            external |= root.as_path().is_some_and(|root| {
                host.external_names
                    .get(&PathBuf::from(root.to_string()))
                    .is_some_and(|names| path.first().is_some_and(|name| names.contains(name)))
            });
            let mut definitions = if single && node.is_some() {
                match lexical {
                    Some(PathResolution::Def(def)) => vec![def],
                    Some(PathResolution::SelfType(implementation)) => implementation
                        .self_ty(db)
                        .as_adt()
                        .map(ModuleDef::Adt)
                        .into_iter()
                        .collect(),
                    _ => Vec::new(),
                }
            } else {
                resolve_module_path(db, module, path)
            };
            if definitions.is_empty() && path == ["Self"] {
                definitions.extend(
                    self_implementation
                        .and_then(|implementation| implementation.self_ty(db).as_adt())
                        .map(ModuleDef::Adt),
                );
            }
            if path.len() > 1 && (definitions.is_empty() || definitions.iter().any(|definition| {
                matches!(definition, ModuleDef::Function(function) if function.as_assoc_item(db).is_some_and(|item| item.container_trait(db).is_some()))
            })) {
                let prefix = &path[..path.len() - 1];
                let mut heads = resolve_module_path(db, module, prefix)
                    .into_iter()
                    .map(PathResolution::Def)
                    .collect::<Vec<_>>();
                if let Some(scope) = scoped.as_ref() {
                    let parsed = ra_ap_syntax::SourceFile::parse(
                        &format!("use {};", prefix.join("::")),
                        ra_ap_ide::Edition::CURRENT,
                    )
                    .tree();
                    if let Some(written) = parsed.syntax().descendants().find_map(ast::Path::cast) {
                        if let Some(head) = scope.speculative_resolve(&written) {
                            if single || matches!(head, PathResolution::SelfType(_)) {
                                heads = vec![head];
                            }
                        }
                    }
                }
                if prefix == ["Self"] {
                    if let Some(implementation) = self_implementation {
                        heads = vec![PathResolution::SelfType(implementation)];
                    }
                }
                let mut associated = Vec::new();
                for head in heads {
                    let from = if matches!(head, PathResolution::SelfType(_)) {
                        scoped.as_ref().map_or(module, |scope| scope.module())
                    } else {
                        module
                    };
                    associated.extend(associated_defs(
                        db,
                        from,
                        head,
                        scoped.as_ref(),
                        path.last().unwrap(),
                    ));
                }
                if !associated.is_empty() { definitions = associated; }
            }
            for def in definitions {
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
                let Some(krate) = (match def {
                    ModuleDef::Module(module) => Some(module.krate(db)),
                    _ => def.module(db).map(|module| module.krate(db)),
                }) else {
                    continue;
                };
                let crate_file = workspace
                    .vfs
                    .file_path(ra_ap_vfs::FileId::from_raw(krate.root_file(db).index()));
                if !crate_file.as_path().is_some_and(|path| {
                    host.crate_roots
                        .contains_key(&PathBuf::from(path.to_string()))
                }) {
                    external = true;
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
            Err(if external {
                Abstain::OutsideWorkspace
            } else {
                Abstain::UnresolvedPath
            })
        } else {
            Ok(places)
        }
    });
    workspace.names.paths.insert(key, answer.clone());
    answer
}

/// Compare RA answers for glob sources when its def map retained the first
/// conflicting import. Explicit imports and declarations shadow glob names.
fn ambiguous_glob(
    db: &RootDatabase,
    sema: &Semantics<'_, RootDatabase>,
    module: Module,
    name: &str,
) -> bool {
    if module.declarations(db).iter().any(|definition| {
        definition
            .name(db)
            .is_some_and(|declared| declared.as_str() == name)
    }) {
        return false;
    }
    let source = sema.module_definition_node(module).value;
    let container = ast::Module::cast(source.clone())
        .and_then(|module| module.item_list().map(|items| items.syntax().clone()))
        .unwrap_or(source);
    let uses: Vec<_> = container.children().filter_map(ast::Use::cast).collect();
    let trees: Vec<_> = uses
        .iter()
        .flat_map(|item| item.syntax().descendants().filter_map(ast::UseTree::cast))
        .collect();
    if trees.iter().any(|tree| {
        if tree.star_token().is_some() || tree.use_tree_list().is_some() {
            return false;
        }
        let local = tree
            .rename()
            .and_then(|rename| rename.name())
            .map(|name| name.text().to_string())
            .or_else(|| {
                tree.path().and_then(|path| {
                    path.segment()?
                        .name_ref()
                        .map(|name| name.text().to_string())
                })
            });
        local.as_deref() == Some(name)
    }) {
        return false;
    }
    let mut definitions = Vec::new();
    for tree in trees.iter().filter(|tree| tree.star_token().is_some()) {
        let mut prefixes: Vec<_> = tree
            .syntax()
            .ancestors()
            .filter_map(ast::UseTree::cast)
            .filter_map(|tree| tree.path().map(|path| path.syntax().text().to_string()))
            .collect();
        prefixes.reverse();
        prefixes.push(name.to_string());
        let parsed = ra_ap_syntax::SourceFile::parse(
            &format!("use {};", prefixes.join("::")),
            ra_ap_ide::Edition::CURRENT,
        )
        .tree();
        if let Some(path) = parsed.syntax().descendants().find_map(ast::Path::cast) {
            if let Some(PathResolution::Def(definition)) = sema
                .scope(tree.syntax())
                .and_then(|scope| scope.speculative_resolve(&path))
            {
                if !definitions.contains(&definition) {
                    definitions.push(definition);
                }
            }
        }
    }
    definitions.len() > 1
}

/// Inherent associated items come from RA's indexed impl lookup. This lowers
/// declared type heads, without querying inference for the caller's body.
pub(super) fn associated_defs(
    db: &RootDatabase,
    from: Module,
    head: PathResolution<'_>,
    scope: Option<&ra_ap_hir::SemanticsScope<'_>>,
    name: &str,
) -> Vec<ModuleDef> {
    let mut definitions = Vec::new();
    let mut items = Vec::new();
    let ty = match head {
        PathResolution::Def(ModuleDef::Adt(adt)) => Some(adt.ty(db)),
        PathResolution::Def(ModuleDef::TypeAlias(alias)) => Some(alias.ty(db)),
        PathResolution::SelfType(implementation) => Some(implementation.self_ty(db)),
        PathResolution::Def(ModuleDef::Trait(trait_)) => {
            items.extend(trait_.items(db));
            None
        }
        _ => None,
    };
    if let Some(ty) = ty {
        if let Some(ra_ap_hir::Adt::Enum(enum_)) = ty.as_adt() {
            definitions.extend(
                enum_
                    .variants(db)
                    .into_iter()
                    .filter(|variant| variant.name(db).as_str() == name)
                    .map(ModuleDef::EnumVariant),
            );
        }
        ty.iterate_assoc_items(db, |item| {
            if item
                .name(db)
                .is_some_and(|candidate| candidate.as_str() == name)
                && item.is_visible_from(db, from)
            {
                items.push(item);
            }
            None::<()>
        });
        if items.is_empty() {
            if let Some(scope) = scope {
                ty.iterate_path_candidates(
                    db,
                    scope,
                    &scope.visible_traits(),
                    Some(&ra_ap_hir::Name::new_root(name)),
                    |item| {
                        items.push(item);
                        None::<()>
                    },
                );
            }
        }
        for item in &mut items {
            if let (AssocItem::Function(function), Some(trait_)) = (*item, item.container_trait(db))
            {
                if trait_.type_or_const_param_count(db, false) == 0
                    && ra_ap_hir::GenericDef::Trait(trait_)
                        .lifetime_params(db)
                        .is_empty()
                {
                    if let Some(implementation) = Semantics::new(db).resolve_trait_impl_method(
                        ty.clone(),
                        trait_,
                        function,
                        [ty.clone()],
                    ) {
                        *item = AssocItem::Function(implementation);
                    }
                }
            }
        }
    }
    definitions.extend(
        items
            .into_iter()
            .filter(|item| {
                item.name(db)
                    .is_some_and(|candidate| candidate.as_str() == name)
                    && item.is_visible_from(db, from)
            })
            .map(|item| match item {
                AssocItem::Function(item) => ModuleDef::Function(item),
                AssocItem::Const(item) => ModuleDef::Const(item),
                AssocItem::TypeAlias(item) => ModuleDef::TypeAlias(item),
            }),
    );
    definitions
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
    scope_names_at(host, file, None)
}

pub fn scope_names_at(
    host: &NamesHost,
    file: &Path,
    offset: Option<u32>,
) -> Result<Vec<String>, Abstain> {
    let workspace = host.workspace.lock().unwrap();
    let Some((id, _)) = workspace.vfs.file_id(&vfs_path(&host_path(file))) else {
        return Err(Abstain::OutsideWorkspace);
    };
    let db = workspace.host.raw_database();
    attach_db(db, || {
        let sema = Semantics::new(db);
        let file = ra_ap_ide::FileId::from_raw(id.index());
        let parsed = sema.parse_guess_edition(file);
        let mut names = Vec::new();
        if let Some(offset) =
            offset.filter(|offset| *offset <= u32::from(parsed.syntax().text_range().end()))
        {
            let node = parsed
                .syntax()
                .token_at_offset(offset.into())
                .right_biased()
                .and_then(|token| token.parent());
            if let Some(scope) = node.as_ref().and_then(|node| sema.scope(node)) {
                scope.process_all_names(&mut |name, def| {
                    if matches!(def, ScopeDef::ModuleDef(_)) {
                        names.push(name.as_str().to_string());
                    }
                });
            }
        } else {
            for module in sema.file_to_module_defs(file) {
                names.extend(
                    module
                        .scope(db, None)
                        .into_iter()
                        .filter_map(|(name, def)| {
                            matches!(def, ScopeDef::ModuleDef(_)).then(|| name.as_str().to_string())
                        }),
                );
            }
        }
        names.sort();
        names.dedup();
        if names.is_empty() {
            Err(Abstain::OutsideWorkspace)
        } else {
            Ok(names)
        }
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

/// Enumerate the engine's module graph, including inline modules.
pub fn all_module_places(host: &NamesHost) -> Vec<ModulePlace> {
    let mut workspace = host.workspace.lock().unwrap();
    super::names_cache::ensure_all_places(host, &mut workspace);
    workspace.names.all_places.as_ref().unwrap().clone()
}

/// Workspace dependency roots from RA's crate graph, including a bin's own library.
pub fn dependency_places(host: &NamesHost, file: &Path) -> Vec<(String, ModulePlace)> {
    let mut workspace = host.workspace.lock().unwrap();
    let key = host_path(file);
    if let Some(answer) = workspace.names.dependencies.get(&key) {
        return answer.clone();
    }
    let Some((id, _)) = workspace.vfs.file_id(&vfs_path(&host_path(file))) else {
        return Vec::new();
    };
    let db = workspace.host.raw_database();
    let answer = attach_db(db, || {
        let sema = Semantics::new(db);
        let parsed = sema.parse_guess_edition(ra_ap_ide::FileId::from_raw(id.index()));
        let prefixes: std::collections::HashSet<String> = parsed
            .syntax()
            .descendants()
            .filter_map(ast::Path::cast)
            .filter_map(|path| {
                path.segments()
                    .next()?
                    .name_ref()
                    .map(|name| name.text().to_string())
            })
            .collect();
        let mut places = Vec::new();
        for module in sema.file_to_module_defs(ra_ap_ide::FileId::from_raw(id.index())) {
            for dependency in module.krate(db).dependencies(db) {
                if !prefixes.contains(dependency.name.as_str()) {
                    continue;
                }
                if let Some(place) = host.place_of(&workspace, db, dependency.krate.root_module(db))
                {
                    places.push((dependency.name.as_str().to_string(), place));
                }
            }
        }
        places.sort();
        places.dedup();
        places
    });
    workspace.names.dependencies.insert(key, answer.clone());
    answer
}
