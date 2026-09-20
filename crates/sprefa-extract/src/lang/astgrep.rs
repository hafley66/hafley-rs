//! The ast-grep Parser + the CstF projection.
//!
//! ast-grep-language ships the tree-sitter grammars for rust/ts/tsx/js/go in
//! ONE dep; `SupportLang::from_path` picks the grammar from the path. The parse
//! goes through `AstGrep::new` (the library, never a subprocess), so this abides
//! the trait seams. The CstF walk stays inside ast-grep's `Node` API
//! (`is_named` / `kind` / `range` / `children`): a port of v5
//! `src/cst.rs::walk_cst`, iterative pre-order DFS, named nodes only, unnamed
//! nodes reparenting their named descendants to the nearest named ancestor.

use crate::lang::extract_lang::RyiLang;
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_core::Language as _;
use ast_grep_core::{AstGrep, Node as SgNode, Pattern};
use ast_grep_language::SupportLang;
use serde::Serialize;

use crate::family::{
    CallEdgeKind, CallF, CallSite, CstEdgeKind, CstF, ProjectEdge, ResolutionOrigin,
};
use crate::rows::{Edge, FamilyBundle, Node};
use crate::seams::{corpus_defs, covering_def, DefIndex, ParseError, Parser, Project, Resolve};
use crate::shape::{ContentId, FamilyTag, NameId, NodeRef, Span, Strings};
use crate::source::{FamilyMask, ProjectCx, RyiOutput, Source};
use crate::trace;
use std::collections::BTreeSet;

use crate::lang::call_kinds::{
    ARG_KINDS, CALLEE_FIRST_KINDS, CALLEE_NAME_KINDS, CALL_KINDS, NAME_LEAF_KINDS,
};
use crate::lang::python::MODULE_CALLER;
use crate::project::ResolveDrop;
use crate::types::UnresolvedReason;

/// The owned ast-grep root: owns its source `String` + the tree-sitter `Tree`.
/// `Send`; the borrowed `Node<'r>` is not, so projection (which walks it) runs on
/// the thread that owns the root. (v5 `src/sg.rs`: `type SgRoot =
/// AstGrep<StrDoc<RyiLang>>`.)
pub type SgRoot = AstGrep<StrDoc<RyiLang>>;

/// One generic ast-grep pattern and the single-node captures the caller wants
/// flattened. Query identity belongs to the caller's program; the extractor
/// only parses, matches, and returns byte-addressed facts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AstPatternQuery {
    pub id: String,
    pub pattern: String,
    pub selector: Option<String>,
    pub captures: Vec<String>,
}

/// One capture from one pattern match. Every field is top-level so JSONL host
/// projections remain flat. The whole-match span lets programs group captures
/// from the same match and join nested matches by containment.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AstCaptureFact {
    pub record: &'static str,
    pub query: String,
    pub capture: String,
    pub text: String,
    pub start: u32,
    pub end: u32,
    pub match_start: u32,
    pub match_end: u32,
}

/// Run several patterns over one parsed source root and flatten only the
/// requested single-node captures. Pattern matching stays inside the extractor;
/// framework meaning is derived by ordinary relations over these rows.
pub fn query_patterns(
    path: &str,
    content: &[u8],
    queries: &[AstPatternQuery],
) -> Result<Vec<AstCaptureFact>, ParseError> {
    let lang = RyiLang::from_path(path).ok_or_else(|| ParseError::NoGrammar(path.to_string()))?;
    let source =
        std::str::from_utf8(content).map_err(|error| ParseError::Utf8(error.to_string()))?;
    let root = AstGrep::new(source, lang);
    let mut facts = Vec::new();

    for query in queries {
        let pattern = match &query.selector {
            Some(selector) => Pattern::contextual(&query.pattern, selector, lang),
            None => Pattern::try_new(&query.pattern, lang),
        }
        .map_err(|error| {
            ParseError::Parse(format!("ast pattern '{}' failed: {error}", query.id))
        })?;
        let defined = pattern.defined_vars();
        for capture in &query.captures {
            if !defined.contains(capture.as_str()) {
                return Err(ParseError::Parse(format!(
                    "ast pattern '{}' does not define capture '{capture}'",
                    query.id
                )));
            }
        }
        for matched in root.root().find_all(&pattern) {
            let match_range = matched.range();
            for capture in &query.captures {
                let Some(node) = matched.get_env().get_match(capture) else {
                    continue;
                };
                let range = node.range();
                facts.push(AstCaptureFact {
                    record: "capture",
                    query: query.id.clone(),
                    capture: capture.clone(),
                    text: node.text().to_string(),
                    start: range.start as u32,
                    end: range.end as u32,
                    match_start: match_range.start as u32,
                    match_end: match_range.end as u32,
                });
            }
        }
    }

    facts.sort_by(|left, right| {
        (
            &left.query,
            left.match_start,
            left.match_end,
            &left.capture,
            left.start,
            left.end,
            &left.text,
        )
            .cmp(&(
                &right.query,
                right.match_start,
                right.match_end,
                &right.capture,
                right.start,
                right.end,
                &right.text,
            ))
    });
    facts.dedup();
    Ok(facts)
}

