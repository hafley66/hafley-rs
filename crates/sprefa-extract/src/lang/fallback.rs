//! The linked-grammar fallback `Source` + the CstF and guessed-call
//! projections over one plain tree-sitter parse.
//!
//! Grammars come from the crates this workspace links
//! (`RyiLang::tree_sitter_language`); the parse is a `tree_sitter::Parser`, so
//! the extract abides the trait seams. The CST walk is `hafley_scm::cst`'s
//! named-node walk; the call plane walks `tree_sitter::Node` with the same
//! discipline: iterative pre-order DFS, children pushed in reverse so rows
//! stay doc-ordered.
//! @comment-ok: module header, the shape every lang/*.rs opens with

use crate::family::{
    CallEdgeKind, CallF, CallSite, CstEdgeKind, CstF, ProjectEdge, ResolutionOrigin,
};
use crate::lang::call_kinds::{
    ARG_KINDS, CALLEE_FIRST_KINDS, CALLEE_NAME_KINDS, CALL_KINDS, MODULE_CALLER, NAME_LEAF_KINDS,
};
use crate::lang::extract_lang::RyiLang;
use crate::project::ResolveDrop;
use crate::rows::{Edge, FamilyBundle, Node};
use crate::seams::{corpus_defs, covering_def, DefIndex, Resolve};
use crate::shape::{ContentId, FamilyTag, NameId, NodeRef, Span, Strings};
use crate::source::{FamilyMask, ProjectCx, RyiOutput, Source};
use crate::trace;
use crate::types::UnresolvedReason;
use std::collections::BTreeSet;

/// The `CstF` bundle for one file: the `hafley_scm` named-node walk mapped into
/// rows. Kind text is the grammar's own spelling, interned per file; a node's
/// name is its grammar `name:` field, else — for a name-leaf kind with no
/// named children — its own text. One row per named node; a `Child` edge to
/// the nearest named ancestor. `None`: no grammar for the path, non-UTF-8
/// bytes, or a refused parse.
pub fn cst_bundle(path: &str, content: &[u8], strings: &mut Strings) -> Option<FamilyBundle<CstF>> {
    let lang = RyiLang::from_path(path)?;
    let tree = parse_tree(&lang, content)?;
    Some(cst_rows(&lang, &tree, content, strings))
}

/// The guessed-call bundle for one file: one site per node whose kind is in
/// the call-kind table, plus the file-covering MODULE_CALLER def the resolver
/// joins against. `None` under the same conditions as `cst_bundle`.
pub fn call_bundle(
    path: &str,
    content: &[u8],
    strings: &mut Strings,
) -> Option<FamilyBundle<CallF>> {
    let lang = RyiLang::from_path(path)?;
    let tree = parse_tree(&lang, content)?;
    Some(call_rows(&lang, &tree, content, strings))
}

/// The UTF-8 gate the ast-grep parse held (a non-UTF-8 file failed with
/// `Utf8`, and the callers' `.ok()` made it `None`), then the plain
/// tree-sitter parse.
fn parse_tree(lang: &RyiLang, content: &[u8]) -> Option<tree_sitter::Tree> {
    std::str::from_utf8(content)
        .ok()
        .and_then(|_| hafley_scm::cst::parse(&lang.tree_sitter_language(), content))
}

/// One file's CST rows off an already-parsed tree. Shared by `cst_bundle` and
/// the fallback `Source`'s extract, which parses once for both planes.
fn cst_rows(
    lang: &RyiLang,
    tree: &tree_sitter::Tree,
    content: &[u8],
    strings: &mut Strings,
) -> FamilyBundle<CstF> {
    let ts = lang.tree_sitter_language();
    let kinds = RootKinds::resolve(lang);
    let src = std::str::from_utf8(content).unwrap_or("");
    let mut bundle = FamilyBundle::<CstF>::default();
    // Streaming: the walk hands each row straight to the bundle, so peak
    // memory is one bundle, never a second materialized walk beside it.
    hafley_scm::cst::walk_named_streaming(
        tree,
        |ix, parent, kind_id, start, end, name, named_children| {
            let kind_text = hafley_scm::cst::kind_name(&ts, kind_id);
            let kind = strings.intern(kind_text);
            let mut node = Node::new(Span { start, len: end - start }, kind);
            node.name = match name {
                Some((name_start, name_end)) => {
                    Some(strings.intern(&src[name_start as usize..name_end as usize]))
                }
                // Name = the grammar's `name:` field, else an identifier leaf's
                // own text: a name-leaf kind (or a callee-name kind) with no
                // named children.
                None if kinds.is_name_leaf_kind(kind_id)
                    || CALLEE_NAME_KINDS.contains(&kind_text) =>
                {
                    (named_children == 0).then(|| strings.intern(&src[start as usize..end as usize]))
                }
                None => None,
            };
            bundle.nodes.push(node);
            if let Some(parent) = parent {
                bundle
                    .edges
                    .push(Edge::new(NodeRef(parent), NodeRef(ix), CstEdgeKind::Child));
            }
        },
    );
    bundle
}

