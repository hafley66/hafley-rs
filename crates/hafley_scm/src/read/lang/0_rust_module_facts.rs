//! Rust syntax rows for the shared Names provider and corpus joins.

use crate::read::shape::{ContentId, Span};

/// What one `use` leaf binds a local name to. `qualifier` is the source
/// module's path as written (`crate`/`self`/`super` kept literal).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UseBinding {
    pub(crate) offset: u32,
    pub(crate) local: String,
    pub(crate) qualifier: Vec<String>,
    pub(crate) asked: String,
    pub(crate) reexport: bool,
}

/// A bare `use a::b::*;` / `pub use a::b::*;`: no local name, only a star hop
/// candidate for names the qualifier module's own resolve asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StarImport {
    pub(crate) offset: u32,
    pub(crate) qualifier: Vec<String>,
    pub(crate) reexport: bool,
}

/// Syntax facts consumed by the shared Names corpus adapter.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RustModuleFacts {
    pub(crate) uses: Vec<UseBinding>,
    pub(crate) stars: Vec<StarImport>,
    pub(crate) mod_decls: Vec<(String, Option<String>)>,
    pub(crate) aliases: Vec<Span>,
    pub macro_invocations: Vec<(Span, String)>,
}
/// Standalone fallback for callers without a phase-1 Rust output.
pub fn rust_module_facts(path: &str, content: &[u8]) -> Option<RustModuleFacts> {
    if !path.ends_with(".rs") {
        return None;
    }
    let text = std::str::from_utf8(content).ok()?;
    let parsed = hafley_scm::lang::rust::parse_rust_file(text).ok()?;
    Some(rust_module_facts_from_parsed(&parsed))
}

/// The module facts off the extract pass's own syn parse, so no second parse.
pub fn rust_module_facts_from_parsed(parsed: &syn::File) -> RustModuleFacts {
    let rows = hafley_scm::lang::rust::module_resolution_rows(parsed);
    let macros = hafley_scm::lang::rust::macro_invocation_rows_from_parsed(parsed);
    rust_module_facts_from_rows(rows, macros)
}

pub fn rust_module_facts_from_tree(tree: &tree_sitter::Tree, source: &[u8]) -> RustModuleFacts {
    let rows = hafley_scm::lang::rust::module_resolution_rows_from_tree(tree, source);
    let macros = hafley_scm::lang::rust::macro_invocation_rows_from_tree(tree, source);
    rust_module_facts_from_rows(rows, macros)
}

fn rust_module_facts_from_rows(
    rows: hafley_scm::lang::rust::ModuleResolutionRows,
    mut macros: Vec<hafley_scm::lang::rust::MacroInvocationRow>,
) -> RustModuleFacts {
    macros.sort_by_key(|row| (row.range.start, row.range.end));
    RustModuleFacts {
        uses: rows
            .uses
            .into_iter()
            .map(|row| UseBinding {
                offset: row.offset,
                local: row.local,
                qualifier: row.qualifier,
                asked: row.asked,
                reexport: row.reexport,
            })
            .collect(),
        stars: rows
            .stars
            .into_iter()
            .map(|row| StarImport {
                offset: row.offset,
                qualifier: row.qualifier,
                reexport: row.reexport,
            })
            .collect(),
        mod_decls: rows.mod_decls,
        aliases: rows
            .aliases
            .into_iter()
            .map(|range| Span {
                start: range.start,
                len: range.end - range.start,
            })
            .collect(),
        macro_invocations: macros
            .into_iter()
            .map(|row| {
                (
                    Span {
                        start: row.range.start,
                        len: row.range.end - row.range.start,
                    },
                    row.name,
                )
            })
            .collect(),
    }
}
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ResolvedImportKind {
    Local,
    Indirect,
    Star,
    Namespace,
    Module,
}

impl ResolvedImportKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            ResolvedImportKind::Local => "local",
            ResolvedImportKind::Indirect => "indirect",
            ResolvedImportKind::Star => "star",
            ResolvedImportKind::Namespace => "namespace",
            ResolvedImportKind::Module => "module",
        }
    }
}

/// One `use` binding resolved to a corpus declaration (or a whole module for
/// a namespace binding). What `Resolve<CallF>`/`Resolve<TypeF>` bind through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedImport {
    pub local: String,
    /// The name asked of the source module; `"*"` for a namespace binding.
    pub name: String,
    pub target_path: String,
    pub target_blob: ContentId,
    /// `Span::anchor(0)` for a namespace binding: no one def span applies.
    pub target_span: Span,
    pub target_name: Option<String>,
    pub kind: ResolvedImportKind,
    pub hops: u32,
}

/// The `resolved_import` wire row: `ResolvedImport` with blob/span dropped.
pub struct ImportRow {
    pub local: String,
    pub name: String,
    pub target_path: String,
    pub target_name: Option<String>,
    pub kind: ResolvedImportKind,
    pub hops: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleCallTarget {
    Target(ContentId, Span),
    /// The prefix names a module no corpus file spells: an external crate.
    External,
    Miss,
}
