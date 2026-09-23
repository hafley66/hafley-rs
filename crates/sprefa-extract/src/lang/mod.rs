//! The language roster. First-match (v5 `type_langs()`, typegraph/mod.rs:491):
//! the lang-specific `Source` precedes the shared tree-sitter fallback. A `.rs`
//! hits `RustSource` (cst via the shared tree-sitter walk + type/call/df via
//! syn); a `.go` hits `GoSource` (cst likewise + type/call/df via
//! tree-sitter-go); a `.kt`/`.kts` hits `KotlinSource` (cst likewise +
//! type/call/df via tree-sitter-kotlin); a `.py`/`.pyi` hits `PythonSource`
//! (cst likewise + type/call/df via tree-sitter-python); a `.ts` hits `TsSource`
//! (cst likewise + type/call/df via oxc); anything else with a linked grammar
//! falls to `FallbackSource` (cst-only).

pub mod fallback;
#[path = "0_call_kinds.rs"]
pub mod call_kinds;
pub mod commonlisp;
pub mod data;
pub mod extract_lang;
pub mod fact;
pub mod gdscript;
pub mod go;
pub mod go_checker;
pub mod go_modules;
pub mod go_type_edges;
pub mod kotlin;
pub mod kotlin_modules;
pub mod kotlin_receivers;
pub mod kotlin_rehome;
pub mod kotlin_rename;
pub mod kotlin_type_edges;
pub mod markdown;
#[path = "4_owned_region.rs"]
pub mod owned_region;
pub mod prolog;
pub mod python;
#[path = "rust/lib.rs"] pub mod rust;
pub mod rust_checker;
#[cfg(feature = "rust-checker")]
mod rust_checker_ra;
pub mod rust_docs;
pub mod rust_modules;
#[path = "rust/cleave.rs"]
pub mod rust_mutate;
pub mod rust_receivers;
pub mod rust_rehome;
pub mod rust_rename;
pub mod rust_scip_macros;
pub mod rust_type_edges;
pub mod rust_type_refs;
#[path = "6_scm_family.rs"]
mod scm_family;
#[path = "7_scm_rows.rs"]
pub mod scm_rows;
#[path = "8_scm_store.rs"]
pub mod scm_store;
#[path = "3_source_facts.rs"]
pub mod source_facts;
#[path = "2_source_query.rs"]
pub mod source_query;
pub mod ts;
pub mod ts_checker;
#[path = "ts/cleave.rs"]
pub mod ts_mutate;
pub mod ts_paths;
pub mod ts_receivers;
pub mod ts_rehome;
pub mod ts_rename;
pub mod ts_resolve;

pub use fallback::{call_bundle, call_drops, cst_bundle, FallbackSource};
pub use commonlisp::CommonlispSource;
pub use data::DataSource;
pub use extract_lang::RyiLang;
pub use fact::{
    dl6_db_path, open_dl6_readonly, open_readonly, FactError, FactSet,
    DL6_DB_RELATIVE_PATH,
};
pub use gdscript::GdscriptSource;
pub use go::GoSource;
pub use kotlin::KotlinSource;
pub use markdown::MarkdownSource;
pub use owned_region::{
    find_owned_region, owned_region_markers, propose_owned_region, OwnedRegion, OwnedRegionError,
    OwnedRegionProposal,
};
pub use prolog::PrologSource;
pub use python::PythonSource;
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
pub use ts::{
    ts_specifiers, CallProjector, DfProjector, OxcParser, TsSource, TsSpecifier, TypeProjector,
};
pub use ts_rehome::{build_paths, compiled_spellings, BuildPaths};
pub use ts_resolve::{respell, TsResolver};

use crate::source::Source;
use crate::types::{Cleave, RehomeArm, Rename};

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
/// (`.gd`, `.lisp`, `.lsp`, `.cl`, `.asd` are unclaimed above).
pub fn sources() -> &'static [&'static dyn Source] {
    &[
        &RustSource,
        &GoSource,
        &KotlinSource,
        &MarkdownSource,
        &PrologSource,
        &PythonSource,
        &DataSource,
        &TsSource,
        &GdscriptSource,
        &CommonlispSource,
        &FallbackSource,
    ]
}

/// The first `Source` whose `matches(path)` is true, else None.
pub fn source_for(path: &str) -> Option<&'static dyn Source> {
    sources().iter().copied().find(|src| src.matches(path))
}

/// The `Rehome` roster: one impl per language `extract move` can rehome, in
/// `sources()` order. A language with no impl here is a named stop, never a
/// `match` arm in the move core.
pub fn rehomes() -> &'static [RehomeArm] {
    const ROSTER: [RehomeArm; 4] = [
        RehomeArm {
            core: &RustSource,
            manifests: Some(&RustSource),
            shim: None,
            text_spellings: None,
            plan_check: Some(&RustSource),
        },
        RehomeArm {
            core: &KotlinSource,
            manifests: None,
            shim: None,
            text_spellings: None,
            plan_check: None,
        },
        RehomeArm {
            core: &PrologSource,
            manifests: None,
            // Disabled: the shim leg rode the YAML rule engine.
            shim: None,
            text_spellings: None,
            plan_check: None,
        },
        RehomeArm {
            core: &TsSource,
            manifests: Some(&TsSource),
            shim: None,
            text_spellings: Some(&TsSource),
            plan_check: None,
        },
    ];
    &ROSTER
}

/// The `Rehome` that owns `path`, under the SAME first-match law `sources()`
/// states: `"x.kts".ends_with(".ts")` is true, so `TsSource` matches a kotlin
/// script too and only `source_for`'s own winner may claim it.
pub fn rehome_for(path: &str) -> Option<&'static RehomeArm> {
    let owner = source_for(path)?.name();
    rehomes().iter().find(|arm| arm.name() == owner)
}

/// The `Rename` roster, in `sources()` order. Membership is "has a scope plane
/// with exact identifier spans", a different question from `rehomes()`'s.
pub fn renames() -> &'static [&'static dyn Rename] {
    &[&TsSource, &RustSource, &KotlinSource, &PrologSource]
}

/// The `Rename` that owns `path`, under the SAME first-match law `rehome_for`
/// states: only `source_for`'s own winner may claim a path.
pub fn rename_for(path: &str) -> Option<&'static dyn Rename> {
    let owner = source_for(path)?.name();
    renames().iter().copied().find(|arm| arm.name() == owner)
}

/// The `Cleave` roster, in `sources()` order. Membership is "this language can
/// be text-edited by a verb", a third question again from `rehomes()`'s.
pub fn cleaves() -> &'static [&'static dyn Cleave] {
    &[&RustSource, &TsSource]
}

/// The `Cleave` that owns `path`, under the SAME first-match law `rehome_for`
/// states: only `source_for`'s own winner may claim a path.
pub fn cleave_for(path: &str) -> Option<&'static dyn Cleave> {
    let owner = source_for(path)?.name();
    cleaves().iter().copied().find(|arm| arm.name() == owner)
}
