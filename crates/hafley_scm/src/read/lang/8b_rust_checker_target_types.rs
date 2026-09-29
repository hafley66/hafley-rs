//! rust-analyzer references to named type declarations in selected files.

use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TargetTypeReference {
    pub source_path: String,
    pub owner_name: Option<String>,
    pub site_start: u32,
    pub site_end: u32,
    pub target_path: String,
}

fn owner_name(reference: &ra_ap_ide_db::search::FileReference) -> Option<String> {
    let name = reference.name.as_name_ref()?;
    for ancestor in name.syntax().ancestors() {
        if let Some(item) = ast::Fn::cast(ancestor.clone()) {
            return item.name().map(|name| name.text().to_string());
        }
        if let Some(item) = ast::Struct::cast(ancestor.clone()) {
            return item.name().map(|name| name.text().to_string());
        }
        if let Some(item) = ast::Enum::cast(ancestor.clone()) {
            return item.name().map(|name| name.text().to_string());
        }
        if let Some(item) = ast::Trait::cast(ancestor.clone()) {
            return item.name().map(|name| name.text().to_string());
        }
        if let Some(item) = ast::TypeAlias::cast(ancestor) {
            return item.name().map(|name| name.text().to_string());
        }
    }
    None
}

pub fn target_types(
    root: &Path,
    files: &[(String, PathBuf)],
    seeds: &[(String, String)],
    budget: Duration,
) -> Result<Vec<TargetTypeReference>, CheckerError> {
    if seeds.is_empty() {
        return Ok(Vec::new());
    }
    let (workspace, _) =
        super::super::rust_checker_session::checker_workspace(root, files, budget)?;
    let workspace = workspace.lock().unwrap();
    let wanted: HashMap<PathBuf, &str> = files
        .iter()
        .map(|(name, path)| {
            (
                std::fs::canonicalize(path).unwrap_or_else(|_| path.clone()),
                name.as_str(),
            )
        })
        .collect();
    let mut ids = HashMap::new();
    let mut paths = HashMap::new();
    for (vfs_id, vfs_path) in workspace.vfs.iter() {
        let Some(absolute) = vfs_path.as_path() else {
            continue;
        };
        let path = PathBuf::from(absolute.to_string());
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        if let Some(name) = wanted.get(&path) {
            let id = ra_ap_ide::FileId::from_raw(vfs_id.index());
            ids.insert((*name).to_string(), id);
            paths.insert(id, (*name).to_string());
        }
    }
    let db = workspace.host.raw_database();
    let _query_span = tracing::info_span!("rust_analyzer.queries").entered();
    attach_db(db, || {
        let sema = Semantics::new(db);
        let mut found = BTreeSet::new();
        for (target_path, name) in seeds {
            let Some(&file_id) = ids.get(target_path) else {
                continue;
            };
            let syntax = sema.parse_guess_edition(file_id);
            let definitions = syntax.syntax().descendants().filter_map(|node| {
                let definition = if let Some(item) = ast::Struct::cast(node.clone()) {
                    item.name()
                        .filter(|item_name| item_name.text() == name)
                        .and_then(|_| sema.to_def(&item))
                        .map(|item| Definition::Adt(Adt::Struct(item)))
                } else if let Some(item) = ast::Enum::cast(node.clone()) {
                    item.name()
                        .filter(|item_name| item_name.text() == name)
                        .and_then(|_| sema.to_def(&item))
                        .map(|item| Definition::Adt(Adt::Enum(item)))
                } else if let Some(item) = ast::Union::cast(node.clone()) {
                    item.name()
                        .filter(|item_name| item_name.text() == name)
                        .and_then(|_| sema.to_def(&item))
                        .map(|item| Definition::Adt(Adt::Union(item)))
                } else if let Some(item) = ast::Trait::cast(node.clone()) {
                    item.name()
                        .filter(|item_name| item_name.text() == name)
                        .and_then(|_| sema.to_def(&item))
                        .map(Definition::Trait)
                } else if let Some(item) = ast::TypeAlias::cast(node) {
                    item.name()
                        .filter(|item_name| item_name.text() == name)
                        .and_then(|_| sema.to_def(&item))
                        .map(Definition::TypeAlias)
                } else {
                    None
                };
                definition
            });
            for definition in definitions {
                for (file, references) in definition.usages(&sema).all().references {
                    let source_id = file.file_id(db);
                    let Some(source_path) = paths.get(&source_id) else {
                        continue;
                    };
                    let text = sema
                        .parse_guess_edition(source_id)
                        .syntax()
                        .text()
                        .to_string();
                    let offsets = OffsetMap::new(&text);
                    for reference in references {
                        found.insert(TargetTypeReference {
                            source_path: source_path.clone(),
                            owner_name: owner_name(&reference),
                            site_start: offsets.to_span_offset(u32::from(reference.range.start())),
                            site_end: offsets.to_span_offset(u32::from(reference.range.end())),
                            target_path: target_path.clone(),
                        });
                    }
                }
            }
        }
        Ok(found.into_iter().collect())
    })
}
