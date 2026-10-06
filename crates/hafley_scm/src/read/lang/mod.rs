//! The language roster. First-match (v5 `type_langs()`, typegraph/mod.rs:491):
//! the lang-specific `Source` precedes the shared tree-sitter fallback. A `.rs`
//! hits `RustSource` (cst via the shared tree-sitter walk + type/call/df via
//! syn); a `.go` hits `GoSource` (cst likewise + type/call/df via
//! tree-sitter-go); a `.kt`/`.kts` hits `KotlinSource` (cst likewise +
//! type/call/df via tree-sitter-kotlin); a `.py`/`.pyi` hits `PythonSource`
//! (cst likewise + type/call/df via tree-sitter-python); a `.ts` hits `TsSource`
//! (cst likewise + type/call/df via oxc); anything else with a linked grammar
//! falls to `FallbackSource` (cst-only).

use std::collections::HashMap;
use std::time::Duration;

use crate::read::shape::FamilyTag;
use crate::read::tsi::FactOut;
use crate::read::types::{ContentId, DefIndex, DefSite};
use crate::span::Span;

const CALL_FACETS: &[FamilyTag] = &[FamilyTag::Call, FamilyTag::Type];
const TYPE_FACETS: &[FamilyTag] = &[FamilyTag::Type, FamilyTag::Call];

/// One resolved reference shared by checker tiers with the same wire shape.
#[derive(Clone, Debug)]
pub struct CheckerRef {
    pub start: u32,
    pub end: u32,
    pub name: String,
    pub dst_path: String,
    pub dst_name: String,
    pub dst_offset: u32,
}

/// The checker's resolution for a reference into or outside the corpus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckerAnswer {
    Corpus(ContentId, Span),
    /// Resolved outside the corpus; carries the crate-qualified path.
    External(String),
}

fn answer_of(
    reference: (&str, &str, u32),
    facets: &[FamilyTag],
    blob_of: &HashMap<&str, &ContentId>,
    defs: &DefIndex,
) -> Option<CheckerAnswer> {
    let (dst_path, dst_name, dst_offset) = reference;
    if dst_path.is_empty() {
        return Some(CheckerAnswer::External(dst_name.to_string()));
    }
    let blob = *blob_of.get(dst_path)?;
    let sites = defs.map.get(dst_name)?;
    facets.iter().find_map(|facet| {
        let in_file: Vec<&DefSite> = sites
            .iter()
            .filter(|site| &site.blob == blob && site.family == *facet)
            .collect();
        let covering = in_file
            .iter()
            .find(|site| site.span.start <= dst_offset && dst_offset < site.span.end());
        let chosen = match covering {
            Some(site) => *site,
            None if in_file.len() == 1 => in_file[0],
            None => return None,
        };
        Some(CheckerAnswer::Corpus(chosen.blob.clone(), chosen.span))
    })
}

struct CheckerBound {
    start: u32,
    end: u32,
    name: String,
    answer: CheckerAnswer,
}

/// The driver output shared by checker tiers with the same answer shape.
#[derive(Default)]
pub struct CheckerAnswers {
    pub version: String,
    pub calls: HashMap<String, Vec<CheckerRef>>,
    pub types: HashMap<String, Vec<CheckerRef>>,
    pub tsi: Vec<FactOut>,
    pub coverage: Vec<(String, bool, Option<String>)>,
    pub load: Duration,
    pub walk: Duration,
    pub files_answered: usize,
}

#[path = "0_call_kinds.rs"]
pub mod call_kinds;
#[cfg(feature = "commonlisp")]
pub mod commonlisp;
#[path = "1_cst_bundle.rs"]
pub mod cst_bundle;
#[cfg(feature = "data")]
pub mod data;
pub mod extract_lang;
pub mod fact;
#[cfg(feature = "fallback")]
pub mod fallback;
#[cfg(feature = "gdscript")]
pub mod gdscript;
#[cfg(feature = "go")]
pub mod go;
#[cfg(feature = "go")]
pub mod go_checker;
#[cfg(feature = "go")]
pub mod go_modules;
#[cfg(feature = "go")]
pub mod go_type_edges;
#[cfg(feature = "kotlin")]
pub mod kotlin;
#[cfg(feature = "kotlin")]
pub mod kotlin_modules;
#[cfg(feature = "kotlin")]
pub mod kotlin_receivers;
#[cfg(feature = "kotlin")]
pub mod kotlin_type_edges;
#[cfg(feature = "markdown")]
pub mod markdown;
#[path = "4_owned_region.rs"]
pub mod owned_region;
#[cfg(feature = "prolog")]
pub mod prolog;
#[cfg(feature = "python")]
pub mod python;
#[cfg(feature = "rust")]
#[path = "rust/lib.rs"]
pub mod rust;
#[cfg(feature = "rust")]
pub mod rust_checker;
#[cfg(feature = "rust-checker")]
mod rust_checker_ra;
#[cfg(feature = "rust-checker")]
#[path = "7a_rust_checker_project.rs"]
mod rust_checker_project;
#[cfg(feature = "rust-checker")]
#[path = "8_rust_checker_session.rs"]
mod rust_checker_session;
#[cfg(feature = "cargo-metadata")]
#[path = "0_rust_workspace.rs"]
pub mod rust_workspace;
#[cfg(feature = "rust")]
pub mod rust_modules;
#[cfg(feature = "rust")]
pub mod rust_receivers;
#[cfg(feature = "rust")]
pub mod rust_scip_macros;
#[cfg(feature = "rust")]
pub mod rust_type_edges;
#[cfg(feature = "rust")]
pub mod rust_type_refs;
#[path = "6_scm_family.rs"]
mod scm_family;
#[path = "7_scm_rows.rs"]
pub mod scm_rows;
#[path = "8c_scope_products.rs"]
mod scope_products;
#[path = "8_scm_store.rs"]
pub mod scm_store;
#[path = "3_source_facts.rs"]
pub mod source_facts;
#[path = "2_source_query.rs"]
pub mod source_query;
#[cfg(feature = "typescript")]
pub mod ts;
#[cfg(feature = "typespec")]
pub mod typespec;
#[cfg(feature = "typescript")]
pub mod ts_checker;
#[cfg(feature = "typescript")]
#[path = "0_ts7_lsp_session.rs"]
pub mod ts7_lsp_session;
#[cfg(feature = "ts-checker")]
#[path = "1_tsgo_rows.rs"]
pub mod tsgo_rows;
#[cfg(feature = "typescript")]
pub mod ts_lib;
#[cfg(feature = "typescript")]
mod ts_packages;
#[cfg(feature = "typescript")]
pub mod ts_paths;
#[cfg(feature = "typescript")]
pub mod ts_receivers;
#[cfg(feature = "typescript")]
pub mod ts_resolve;

