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
pub struct SymbolStackEdge {
    pub path: Vec<String>,
    pub from: Vec<String>,
    pub operation: SymbolOp,
    pub to: Vec<String>,
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
    pub owner: Option<String>,
    pub value_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberAccess {
    pub receiver: Capture,
    pub member: Capture,
    pub access: Capture,
    pub receiver_type: Option<String>,
    pub result_type: Option<String>,
    pub target: Option<ExternalDefinition>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KotlinUnit {
    pub path: String,
    pub package: String,
    pub imports: Vec<ImportBinding>,
    pub exports: Vec<Export>,
    pub symbol_edges: Vec<SymbolStackEdge>,
    pub member_accesses: Vec<MemberAccess>,
    pub captures: Vec<Capture>,
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
            let member_accesses = matches
                .iter()
                .filter_map(|matched| {
                    let capture = |label: &str| {
                        matched
                            .captures
                            .iter()
                            .find(|capture| capture.label == label)
                            .cloned()
                    };
                    Some(MemberAccess {
                        receiver: capture("local.receiver")?,
                        member: capture("local.member.reference")?,
                        access: capture("local.member.access")?,
                        receiver_type: None,
                        result_type: None,
                        target: None,
                    })
                })
                .collect::<Vec<_>>();
            let captures = matches
                .iter()
                .flat_map(|matched| matched.captures.iter().cloned())
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
                .collect::<Vec<_>>();
            let symbol_edges = imports
                .iter()
                .flat_map(|import| stack_edges(&import.path, &import.stack))
                .collect();
            let graph = crate::scope::resolve(captures.clone(), source.len());
            let type_definitions = graph
                .definitions
                .iter()
                .filter(|definition| definition.role == "type")
                .collect::<Vec<_>>();
            let exports = graph
                .definitions
                .iter()
                .filter_map(|definition| {
                    let owner = (definition.role != "type")
                        .then(|| enclosing_type(&definition.capture, &type_definitions))
                        .flatten();
                    (owner.is_some() || is_top_level_export(&definition.capture)).then(|| Export {
                        path: path.clone(),
                        package: package.clone(),
                        name: definition.name.clone(),
                        start: definition.capture.start,
                        role: definition.role.clone(),
                        owner,
                        value_type: capture_value_type(&definition.capture, &captures),
                    })
                })
                .collect();
            units.insert(
                path.clone(),
                KotlinUnit {
                    path,
                    package,
                    imports,
                    exports,
                    symbol_edges,
                    member_accesses,
                    captures,
                    graph,
                },
            );
        }
        Ok(Self { units })
    }

    pub fn resolve(&mut self) {
        let mut exports: BTreeMap<(String, String), Vec<Export>> = BTreeMap::new();
        let member_exports = self
            .units
            .values()
            .flat_map(|unit| unit.exports.iter())
            .filter(|export| export.owner.is_some())
            .cloned()
            .collect::<Vec<_>>();
        for unit in self.units.values() {
            for export in unit.exports.iter().filter(|export| export.owner.is_none()) {
                exports
                    .entry((export.package.clone(), export.name.clone()))
                    .or_default()
                    .push(export.clone());
            }
        }
        for unit in self.units.values_mut() {
            unit.member_accesses
                .sort_by_key(|access| (access.access.start, access.access.end));
            let mut resolved_accesses = Vec::with_capacity(unit.member_accesses.len());
            for mut access in std::mem::take(&mut unit.member_accesses) {
                access.receiver_type =
                    infer_receiver_type(unit, &access.receiver, &exports, &resolved_accesses);
                let candidates = member_exports
                    .iter()
                    .filter(|export| {
                        export.owner.as_deref() == access.receiver_type.as_deref()
                            && export.name == access.member.text
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                match unique_exports(candidates).as_slice() {
                    [target] => {
                        access.result_type = target.value_type.clone();
                        access.target = Some(ExternalDefinition {
                            path: target.path.clone(),
                            name: target.name.clone(),
                            start: target.start,
                            role: target.role.clone(),
                            resolution: "typed_receiver".into(),
                        });
                    }
                    _ => {}
                }
                resolved_accesses.push(access);
            }
            unit.member_accesses = resolved_accesses;
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

fn stack_edges(path: &[String], stack: &[SymbolOp]) -> Vec<SymbolStackEdge> {
    let mut state = Vec::new();
    let mut edges = Vec::new();
    for operation in stack {
        let from = state.clone();
        match operation {
            SymbolOp::Push(name) => state.insert(0, name.clone()),
            SymbolOp::Pop(name) if state.first() == Some(name) => {
                state.remove(0);
            }
            SymbolOp::Pop(_) => continue,
        }
        edges.push(SymbolStackEdge {
            path: path.to_vec(),
            from,
            operation: operation.clone(),
            to: state.clone(),
        });
    }
    edges
}

fn enclosing_type(
    capture: &Capture,
    type_definitions: &[&crate::scope::Definition],
) -> Option<String> {
    let declaration = capture
        .ancestor_kinds
        .iter()
        .zip(&capture.ancestors)
        .find(|(kind, _)| *kind == "class_declaration" || *kind == "object_declaration")
        .map(|(_, span)| span)?;
    type_definitions.iter().find_map(|definition| {
        definition
            .capture
            .ancestor_kinds
            .iter()
            .zip(&definition.capture.ancestors)
            .any(|(kind, span)| {
                (kind == "class_declaration" || kind == "object_declaration") && span == declaration
            })
            .then(|| definition.name.clone())
    })
}

fn capture_value_type(capture: &Capture, captures: &[Capture]) -> Option<String> {
    let scope_kinds = ["parameter", "class_parameter", "variable_declaration"];
    let scopes = capture
        .ancestor_kinds
        .iter()
        .zip(&capture.ancestors)
        .filter(|(kind, _)| scope_kinds.contains(&kind.as_str()))
        .map(|(_, span)| *span)
        .collect::<Vec<_>>();
    captures
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.label.as_str(),
                "local.parameter.type" | "local.member.type" | "local.binding.type"
            )
        })
        .find(|candidate| candidate.ancestors.iter().any(|span| scopes.contains(span)))
        .map(|candidate| candidate.text.clone())
}

fn infer_receiver_type(
    unit: &KotlinUnit,
    receiver: &Capture,
    exports: &BTreeMap<(String, String), Vec<Export>>,
    resolved_accesses: &[MemberAccess],
) -> Option<String> {
    if let Some(access) = resolved_accesses
        .iter()
        .find(|access| access.access.text == receiver.text)
    {
        return access.result_type.clone();
    }
    if exports
        .values()
        .flatten()
        .any(|export| export.role == "type" && export.name == receiver.text)
    {
        return Some(receiver.text.clone());
    }
    let definition = unit
        .graph
        .definitions
        .iter()
        .filter(|definition| definition.name == receiver.text)
        .filter(|definition| definition.capture.start <= receiver.start)
        .max_by_key(|definition| definition.capture.start)?;
    let value_type = capture_value_type(&definition.capture, &unit.captures);
    value_type.map(|value_type| {
        unit.captures
            .iter()
            .find(|capture| capture.label == "local.type.parameter" && capture.text == value_type)
            .and_then(|parameter| {
                unit.captures
                    .iter()
                    .find(|capture| {
                        capture.label == "local.type.bound" && capture.parent == parameter.parent
                    })
                    .map(|capture| capture.text.clone())
            })
            .unwrap_or(value_type)
    })
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
