//! @comment-ok: module header, the seam list every lang file opens with
//! The kotlin module plane: `import` headers resolved against the supplied
//! file set only, so `import_facts` writes `resolved_import` rows for kotlin
//! the way `ts_resolve.rs` does for ts. Project extraction reuses Kotlin's
//! family tree and query arena; the standalone door parses its own tree.
//!
//! A kotlin package maps to a directory by convention only, so the plane
//! indexes the supplied files' own `package` headers: `import a.b.C` binds
//! `C` to the file declaring a top-level class/object/fun/typealias/val of
//! that name in package `a.b` (kind=local); `import a.b.*` is one star row
//! per file declaring package `a.b`. Kotlin has no re-export, so no chain
//! and no `indirect` kind; a name two files of one package both declare is
//! ambiguous and binds nothing.

use std::collections::{BTreeSet, HashMap};

use crate::family::SpecifierKind;
use crate::lang::ts_resolve::{ImportRow, ResolvedImportKind};
use crate::shape::Strings;

use super::kotlin::{kt_first_child, kt_header_facts, kt_header_facts_from_arena, kt_parse, kt_text};

// ── phase-2 facts ────────────────────────────────────────────────────────────

/// One `import` header, `Specifier`'s NameIds resolved to owned text.
#[derive(Clone, Debug, PartialEq, Eq)]
struct KtImport {
    /// The dotted path as written, `.*` stripped for a wildcard.
    path: String,
    /// The bound name: the alias, else the path's last segment.
    local: String,
    wildcard: bool,
}

/// One file's `package` header, import headers, and top-level declared
/// names.
#[derive(Clone, Debug, Default)]
pub struct KtModuleFacts {
    package: Option<String>,
    imports: Vec<KtImport>,
    top_level: BTreeSet<String>,
}

/// `None`: a non-kotlin path, or a parse that fails.
pub fn kt_module_facts(path: &str, content: &[u8]) -> Option<KtModuleFacts> {
    if !(path.ends_with(".kt") || path.ends_with(".kts")) {
        return None;
    }
    let text = std::str::from_utf8(content).ok()?;
    let tree = kt_parse(text)?;
    let root = tree.root_node();
    let src = text.as_bytes();
    let mut strings = Strings::new();
    let mut raw = Vec::new();
    let package = kt_header_facts(&tree, src, &mut strings, &mut raw).map(|(_, name)| name);
    Some(kt_module_facts_from_parts(root, src, &strings, raw, package))
}

pub(crate) fn kt_module_facts_from_arena(
    root: tree_sitter::Node,
    src: &[u8],
    query: &hafley_scm::QueryExt,
    arena: &hafley_scm::MatchArena,
) -> KtModuleFacts {
    let mut strings = Strings::new();
    let mut raw = Vec::new();
    let package = kt_header_facts_from_arena(src, query, arena, &mut strings, &mut raw)
        .map(|(_, name)| name);
    kt_module_facts_from_parts(root, src, &strings, raw, package)
}

fn kt_module_facts_from_parts(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &Strings,
    raw: Vec<crate::family::Specifier>,
    package: Option<String>,
) -> KtModuleFacts {
    let imports = raw
        .into_iter()
        .filter_map(|spec| {
            let path = strings.lookup(spec.module?).to_string();
            Some(KtImport {
                local: strings.lookup(spec.name).to_string(),
                path,
                wildcard: spec.kind == SpecifierKind::Namespace,
            })
        })
        .collect();
    let mut top_level = BTreeSet::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if let Some(name) = decl_name(child, src) {
            top_level.insert(name);
        }
    }
    KtModuleFacts {
        package,
        imports,
        top_level,
    }
}

/// The name a top-level declaration binds, backticks stripped.
fn decl_name(node: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let identifier = match node.kind() {
        "class_declaration" | "object_declaration" | "type_alias" => {
            kt_first_child(node, "type_identifier")?
        }
        "function_declaration" => kt_first_child(node, "simple_identifier")?,
        "property_declaration" => {
            let variable = kt_first_child(node, "variable_declaration")?;
            kt_first_child(variable, "simple_identifier")?
        }
        _ => return None,
    };
    Some(kt_text(identifier, src).trim_matches('`').to_string())
}

// ── the module plane proper ──────────────────────────────────────────────────