/// One file's guessed-call rows off an already-parsed tree: the
/// file-covering MODULE_CALLER def plus one site per call-kind node.
fn call_rows(
    lang: &RyiLang,
    tree: &tree_sitter::Tree,
    content: &[u8],
    strings: &mut Strings,
) -> FamilyBundle<CallF> {
    let kinds = RootKinds::resolve(lang);
    let src = std::str::from_utf8(content).unwrap_or("");
    let root = tree.root_node();
    let mut bundle = FamilyBundle::<CallF>::default();
    bundle.nodes.push(Node::new(
        Span {
            start: root.start_byte() as u32,
            len: (root.end_byte() - root.start_byte()) as u32,
        },
        MODULE_CALLER,
    ));
    let mut stack: Vec<tree_sitter::Node<'_>> = vec![root];
    while let Some(node) = stack.pop() {
        let kind = node.kind();
        if node.is_named() && CALL_KINDS.contains(&kind) {
            // An application nested in another application is the SAME chain
            // (the left-associative parse), never another call site; only the
            // outermost apply mints.
            let same_chain = CALLEE_FIRST_KINDS.contains(&kind)
                && node
                    .parent()
                    .is_some_and(|parent| parent.is_named() && CALL_KINDS.contains(&parent.kind()));
            if !same_chain {
                if let Some((callee, callee_path)) = callee_of(&node, src.as_bytes(), &kinds, strings) {
                    bundle.aux.sites.push(CallSite {
                        span: Span {
                            start: node.start_byte() as u32,
                            len: (node.end_byte() - node.start_byte()) as u32,
                        },
                        callee,
                        callee_path,
                    });
                }
            }
        }
        let mark = stack.len();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
        stack[mark..].reverse();
    }
    bundle
}

/// The kind tables resolved to ids against ONE file's grammar
/// (`RyiLang::kind_to_id`, once per bundle, never per node). A name the
/// grammar does not declare resolves to 0, the absent mark: dropped at
/// resolve time, it can never match a node of that grammar.
struct RootKinds {
    name_leaves: Vec<u16>,
    args: Vec<u16>,
}

impl RootKinds {
    fn resolve(lang: &RyiLang) -> Self {
        let ids = |names: &[&str]| {
            names
                .iter()
                .map(|kind| lang.kind_to_id(kind))
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

/// A named leaf whose kind carries a name: the identifier kinds, plus the
/// leaf kinds `0_call_kinds.rs` names (php `name`, haskell `variable`, bash
/// `word`).
fn is_name_leaf(node: &tree_sitter::Node<'_>, kinds: &RootKinds) -> bool {
    node.is_named()
        && node.named_child_count() == 0
        && (kinds.is_name_leaf_kind(node.kind_id()) || CALLEE_NAME_KINDS.contains(&node.kind()))
}

/// The name leaves under `node`, pre-order. A nested call kind stops the
/// descent: `foo (helper 1)` names `foo` here, and the nested call names
/// itself when its own turn comes. Iterative on an explicit stack, the walk
/// discipline of the file's projectors: recursion would die on deep trees.
fn collect_name_leaves<'a>(
    node: tree_sitter::Node<'a>,
    _src: &'a [u8],
    kinds: &RootKinds,
    out: &mut Vec<tree_sitter::Node<'a>>,
) {
    let mut stack: Vec<tree_sitter::Node<'a>> = vec![node];
    while let Some(node) = stack.pop() {
        if CALL_KINDS.contains(&node.kind()) {
            continue;
        }
        if is_name_leaf(&node, kinds) {
            out.push(node);
            continue;
        }
        let mark = stack.len();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
        stack[mark..].reverse();
    }
}

/// The head of a prefix-application chain: the leftmost name leaf, walking
/// the first named child through the nesting. tree-sitter-haskell's `apply`
/// is left-associative, so `map f xs` parses `apply(apply(map, f), xs)` and
/// the head sits at the end of the first-child spine.
fn head_leaf<'a>(node: tree_sitter::Node<'a>) -> tree_sitter::Node<'a> {
    let mut cur = node;
    loop {
        let mut cursor = cur.walk();
        let Some(child) = cur.children(&mut cursor).find(|child| child.is_named()) else {
            return cur;
        };
        cur = child;
    }
}