/// The Parser: one dep covers rust/ts/tsx/js/go via ast-grep's grammars.
#[derive(Default)]
pub struct AstGrepParser;

impl Parser for AstGrepParser {
    type Arena = ();
    type Parsed<'a> = SgRoot;

    fn name(&self) -> &'static str {
        "ast-grep"
    }

    fn matches(&self, path: &str) -> bool {
        // SupportLang directly: the roster's per-grammar Sources answer ahead of
        // this fallback, and RyiLang::from_path routes through the roster,
        // so asking it here would recurse.
        SupportLang::from_path(path).is_some()
    }

    fn make_arena(&self) {}

    fn parse<'a>(
        &self,
        _arena: &'a (),
        path: &str,
        content: &'a [u8],
    ) -> Result<SgRoot, ParseError> {
        let lang =
            RyiLang::from_path(path).ok_or_else(|| ParseError::NoGrammar(path.to_string()))?;
        let src = std::str::from_utf8(content).map_err(|err| ParseError::Utf8(err.to_string()))?;
        Ok(AstGrep::new(src, lang))
    }
}

/// The kind tables resolved to ids against ONE parsed root's own grammar
/// (`Language::kind_to_id`, once per `project`, never per node). A name the
/// grammar does not declare resolves to 0, the absent mark: dropped at
/// resolve time, it can never match a node of that grammar.
struct RootKinds {
    name_leaves: Vec<u16>,
    args: Vec<u16>,
}

impl RootKinds {
    fn resolve(root: &SgRoot) -> Self {
        let ids = |names: &[&str]| {
            names
                .iter()
                .map(|kind| root.root().lang().kind_to_id(kind))
                .filter(|id| *id != 0)
                .collect()
        };
        Self {
            name_leaves: ids(NAME_LEAF_KINDS),
            args: ids(ARG_KINDS),
        }
    }

    fn is_name_leaf_kind(&self, id: u16) -> bool {
        self.name_leaves.contains(&id)
    }

    fn is_arg(&self, id: u16) -> bool {
        self.args.contains(&id)
    }
}

/// A named node whose kind is one of the root grammar's name-leaf kinds
/// (`NAME_LEAF_KINDS`, resolved per root) with no named children.
fn is_identifier_leaf(node: &SgNode<StrDoc<RyiLang>>, kinds: &RootKinds) -> bool {
    kinds.is_name_leaf_kind(node.kind_id()) && node.children().all(|child| !child.is_named())
}

/// The CstF projector: walks the parsed ast-grep tree, emitting one row per
/// named node + a `Child` edge to its nearest named ancestor.
#[derive(Default)]
pub struct CstProjector;

impl Project<CstF> for CstProjector {
    type Parsed<'a> = SgRoot;