/// THE corpus kotlin module plane, built ONCE per refresh in `resolve_project`.
#[derive(Default)]
pub struct KtModuleIndex {
    facts: HashMap<String, KtModuleFacts>,
    /// package -> the files declaring it, sorted so a star import's rows are
    /// byte-stable whatever order the inputs arrive in.
    package_files: HashMap<String, Vec<String>>,
}

impl KtModuleIndex {
    /// `files` is every kotlin input's facts.
    pub fn build(files: Vec<(String, KtModuleFacts)>) -> KtModuleIndex {
        let mut index = KtModuleIndex::default();
        for (path, facts) in &files {
            if let Some(package) = &facts.package {
                index
                    .package_files
                    .entry(package.clone())
                    .or_default()
                    .push(path.clone());
            }
        }
        for paths in index.package_files.values_mut() {
            paths.sort();
        }
        index.facts = files.into_iter().collect();
        index
    }

    /// `path`'s own `package` header, if it declares one.
    pub fn package_of(&self, path: &str) -> Option<&str> {
        self.facts.get(path)?.package.as_deref()
    }

    /// The def file an import of `path` binds `name` to, with the def's
    /// declared name: a wildcard binds any top-level name of its package, a
    /// named import binds by its alias-or-last-segment and the def is the
    /// path's last segment. The referring file itself never answers.
    pub fn import_target(&self, path: &str, name: &str) -> Option<(String, String)> {
        let facts = self.facts.get(path)?;
        for import in &facts.imports {
            let (package, def) = if import.wildcard {
                (import.path.clone(), name.to_string())
            } else {
                if import.local != name {
                    continue;
                }
                match self.split_import(&import.path) {
                    Some((package, segment)) => (package, segment),
                    None => continue,
                }
            };
            if let Some(file) = self.declaring_file(&package, &def) {
                if file != path {
                    return Some((file.to_string(), def));
                }
            }
        }
        None
    }
    /// The one file in `package` declaring `name` at top level; two files
    /// declaring it is ambiguous and binds nothing.
    pub fn declaring_file(&self, package: &str, name: &str) -> Option<&str> {
        let mut hits = self.package_files.get(package)?.iter().filter(|path| {
            self.facts
                .get(*path)
                .is_some_and(|facts| facts.top_level.contains(name))
        });
        let first = hits.next()?;
        hits.next().is_none().then_some(first.as_str())
    }

    /// `path`'s longest package prefix a supplied file declares, and the
    /// top-level name the next segment spells (`a.b.Outer.Inner` binds `Outer`).
    fn split_import(&self, path: &str) -> Option<(String, String)> {
        let segments: Vec<&str> = path.split('.').collect();
        (1..segments.len()).rev().find_map(|split| {
            let package = segments[..split].join(".");
            self.package_files
                .contains_key(&package)
                .then(|| (package, segments[split].to_string()))
        })
    }

    /// Every import header `path` writes: one `module` row per header whose
    /// package a corpus file declares, plus one binding row when a name binds.
    pub fn bindings(&self, path: &str) -> Vec<ImportRow> {
        let Some(facts) = self.facts.get(path) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for import in &facts.imports {
            if import.wildcard {
                let Some(files) = self.package_files.get(&import.path) else {
                    continue;
                };
                for target in files {
                    rows.push(ImportRow {
                        local: String::new(),
                        name: format!("{}.*", import.path),
                        target_path: target.clone(),
                        target_name: None,
                        kind: ResolvedImportKind::Module,
                        hops: 1,
                    });
                    rows.push(ImportRow {
                        local: "*".to_string(),
                        name: "*".to_string(),
                        target_path: target.clone(),
                        target_name: None,
                        kind: ResolvedImportKind::Star,
                        hops: 1,
                    });
                }
                continue;
            }
            let Some((package, name)) = self.split_import(&import.path) else {
                continue;
            };
            let Some(target) = self.declaring_file(&package, &name) else {
                continue;
            };
            rows.push(ImportRow {
                local: String::new(),
                name: import.path.clone(),
                target_path: target.to_string(),
                target_name: None,
                kind: ResolvedImportKind::Module,
                hops: 1,
            });
            rows.push(ImportRow {
                local: import.local.clone(),
                name: import.path[package.len() + 1..].to_string(),
                target_path: target.to_string(),
                target_name: Some(name),
                kind: ResolvedImportKind::Local,
                hops: 1,
            });
        }
        rows
    }
}