/// The node's own text, the shape the old ast-grep `text()` answered.
fn node_text<'a>(node: &tree_sitter::Node<'a>, src: &'a [u8]) -> &'a str {
    std::str::from_utf8(&src[node.start_byte()..node.end_byte()]).unwrap_or("")
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
    node: &tree_sitter::Node<'_>,
    src: &[u8],
    kinds: &RootKinds,
    strings: &mut Strings,
) -> Option<(NameId, Option<NameId>)> {
    for field in ["name", "method", "function"] {
        let Some(seat) = node.child_by_field_name(field) else {
            continue;
        };
        let mut leaves = Vec::new();
        collect_name_leaves(seat, src, kinds, &mut leaves);
        let Some(leaf) = leaves.last() else {
            continue;
        };
        let callee = strings.intern(node_text(&leaf, src));
        let seat_text = node_text(&seat, src);
        let callee_path =
            (seat_text != node_text(&leaf, src)).then(|| strings.intern(seat_text));
        return Some((callee, callee_path));
    }
    let mut leaves = Vec::new();
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    for child in children {
        if kinds.is_arg(child.kind_id()) {
            continue;
        }
        collect_name_leaves(child, src, kinds, &mut leaves);
    }
    if CALLEE_FIRST_KINDS.contains(&node.kind()) {
        let head = head_leaf(*node);
        return Some((strings.intern(node_text(&head, src)), None));
    }
    let leaf = leaves.last()?;
    Some((strings.intern(node_text(leaf, src)), None))
}

// ════════════════════════════════════════════════════════════════════════════
// FallbackSource: the floor for every linked grammar no dedicated Source
// claimed (today: html).
//
// The lossless CST on `--family cst`, and under the default mask a GUESSED
// call plane minted from the call-kind table. The roster's fallback behind the
// lang-specific Sources.
// ════════════════════════════════════════════════════════════════════════════

/// The linked-grammar fallback `Source`. `cst` opt-in, plus the guessed call
/// plane.
#[derive(Default)]
pub struct FallbackSource;

impl Source for FallbackSource {
    fn name(&self) -> &'static str {
        "fallback"
    }

    fn matches(&self, path: &str) -> bool {
        // Direct suffix check, NOT `RyiLang::from_path`: the roster routes
        // through `source_for`, and this row IS the roster's floor, so asking
        // it here would recurse.
        matches!(path.rsplit('.').next().unwrap_or(""), "html" | "htm")
    }

    fn extract(&self, path: &str, content: &[u8], mask: FamilyMask) -> RyiOutput {
        let mut strings = Strings::new();
        // ONE parse feeds both planes: cst and the guessed calls walk the same
        // tree, so a mask naming either pays the parse exactly once.
        let parsed = if mask.cst || mask.call {
            let span = trace::parse_span("fallback", "fallback");
            let _entered = span.enter();
            RyiLang::from_path(path).and_then(|lang| parse_tree(&lang, content))
        } else {
            None
        };
        let cst = if mask.cst {
            parsed.as_ref().map(|tree| {
                let span = trace::family_span("fallback", "cst");
                let _entered = span.enter();
                let lang = RyiLang::from_path(path).expect("the parse proved a grammar");
                let bundle = cst_rows(&lang, tree, content, &mut strings);
                trace::record_bundle(&span, &bundle, 0);
                bundle
            })
        } else {
            None
        };
        let call = if mask.call {
            parsed.as_ref().map(|tree| {
                let span = trace::family_span("fallback", "call");
                let _entered = span.enter();
                let lang = RyiLang::from_path(path).expect("the parse proved a grammar");
                let bundle = call_rows(&lang, tree, content, &mut strings);
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
// planes the fallback does not project (no receivers, no module plane, no
// same-file defs). A callee naming exactly one corpus blob binds
// `corpus_unique`; everything else lands in the drop channel with the
// def-count reason.
impl Resolve<CallF> for FallbackSource {
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
