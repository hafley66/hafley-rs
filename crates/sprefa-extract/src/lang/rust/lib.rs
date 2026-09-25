//! The Rust extractor arm: syn front-end for type/call/df/const, the shared
//! tree-sitter walk for cst. Mirrors TsSource (same shape, different front-end): cst via the shared walk
//! grammar + one SCM-owned `syn` parse feeding the type/call/df/const projections.
//! Type edges ride `TypeFAux` candidates out of the one parse (port of v5
//! `edges_from`: field/variant/generic/impl — v5 rust emits NO param/returns
//! and NO uses). Resolve<CallF> is NameResolve primary, ScipOverride on scip
//! disagreement; the rust-analyzer `local `-symbol adaptation is documented on
//! the arm. Df argument slots, parameter positions, field names and literal
//! texts are emitted.
//!
//! Span bridge: syn's proc_macro2 spans are line/col; v6 `Span` is byte offsets,
//! so one `line_starts` table + `line_col_to_byte` converts (the rust-specific
//! bit oxc gives for free). v5's `rust_line` used `span.start().line`; the
//! parity oracle (v5_normalize) reconstructs the byte as `line_starts[line-1] +
//! col`, which is exactly `line_col_to_byte`.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use hafley_scm::lang::rust::{
    call_metadata_rows, call_site_rows, line_col_to_byte, parse_rust_syntax,
    rust_combined_query,
    CallDefinitionKind, RUST_CALL_QUERY, RUST_FAST_QUERY,
};

use super::fallback::cst_bundle_from_tree;
use super::rust_checker::CheckerAnswer;
use super::rust_type_edges::edge_candidates;
use crate::family::{
    CallEdgeKind, CallF, CallKind, CallSite, ConstKind, ConstValue, DfArg, DfEdgeKind, DfF,
    DfField, DfLit, DfNodeKind, DfParam, DocFact, DocTag, MethodOwner, ProjectEdge,
    ResolutionOrigin, SigSlot, Specifier, SpecifierKind, TypeEdgeCandidate, TypeEdgeKind,
    TypeEntityKind, TypeF, TypeSig,
};
use crate::project::ResolveDrop;
use crate::rows::{Edge, FamilyBundle, Node};
use crate::scip::{byte_range_cached, definition_of, join_documents, site_occurrence};
use crate::seams::{
    containing_def_site, corpus_defs, covering_def, def_named, own_blob, DefIndex,
    Resolve,
};
use crate::shape::{ContentId, FamilyTag, NodeRef, Span, Strings, ZERO_CONTENT_ID};
use crate::source::{FamilyMask, ProjectCx, RyiOutput, Source};

use crate::trace;
use crate::types::LangKind;
use crate::types::ScipIndex;
use crate::types::{
    CfgScope, DefSite, MacroSite, MacroSiteSource, PathIndex, ReceiverOutcome, TestOnlyCall,
    UnresolvedReason,
};

// ── span bridge: proc_macro2 line/col -> v6 byte Span ───────────────────────
pub(crate) use hafley_scm::lang::rust::build_line_starts;

/// A proc_macro2 span -> v6 byte Span. Used for entity/def spans where a real
/// length is kept (joins + future resolution); df nodes use start-only anchors.
pub(crate) fn syn_span(line_starts: &[u32], span: proc_macro2::Span) -> Span {
    let start = span.start();
    let end = span.end();
    let start_byte = line_col_to_byte(line_starts, start.line as u32, start.column as u32);
    let end_byte = line_col_to_byte(line_starts, end.line as u32, end.column as u32);
    Span {
        start: start_byte,
        len: end_byte.saturating_sub(start_byte),
    }
}

#[path = "1_type.rs"]
mod type_facts;
use type_facts::{import_bound_target, project_types};

#[path = "2_call.rs"]
mod call_facts;
pub(crate) use call_facts::{crate_root_of, module_segments, module_target};
pub use call_facts::{call_drops, own_blob_probes};
use call_facts::{project_call, scm_call_defs, splice_macro_expansions};

#[path = "3_df.rs"]
mod df;
use df::project_df;

// ════════════════════════════════════════════════════════════════════════════
// RustSource: the Rust Source (cst via the shared tree-sitter walk + type/call/df via syn).
//
// The two-parser, masked shape (mirrors TsSource). cst runs through the shared
// (one dep = the CST floor for every lang); type/call/df run through ONE syn
// parse (three masked projections over the same tree). ONE shared `Strings`
// across all four families.
// ════════════════════════════════════════════════════════════════════════════

fn rust_call_query() -> &'static hafley_scm::QueryExt {
    static QUERY: LazyLock<hafley_scm::QueryExt> = LazyLock::new(|| {
        let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
        hafley_scm::build(&language, RUST_CALL_QUERY).expect("rust call query builds")
    });
    &QUERY
}

fn rust_combined_query_ext() -> &'static hafley_scm::QueryExt {
    static QUERY: LazyLock<hafley_scm::QueryExt> = LazyLock::new(|| {
        let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
        hafley_scm::build(&language, &rust_combined_query()).expect("rust combined query builds")
    });
    &QUERY
}

/// The Rust `Source`. `matches` = the path ends in `.rs`. CST uses the shared
/// grammar; type/call/df/const reuse one SCM-owned `syn` parse. Expanded-call
/// rows are produced by SCM and mapped back to source bytes.
#[derive(Default)]
pub struct RustSource;

