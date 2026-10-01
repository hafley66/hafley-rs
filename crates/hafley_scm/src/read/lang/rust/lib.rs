//! The Rust extractor arm reuses one tree-sitter parse for CST, type, call,
//! dataflow, and module projections.
//! Type edges ride `TypeFAux` candidates out of the one parse (port of v5
//! `edges_from`: field/variant/generic/impl — v5 rust emits NO param/returns
//! and NO uses). Resolve<CallF> is NameResolve primary, ScipOverride on scip
//! disagreement; the rust-analyzer `local `-symbol adaptation is documented on
//! the arm. Df argument slots, parameter positions, field names and literal
//! texts are emitted.
//!
//! Spans are UTF-8 byte offsets: syn spans through `byte_range()`, tree-sitter
//! nodes through their byte ranges.

use hafley_scm::lang::rust::{
    call_metadata_rows_from_tree, call_site_rows_from_tree, fast_file_query,
    CallDefinitionKind, RustFastFile, RUST_FAST_QUERY,
};
use std::collections::BTreeSet;

use super::rust_checker::CheckerAnswer;
use super::rust_type_edges::edge_candidates_from_tree;
use crate::read::family::{
    CallEdgeKind, CallF, CallKind, CallSite, ConstKind, ConstValue, DfArg, DfEdgeKind, DfF,
    DfField, DfLit, DfNodeKind, DfParam, DocFact, DocTag, MethodOwner, ProjectEdge,
    ResolutionOrigin, SigSlot, Specifier, SpecifierKind, TypeEdgeCandidate, TypeEdgeKind,
    TypeEntityKind, TypeF, TypeSig,
};
use crate::read::lang::cst_bundle::cst_bundle_from_tree;
use crate::read::rows::{Edge, FamilyBundle, Node};
use crate::read::scip::{byte_range_cached, definition_of, join_documents, site_occurrence};
use crate::read::seams::{
    containing_def_site, corpus_defs, covering_def, own_blob, DefIndex, Resolve,
};
use crate::read::shape::{ContentId, FamilyTag, NodeRef, Span, Strings, ZERO_CONTENT_ID};
use crate::read::source::{FamilyMask, ProjectCx, RyiOutput, Source};
use crate::read::types::ResolveDrop;

use crate::read::trace;
use crate::read::types::LangKind;
use crate::read::types::ScipIndex;
use crate::read::types::{
    DefSite, MacroSite, MacroSiteSource, PathIndex, ReceiverOutcome,
    UnresolvedReason,
};

pub use hafley_scm::lang::rust::{build_line_starts, syn_span};

#[path = "1_type.rs"]
mod type_facts;
use type_facts::{import_bound_target, project_types};

#[path = "2_call.rs"]
mod call_facts;
pub use call_facts::{call_drops, own_blob_probes};
pub use call_facts::{crate_root_of, module_segments, module_target, ModuleTarget};
use call_facts::{project_call, scm_call_defs, splice_macro_expansions};

#[path = "3_df.rs"]
mod df;
use df::project_df;

// ════════════════════════════════════════════════════════════════════════════
// RustSource: the Rust Source, with fast projections over a shared tree-sitter parse.
//
// The masked shape reuses one parse for each requested family and shares one
// `Strings` table across the output.
// ════════════════════════════════════════════════════════════════════════════

/// The Rust `Source`. `matches` = the path ends in `.rs`. CST uses the shared
/// grammar; type/call/df/module facts reuse the same tree. Expanded-call rows
/// are produced by SCM and mapped back to source bytes.
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
    fn planes(&self) -> crate::read::source::FamilyMask {
        crate::read::source::FamilyMask {
            cst: true,
            types: true,
            call: true,
            df: true,
            data: false,
        }
    }

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

        // One SCM++ Rust front-end call parses and queries the source once.
        let parsed = if mask.cst || mask.types || mask.call || mask.df {
            RustFastFile::extract(path, content)
        } else {
            None
        };
        let tree = parsed.as_ref().map(RustFastFile::tree);
        let scm_arena = parsed.as_ref().map(RustFastFile::arena);

        // cst via the linked tree-sitter grammar (masked, one hafley_scm walk).
        // A refused parse leaves cst None (no panic).
        let cst = if mask.cst {
            let span = trace::family_span("rust", "cst");
            let _entered = span.enter();
            let bundle = tree
                .as_ref()
                .and_then(|tree| cst_bundle_from_tree(path, content, tree, &mut strings));
            if let Some(bundle) = &bundle {
                trace::record_bundle(&span, bundle, 0);
            }
            bundle
        } else {
            None
        };

        // Type/call/dataflow/module facts share the caller's Rust tree. Refuse
        // error-recovery trees to retain the previous parse-failure shape.
        let mut types = None;
        let mut call = None;
        let mut df = None;
        let mut rust_module = None;
        if mask.types || mask.call || mask.df {
            if let (Ok(src), Some(tree)) = (std::str::from_utf8(content), tree.as_ref()) {
                if !tree.root_node().has_error() {
                    rust_module = Some(super::rust_modules::rust_module_facts_from_tree(
                        tree, content,
                    ));
                    if mask.types {
                        let span = trace::family_span("rust", "type");
                        let _entered = span.enter();
                        let mut bundle = FamilyBundle::<TypeF>::default();
                        project_types(tree, content, &mut strings, &mut bundle);
                        trace::record_bundle(&span, &bundle, 0);
                        types = Some(bundle);
                    }
                    if mask.call {
                        let span = trace::family_span("rust", "call");
                        let _entered = span.enter();
                        let mut bundle = FamilyBundle::<CallF>::default();
                        if let Some(arena) = scm_arena {
                            scm_call_defs(
                                fast_file_query(),
                                content,
                                arena,
                                &mut strings,
                                &mut bundle,
                            );
                        }
                        project_call(tree, content, &mut strings, &mut bundle);
                        splice_macro_expansions(src, &mut strings, &mut bundle);
                        trace::record_bundle(&span, &bundle, bundle.aux.sites.len());
                        call = Some(bundle);
                    }
                    if mask.df {
                        let span = trace::family_span("rust", "df");
                        let _entered = span.enter();
                        let mut bundle = FamilyBundle::<DfF>::default();
                        project_df(tree, path, content, &mut strings, &mut bundle);
                        trace::record_bundle(&span, &bundle, 0);
                        df = Some(bundle);
                    }
                }
            }
        }

        let scm_captures = scm_arena.as_ref().map(|arena| {
            super::scm_rows::ScmCaptures::from_arena(fast_file_query(), arena, content)
        });

        RyiOutput {
            strings,
            cst,
            types,
            call,
            df,
            data: None,
            scm_captures,
            #[cfg(feature = "kotlin")]
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