    fn project(&self, root: &SgRoot, strings: &mut Strings, sink: &mut FamilyBundle<CstF>) {
        let kinds = RootKinds::resolve(root);
        // Iterative pre-order DFS. Stack entries carry the node + the index of
        // its nearest named ancestor (None at the root). Unnamed punctuation
        // nodes emit no row but pass `nearest_named` through so their named
        // descendants attach to the nearest named ancestor. Children are pushed
        // in reverse so they pop in source order. (Port of v5 walk_cst.)
        let mut stack: Vec<(SgNode<StrDoc<RyiLang>>, Option<NodeRef>)> = vec![(root.root(), None)];
        while let Some((node, nearest_named)) = stack.pop() {
            let my_named = if node.is_named() {
                let byte_range = node.range();
                let span = Span {
                    start: byte_range.start as u32,
                    len: (byte_range.end - byte_range.start) as u32,
                };
                let ix = NodeRef(sink.nodes.len() as u32);
                let kind_text = node.kind();
                let kind = strings.intern(&kind_text);
                // Name = the grammar's `name:` field, else an identifier leaf's own text.
                let mut row = Node::new(span, kind);
                row.name = match node.field("name") {
                    Some(field) => Some(strings.intern(&field.text())),
                    None if is_identifier_leaf(&node, &kinds) => {
                        Some(strings.intern(&node.text()))
                    }
                    None => None,
                };
                sink.nodes.push(row);
                if let Some(parent_ix) = nearest_named {
                    // child edge: parent -> child (v5 `child(parent, child)`).
                    sink.edges
                        .push(Edge::new(parent_ix, ix, CstEdgeKind::Child));
                }
                Some(ix)
            } else {
                nearest_named
            };
            // Pushed forward then reversed IN PLACE on the stack's own tail: a
            // per-node `children().collect()` is one heap allocation per node.
            let mark = stack.len();
            for child in node.children() {
                stack.push((child, my_named));
            }
            stack[mark..].reverse();
        }
    }
}

/// A named leaf whose kind carries a name: the identifier kinds, plus the
/// leaf kinds `0_call_kinds.rs` names (php `name`, haskell `variable`, bash
/// `word`).
fn is_name_leaf(node: &SgNode<StrDoc<RyiLang>>, kinds: &RootKinds) -> bool {
    node.is_named()
        && node.children().all(|child| !child.is_named())
        && (kinds.is_name_leaf_kind(node.kind_id())
            || CALLEE_NAME_KINDS.contains(&node.kind().as_ref()))
}

/// The name leaves under `node`, pre-order. A nested call kind stops the
/// descent: `foo (helper 1)` names `foo` here, and the nested call names
/// itself when its own turn comes.
fn collect_name_leaves<'r>(
    node: &SgNode<'r, StrDoc<RyiLang>>,
    kinds: &RootKinds,
    out: &mut Vec<SgNode<'r, StrDoc<RyiLang>>>,
) {
    if CALL_KINDS.contains(&node.kind().as_ref()) {
        return;
    }
    if is_name_leaf(node, kinds) {
        out.push(node.clone());
        return;
    }
    for child in node.children() {
        collect_name_leaves(&child, kinds, out);
    }
}

/// The head of a prefix-application chain: the leftmost name leaf, walking
/// the first named child through the nesting. tree-sitter-haskell's `apply`
/// is left-associative, so `map f xs` parses `apply(apply(map, f), xs)` and
/// the head sits at the end of the first-child spine.
fn head_leaf<'r>(node: &SgNode<'r, StrDoc<RyiLang>>) -> Option<SgNode<'r, StrDoc<RyiLang>>> {
    let mut cur = node.clone();
    loop {
        let next = cur.children().find(|child| child.is_named());
        match next {
            Some(child) => cur = child,
            None => break,
        }
    }
    Some(cur)
}

/// One guessed site's callee: the grammar's own seat first (`name`, then
/// `method`, then `function`), resolved to the seat's LAST name leaf, the
/// trailing segment of a member chain (`s.fp(...)` names `fp`). `callee_path`
/// rides the seat text when it says more than the leaf (`s.fp` vs `fp`).
/// No seat, or a seat that names nothing, falls to the generic walk over the
/// children before the argument subtree; `apply`-style kinds take the FIRST
/// leaf (prefix application: `map f xs` names `map`), the rest the last.
/// `None` mints no site: a call with no name to bind is not a row.
fn callee_of(
    node: &SgNode<StrDoc<RyiLang>>,
    kinds: &RootKinds,
    strings: &mut Strings,
) -> Option<(NameId, Option<NameId>)> {
    for field in ["name", "method", "function"] {
        let Some(seat) = node.field(field) else {
            continue;
        };
        let mut leaves = Vec::new();
        collect_name_leaves(&seat, kinds, &mut leaves);
        let Some(leaf) = leaves.last() else {
            continue;
        };
        let callee = strings.intern(&leaf.text());
        let seat_text = seat.text();
        let callee_path = (seat_text != *leaf.text()).then(|| strings.intern(&seat_text));
        return Some((callee, callee_path));
    }
    let mut leaves = Vec::new();
    for child in node.children() {
        if kinds.is_arg(child.kind_id()) {
            continue;
        }
        collect_name_leaves(&child, kinds, &mut leaves);
    }
    if CALLEE_FIRST_KINDS.contains(&node.kind().as_ref()) {
        let head = head_leaf(node)?;
        return Some((strings.intern(&head.text()), None));
    }
    let leaf = leaves.last()?;
    Some((strings.intern(&leaf.text()), None))
}

