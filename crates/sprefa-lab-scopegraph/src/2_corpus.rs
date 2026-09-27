use crate::query::{run_query, Capture, QueryFailure};
use crate::scope::{ExternalDefinition, ScopeGraph, UnresolvedReason};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use tree_sitter::Language;

const KOTLIN_LOCALS: &str = include_str!("../queries/kotlin/locals.scm");

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SymbolOp {
    Push(String),
    Pop(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportBinding {
    pub path: Vec<String>,
    pub local_name: String,
    pub wildcard: bool,
    pub stack: Vec<SymbolOp>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    pub path: String,
    pub package: String,
    pub name: String,
    pub start: usize,
    pub role: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KotlinUnit {
    pub path: String,
    pub package: String,
    pub imports: Vec<ImportBinding>,
    pub exports: Vec<Export>,
    pub graph: ScopeGraph,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Corpus {
    pub units: BTreeMap<String, KotlinUnit>,
}

impl Corpus {
    pub fn build_kotlin(
        sources: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self, QueryFailure> {
        let language = Language::new(tree_sitter_kotlin_sg::LANGUAGE);
        let mut units = BTreeMap::new();
        for (path, source) in sources {
            let matches = run_query(language.clone(), &source, KOTLIN_LOCALS, Path::new(&path))?;
            let captures = matches
                .into_iter()
                .flat_map(|matched| matched.captures)
                .collect::<Vec<_>>();
            let package = captures
                .iter()
                .find(|capture| capture.label == "local.package")
                .map(|capture| {
                    capture
                        .text
                        .trim()
                        .strip_prefix("package")
                        .unwrap_or(&capture.text)
                        .trim()
                        .to_string()
                })
                .unwrap_or_default();
            let imports = captures
                .iter()
                .filter(|capture| capture.label == "local.import")
                .map(|capture| parse_import(&capture.text))
                .collect();
            let graph = crate::scope::resolve(captures.clone(), source.len());
            let exports = graph
                .definitions
                .iter()
                .filter(|definition| is_top_level_export(&definition.capture))
                .map(|definition| Export {
                    path: path.clone(),
                    package: package.clone(),
                    name: definition.name.clone(),
                    start: definition.capture.start,
                    role: definition.role.clone(),
                })
                .collect();
            units.insert(
                path.clone(),
                KotlinUnit {
                    path,
                    package,
                    imports,
                    exports,
                    graph,
                },
            );
        }
        Ok(Self { units })
    }

    pub fn resolve(&mut self) {
        let mut exports: BTreeMap<(String, String), Vec<Export>> = BTreeMap::new();
        for unit in self.units.values() {
            for export in &unit.exports {
                exports
                    .entry((export.package.clone(), export.name.clone()))
                    .or_default()
                    .push(export.clone());
            }
        }
        for unit in self.units.values_mut() {
            for reference in &mut unit.graph.references {
                if reference.definition.is_some() {
                    continue;
                }
                let mut candidates = Vec::new();
                let mut origin = "module_plane";
                for import in &unit.imports {
                    let target = if import.wildcard {
                        Some((import.path.join("."), reference.name.clone()))
                    } else if import.local_name == reference.name {
                        import
                            .path
                            .split_last()
                            .map(|(name, package)| (package.join("."), name.to_string()))
                    } else {
                        None
                    };
                    if let Some((package, name)) = target {
                        candidates
                            .extend(exports.get(&(package, name)).into_iter().flatten().cloned());
                    }
                }
                if candidates.is_empty() {
                    origin = "same_package";
                    candidates.extend(
                        exports
                            .get(&(unit.package.clone(), reference.name.clone()))
                            .into_iter()
                            .flatten()
                            .cloned(),
                    );
                }
                let candidates = unique_exports(candidates);
                match candidates.as_slice() {
                    [target] => {
                        reference.external = Some(ExternalDefinition {
                            path: target.path.clone(),
                            name: target.name.clone(),
                            start: target.start,
                            role: target.role.clone(),
                            resolution: origin.into(),
                        });
                        reference.unresolved = None;
                    }
                    [] => {}
                    _ => reference.unresolved = Some(UnresolvedReason::AmbiguousExternal),
                }
            }
        }
    }
}

pub fn symbol_stack(qualified_name: &str) -> Vec<SymbolOp> {
    let parts = qualified_name
        .split('.')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    parts
        .iter()
        .rev()
        .map(|part| SymbolOp::Push((*part).into()))
        .chain(parts.iter().map(|part| SymbolOp::Pop((*part).into())))
        .collect()
}

fn parse_import(text: &str) -> ImportBinding {
    let text = text.trim().strip_prefix("import").unwrap_or(text).trim();
    let (path, alias) = text
        .split_once(" as ")
        .map(|(path, alias)| (path.trim(), Some(alias.trim())))
        .unwrap_or((text.trim(), None));
    let wildcard = path.ends_with(".*");
    let path = path
        .trim_end_matches(".*")
        .split('.')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let local_name = if wildcard {
        "*".to_string()
    } else {
        alias
            .map(str::to_string)
            .or_else(|| path.last().cloned())
            .unwrap_or_default()
    };
    ImportBinding {
        stack: symbol_stack(&path.join(".")),
        path,
        local_name,
        wildcard,
    }
}

fn is_top_level_export(capture: &Capture) -> bool {
    if !capture.label.starts_with("local.definition.")
        || capture
            .ancestor_kinds
            .iter()
            .any(|kind| kind == "import_header")
    {
        return false;
    }
    let functions = capture
        .ancestor_kinds
        .iter()
        .filter(|kind| kind.as_str() == "function_declaration")
        .count();
    let classes = capture
        .ancestor_kinds
        .iter()
        .filter(|kind| kind.as_str() == "class_declaration")
        .count();
    (capture.label == "local.definition.namespace" && functions == 1 && classes == 0)
        || (capture.label == "local.definition.type" && classes == 1 && functions == 0)
}

fn unique_exports(exports: Vec<Export>) -> Vec<Export> {
    let mut seen = BTreeSet::new();
    exports
        .into_iter()
        .filter(|export| seen.insert((export.path.clone(), export.start, export.name.clone())))
        .collect()
}