#[cfg(feature = "commonlisp")]
pub use commonlisp::CommonlispSource;
#[cfg(feature = "data")]
pub use data::DataSource;
pub use extract_lang::RyiLang;
pub use fact::{
    dl6_db_path, open_dl6_readonly, open_readonly, FactError, FactSet, DL6_DB_RELATIVE_PATH,
};
#[cfg(feature = "fallback")]
pub use fallback::{call_bundle, call_drops, cst_bundle, FallbackSource};
#[cfg(feature = "gdscript")]
pub use gdscript::GdscriptSource;
#[cfg(feature = "go")]
pub use go::GoSource;
#[cfg(feature = "kotlin")]
pub use kotlin::KotlinSource;
#[cfg(feature = "markdown")]
pub use markdown::MarkdownSource;
pub use owned_region::{
    find_owned_region, owned_region_markers, propose_owned_region, OwnedRegion, OwnedRegionError,
    OwnedRegionProposal,
};
#[cfg(feature = "prolog")]
pub use prolog::PrologSource;
#[cfg(feature = "python")]
pub use python::PythonSource;
#[cfg(feature = "rust")]
pub use rust::RustSource;
pub use scm_rows::{scm_edges, scm_facts, ScmEdge, ScmError};
pub use source_facts::{
    query_source_facts, ByteRange, GitBlobFact, SourceCaptureFact, SourceMatchFact, SourcePlace,
    SourceQueryFact, SourceQueryFacts, SourceReplacementFact, SourceRevisionFact,
    SOURCE_FACT_PROTOCOL,
};
pub use source_query::{
    query_source, query_tree_sitter, query_tree_sitter_spans, SourceQuery, SourceQueryError,
    SourceQueryOutput, TreeSitterQuery, TreeSitterQueryMatch, TreeSitterSpannedCapture,
    TreeSitterSpannedMatch,
};
#[cfg(feature = "typescript")]
pub use ts::{
    ts_specifiers, CallProjector, DfProjector, OxcParser, TsSource, TsSpecifier, TypeProjector,
};
#[cfg(feature = "typescript")]
pub use ts_resolve::{respell, TsResolver};
#[cfg(feature = "typespec")]
pub use typespec::TypespecSource;

use crate::read::source::Source;

/// The first-match roster. Order matters: the lang-specific `Source`s precede the
/// linked-grammar CST fallback (v5 `type_langs()` convention). RustSource is first so a
/// `.rs` routes to it, not the cst-only FallbackSource; GoSource precedes
/// FallbackSource so a `.go` routes to it, not the cst-only fallback.
/// KotlinSource precedes TsSource because `"x.kts".ends_with(".ts")` is true -
/// a `.kts` must route to kotlin, not ts (v5 `type_langs()` makes the same
/// order-dependent call, typegraph/mod.rs:488).
/// DataSource precedes FallbackSource so a `.json`/`.yaml` reaches the data plane;
/// it delegates its own cst plane back to FallbackSource, so no row is lost.
/// GdscriptSource/CommonlispSource precede FallbackSource so their rows route a
/// `.gd`/`.lisp` at all. Neither claims a suffix an earlier row owns
/// (`.gd`, `.lisp`, `.lsp`, `.cl`, `.asd`, `.tsp` are unclaimed above).
pub fn sources() -> &'static [&'static dyn Source] {
    &[
        #[cfg(feature = "rust")]
        &RustSource,
        #[cfg(feature = "go")]
        &GoSource,
        #[cfg(feature = "kotlin")]
        &KotlinSource,
        #[cfg(feature = "markdown")]
        &MarkdownSource,
        #[cfg(feature = "prolog")]
        &PrologSource,
        #[cfg(feature = "python")]
        &PythonSource,
        #[cfg(feature = "data")]
        &DataSource,
        #[cfg(feature = "typescript")]
        &TsSource,
        #[cfg(feature = "gdscript")]
        &GdscriptSource,
        #[cfg(feature = "commonlisp")]
        &CommonlispSource,
        #[cfg(feature = "typespec")]
        &TypespecSource,
        #[cfg(feature = "fallback")]
        &FallbackSource,
    ]
}

/// The first `Source` whose `matches(path)` is true, else None.
pub fn source_for(path: &str) -> Option<&'static dyn Source> {
    sources().iter().copied().find(|src| src.matches(path))
}

#[path = "9_rust_names_index.rs"]
pub mod rust_names_index;