/// Kinds only Rust constructs: the core enums do not carry them (tests/6_kind_vocab.rs).
pub const TRAIT: TypeEntityKind = TypeEntityKind::Ext(LangKind {
    lang: "rust",
    tag: "trait",
});
pub const BORROW: DfNodeKind = DfNodeKind::Ext(LangKind {
    lang: "rust",
    tag: "borrow",
});
pub const BREAK: DfNodeKind = DfNodeKind::Ext(LangKind {
    lang: "rust",
    tag: "break",
});
pub const MATCH: DfNodeKind = DfNodeKind::Ext(LangKind {
    lang: "rust",
    tag: "match",
});
pub const BLOCK: DfNodeKind = DfNodeKind::Ext(LangKind {
    lang: "rust",
    tag: "block",
});
/// A `const`/`static` item that owns calls in its initializer. Not `Free`: it
/// is a caller and never a callee, and no other language has the shape.
pub const CONST_INIT: CallKind = CallKind::Ext(LangKind {
    lang: "rust",
    tag: "const_init",
});

impl Source for RustSource {
    fn name(&self) -> &'static str {
        "rust"
    }

    fn matches(&self, path: &str) -> bool {
        path.ends_with(".rs")
    }

    fn scm_query(&self, _path: &str) -> Option<&'static str> {
        Some(RUST_FAST_QUERY)
    }

    fn extract(&self, path: &str, content: &[u8], mask: FamilyMask) -> RyiOutput {
        let mut strings = Strings::new();

        // One tree backs the named CST walk, call definitions, and fast rows.
        // The syn projections below retain their existing separate parse.
        let tree = if mask.cst || mask.types || mask.call || mask.df {
            let parse_span = trace::parse_span("rust", "tree-sitter");
            let _parse_guard = parse_span.enter();
            std::str::from_utf8(content).ok().and_then(|_| {
                let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
                hafley_scm::cst::parse(&language, content)
            })
        } else {
            None
        };
        let scm_arena = tree.as_ref().map(|tree| {
            let query = rust_combined_query_ext();
            let mut arena = hafley_scm::MatchArena::default();
            hafley_scm::run(query, path, content, tree, u32::MAX, &mut arena)
                .expect("rust combined query stays within the engine match limit");
            arena
        });

        // cst via the linked tree-sitter grammar (masked, one hafley_scm walk).
        // A refused parse leaves cst None (no panic).
        let cst = if mask.cst {
            let span = trace::family_span("rust", "cst");
            let _entered = span.enter();
            let bundle = tree.as_ref().and_then(|tree| {
                cst_bundle_from_tree(path, content, tree, &mut strings)
            });
            if let Some(bundle) = &bundle {
                trace::record_bundle(&span, bundle, 0);
            }
            bundle
        } else {
            None
        };

        // type/call/df via ONE syn parse (masked). Owns no arena (syn::File is
        // owned); the line_starts table bridges proc_macro2 line/col to byte
        // spans once, shared across the masked projections. A failed parse leaves
        // all three None (partial output: cst above may still be Some).
        let mut types = None;
        let mut call = None;
        let mut df = None;
        let mut rust_module = None;
        if mask.types || mask.call || mask.df {
            if let Ok(src) = std::str::from_utf8(content) {
                let parsed = {
                    let span = trace::parse_span("rust", "syn");
                    let _entered = span.enter();
                    parse_rust_syntax(src)
                };
                if let Ok(parsed) = parsed {
                    let line_starts = &parsed.line_starts;
                    rust_module = Some(super::rust_modules::rust_module_facts_from_parsed(&parsed.file, line_starts));
                    if mask.types {
                        let span = trace::family_span("rust", "type");
                        let _entered = span.enter();
                        let mut bundle = FamilyBundle::<TypeF>::default();
                        project_types(&parsed.file, line_starts, &mut strings, &mut bundle);
                        trace::record_bundle(&span, &bundle, 0);
                        types = Some(bundle);
                    }
                    if mask.call {
                        let span = trace::family_span("rust", "call");
                        let _entered = span.enter();
                        let mut bundle = FamilyBundle::<CallF>::default();
                        if let Some(arena) = scm_arena.as_ref() {
                            scm_call_defs(
                                rust_combined_query_ext(),
                                content,
                                arena,
                                &mut strings,
                                &mut bundle,
                            );
                        }
                        project_call(&parsed.file, line_starts, &mut strings, &mut bundle);
                        splice_macro_expansions(src, &mut strings, &mut bundle);
                        trace::record_bundle(&span, &bundle, bundle.aux.sites.len());
                        call = Some(bundle);
                    }
                    if mask.df {
                        let span = trace::family_span("rust", "df");
                        let _entered = span.enter();
                        let mut bundle = FamilyBundle::<DfF>::default();
                        project_df(&parsed.file, path, src, line_starts, &mut strings, &mut bundle);
                        trace::record_bundle(&span, &bundle, 0);
                        df = Some(bundle);
                    }
                }
            }
        }

        let scm_captures = scm_arena.as_ref().map(|arena| {
            super::scm_rows::ScmCaptures::from_arena(rust_combined_query_ext(), arena, content)
        });

        RyiOutput {
            strings,
            cst,
            types,
            call,
            df,
            data: None,
            scm_captures,
            kotlin_module: None,
            rust_module,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsed_rust_carries_module_facts_into_resolve() {
        let source = "mod inner { pub fn run() {} }\nuse inner::run;\n";
        let output = RustSource.extract("sample.rs", source.as_bytes(), FamilyMask::DEFAULT);
        assert!(output.rust_module.is_some());
    }
}
