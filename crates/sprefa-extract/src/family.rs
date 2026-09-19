//! S2 family model. Canonical definitions now live in `crate::types`; this
//! module is a re-export so `crate::family::*` import paths keep resolving.
//! `ProjectEdge<F>` (the per-family resolved edge row) rides this shim because
//! `rows.rs` is a frozen seam this increment (commit 4a).
//!
//! ── THE FAMILY VOCABULARY, AND WHAT "DIET" MEANS ────────────────────────────
//!
//! DIET MEANS PARSE TECHNIQUE AND HEURISTICS, NEVER ACTUAL SCIP DATA.
//!
//! Two `--family` names on the CLI are whole-project MODES rather than members
//! of the per-file mask below, and the split between them is the point:
//!
//!   `scip`       REAL SCIP index data. A language's own indexer (rust-analyzer,
//!                scip-typescript, scip-go) runs over the root under a budget,
//!                its index.scip is decoded, and the rows are v5's `scip_*`
//!                relation shapes. Every fact is compiler-resolved.
//!
//!   `diet_scip`  This crate's OWN tree-sitter / oxc / syn front-ends plus
//!                name-match resolution across the supplied file set. No
//!                indexer, no type checker, no index. It is fast and needs no
//!                toolchain, and it is silently wrong wherever a name is
//!                ambiguous corpus-wide: two files defining `helper` make every
//!                unqualified call to `helper` unresolvable, where a real index
//!                resolves it through the import.
//!
//! The four names below (`cst`, `type`, `call`, `df`) are unaffected: they are
//! the per-file extraction mask and every existing caller of them keeps working
//! exactly as before. `--resolve` likewise remains the pre-existing spelling of
//! the pass `diet_scip` labels.

pub use crate::types::{
    flow_edges, CallEdgeKind, CallF, CallFAux, CallKind, CallSite, ConstKind, ConstValue,
    CstEdgeKind, CstF, DfArg, DfEdgeKind, DfF, DfFAux, DfField, DfLit, DfNodeKind, DfParam,
    DocFact, DocTag, Family, FlowEdge, FlowEdgeKind, FlowF, ImplOwner, MethodOwner, ProjectEdge,
    PyBind, PyCallArg, PyDecor, PyDefault, PyParam, PyRetCall, PyReturn, PySubCall,
    ReceiverBinding, ReceiverOutcome, RefPosition, Reference, ResolutionOrigin, SigSlot, Specifier,
    SpecifierKind, TypeEdgeCandidate, TypeEdgeKind, TypeEntityKind, TypeF, TypeFAux, TypeSig,
};

/// THE DEFAULT FAMILY SET IS DERIVED, PER FILE, FROM THE LANGUAGE'S OWN PLANES.
/// A `Source::extract` fills one `Option` bundle per plane it claims; when any
/// non-cst plane answered WITH ROWS, the default families are those planes and
/// the syntax tree is opt-in via `--family cst`. When none did, cst IS the
/// default. The no-rows arm is what keeps a default from collapsing into an
/// empty stream: it covers a cst-only language, a file whose native parse
/// failed (the bundles answer `None`), and a file whose native planes parsed
/// but found nothing (a consts-only rust file parses, so its type/call/df
/// bundles are `Some` and empty). Adding a language never edits this rule:
/// its own extract fills what it fills, and the default follows from the
/// same read.
pub fn default_keeps_cst(output: &crate::types::RyiOutput) -> bool {
    fn answered<F: crate::types::Family>(bundle: &crate::types::FamilyBundle<F>) -> bool {
        !bundle.nodes.is_empty() || !bundle.edges.is_empty()
    }
    // The data plane carries no nodes at all: DataFAux's docs and values are
    // the whole plane, so its rows are counted directly. A type/call/df
    // bundle whose rows were aux-only would keep cst beside them, which errs
    // toward the stream never being empty.
    let data = output
        .data
        .as_ref()
        .is_some_and(|d| !d.aux.docs.is_empty() || !d.aux.values.is_empty());
    !(output.types.as_ref().is_some_and(answered)
        || output.call.as_ref().is_some_and(answered)
        || output.df.as_ref().is_some_and(answered)
        || data)
}