/// The guessed call projector: one site per node whose kind is in the
/// call-kind table (0_call_kinds.rs), no per-language code anywhere. The
/// sites are ORDINARY `CallSite` rows: resolution binds them through
/// `corpus_unique` or the drop channel says why it did not.
#[derive(Default)]
pub struct CallProjector;

impl Project<CallF> for CallProjector {
    type Parsed<'a> = SgRoot;

    fn project(&self, root: &SgRoot, strings: &mut Strings, sink: &mut FamilyBundle<CallF>) {
        let kinds = RootKinds::resolve(root);
        // The module as nameless covering def, python's MODULE_CALLER seat
        // reused verbatim: a guessed site then has a caller for
        // `Resolve<CallF>`'s covering-def join. flatten_call skips the node,
        // so no def row reaches the wire.
        let file_range = root.root().range();
        sink.nodes.push(Node::new(
            Span {
                start: file_range.start as u32,
                len: (file_range.end - file_range.start) as u32,
            },
            MODULE_CALLER,
        ));
        // The cst walk's discipline: iterative pre-order DFS, children pushed
        // in reverse so they pop in source order and rows stay doc-ordered.
        let mut stack: Vec<SgNode<StrDoc<RyiLang>>> = vec![root.root()];
        while let Some(node) = stack.pop() {
            let kind = node.kind();
            if node.is_named() && CALL_KINDS.contains(&kind.as_ref()) {
                // An application nested in another application is the SAME
                // chain (the left-associative parse), never another call
                // site; only the outermost apply mints.
                let same_chain = CALLEE_FIRST_KINDS.contains(&kind.as_ref())
                    && node
                        .parent()
                        .is_some_and(|p| p.is_named() && CALL_KINDS.contains(&p.kind().as_ref()));
                if !same_chain {
                    if let Some((callee, callee_path)) = callee_of(&node, &kinds, strings) {
                        let byte_range = node.range();
                        sink.aux.sites.push(CallSite {
                            span: Span {
                                start: byte_range.start as u32,
                                len: (byte_range.end - byte_range.start) as u32,
                            },
                            callee,
                            callee_path,
                        });
                    }
                }
            }
            let mark = stack.len();
            for child in node.children() {
                stack.push(child);
            }
            stack[mark..].reverse();
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// AstgrepSource: the floor for every ast-grep lang. Epic U.
//
// AstGrepParser + CstProjector + CallProjector as a Source: the lossless CST
// on `--family cst`, and under the default mask a GUESSED call plane minted
// from the call-kind table. The roster's fallback behind the lang-specific
// Sources; a .sh/.java/... that has no front-end still answers calls.
// ════════════════════════════════════════════════════════════════════════════

/// The ast-grep fallback `Source`: every grammar ast-grep ships that no
/// lang-specific Source claimed. `cst` opt-in, plus the guessed call plane.
#[derive(Default)]
pub struct AstgrepSource;

impl Source for AstgrepSource {
    fn name(&self) -> &'static str {
        "astgrep"
    }

    fn matches(&self, path: &str) -> bool {
        AstGrepParser.matches(path)
    }

    fn extract(&self, path: &str, content: &[u8], mask: FamilyMask) -> RyiOutput {
        let mut strings = Strings::new();
        // ONE parse feeds both planes: cst and the guessed calls walk the same
        // root, so a mask naming either pays the parse exactly once.
        let parsed = if mask.cst || mask.call {
            let arena = AstGrepParser.make_arena();
            let span = trace::parse_span("astgrep", "astgrep");
            let _entered = span.enter();
            AstGrepParser.parse(&arena, path, content).ok()
        } else {
            None
        };
        let cst = if mask.cst {
            parsed.as_ref().map(|parsed| {
                let span = trace::family_span("astgrep", "cst");
                let _entered = span.enter();
                let mut bundle = FamilyBundle::<CstF>::default();
                CstProjector.project(parsed, &mut strings, &mut bundle);
                trace::record_bundle(&span, &bundle, 0);
                bundle
            })
        } else {
            None
        };
        let call = if mask.call {
            parsed.as_ref().map(|parsed| {
                let span = trace::family_span("astgrep", "call");
                let _entered = span.enter();
                let mut bundle = FamilyBundle::<CallF>::default();
                CallProjector.project(parsed, &mut strings, &mut bundle);
                trace::record_bundle(&span, &bundle, 0);
                bundle
            })
        } else {
            None
        };
        RyiOutput {
            strings,
            cst,
            types: None,
            call,
            df: None,
            data: None,
        }
    }
}

// The guessed call arm's resolution: the kotlin name-match law minus the
// planes astgrep does not project (no receivers, no module plane, no same-file
// defs). A callee naming exactly one corpus blob binds `corpus_unique`;
// everything else lands in the drop channel with the def-count reason.
impl Resolve<CallF> for AstgrepSource {
    fn resolve(&self, output: &RyiOutput, cx: &ProjectCx) -> Vec<ProjectEdge<CallF>> {
        let Some(call) = &output.call else {
            return Vec::new();
        };
        let Some(def_index) = cx.indexes.def_index.get() else {
            return Vec::new();
        };
        let mut edges = Vec::new();
        for site in &call.aux.sites {
            let Some(caller) = covering_def(call, site.span) else {
                continue;
            };
            let callee = output.strings.lookup(site.callee);
            let Some((blob, span)) = unique_corpus_def(def_index, callee) else {
                continue;
            };
            edges.push(
                ProjectEdge::new(
                    caller,
                    blob,
                    span,
                    CallEdgeKind::NameResolve,
                    ResolutionOrigin::CorpusUnique,
                )
                .with_call_site(site.span),
            );
        }
        edges
    }
}

/// The one corpus CALL def of `name`, or nothing: kotlin's corpus-unique leg
/// without the same-file seat (the guessed bundle projects no defs).
fn unique_corpus_def(index: &DefIndex, callee: &str) -> Option<(ContentId, Span)> {
    let sites = corpus_defs(index, callee);
    let mut blobs: Vec<ContentId> = Vec::new();
    for site in sites {
        if !blobs.contains(&site.blob) {
            blobs.push(site.blob.clone());
        }
    }
    let [blob] = blobs.as_slice() else {
        return None;
    };
    let site = sites
        .iter()
        .find(|site| site.family == FamilyTag::Call)
        .unwrap_or(&sites[0]);
    Some((blob.clone(), site.span))
}

/// The guessed call arm's non-edge channel: one `unresolved` row per site no
/// edge bound, the reason the def count for the callee's name.
pub fn call_drops(
    output: &RyiOutput,
    cx: &ProjectCx,
    edges: &[ProjectEdge<CallF>],
) -> Vec<ResolveDrop> {
    let (Some(call), Some(def_index)) = (&output.call, cx.indexes.def_index.get()) else {
        return Vec::new();
    };
    let bound: BTreeSet<(u32, u32)> = edges
        .iter()
        .filter_map(|edge| edge.call_site.map(|span| (span.start, span.end())))
        .collect();
    call.aux
        .sites
        .iter()
        .filter(|site| !bound.contains(&(site.span.start, site.span.end())))
        .map(|site| {
            let callee = output.strings.lookup(site.callee);
            let reason = if corpus_defs(def_index, callee).is_empty() {
                UnresolvedReason::NoCorpusDef
            } else {
                UnresolvedReason::Ambiguous
            };
            ResolveDrop {
                span: site.span,
                reason,
                detail: callee.to_string(),
            }
        })
        .collect()
}
