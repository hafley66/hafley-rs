use super::*;

// Names binds written paths through RA module maps; Types and SCIP can
// supply receiver-dependent answers. Corpus joins use engine coordinates.

fn module_qualifier(callee_path: &str) -> Option<Vec<&str>> {
    let mut segments: Vec<&str> = callee_path.split("::").collect();
    segments.pop()?;
    if segments.is_empty() {
        return None;
    }
    segments
        .iter()
        .all(|segment| {
            !segment
                .chars()
                .next()
                .is_some_and(|first| first.is_uppercase())
        })
        .then_some(segments)
}

/// One corpus `DefSite` examined while learning a file's own blob. The term
/// that was quadratic while the join ran once per call site.
static OWN_BLOB_PROBES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn own_blob_probes() -> u64 {
    OWN_BLOB_PROBES.load(std::sync::atomic::Ordering::Relaxed)
}

fn probe<T>(value: T) -> T {
    OWN_BLOB_PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    value
}

/// The corpus blob covering every named CallF def of `output`. One def is not
/// a file identity: two files can hold an identical def at the same offset.
fn own_file_blob(output: &RyiOutput, index: &DefIndex) -> Option<ContentId> {
    // The per-file resolve pins the exact blob; the def-set join below is the
    // fallback for hand-built contexts, and cost 1017 of a run's top samples.
    if let Some(pinned) = crate::read::types::pinned_own() {
        return Some(pinned);
    }
    let call = output.call.as_ref()?;
    let own: Vec<(&str, Span)> = call
        .nodes
        .iter()
        .filter_map(|node| Some((output.strings.lookup(node.name?), node.span)))
        .collect();
    // Seeded on the RAREST name: a corpus-wide name like `new` puts every file
    // in the candidate set, and each candidate costs a full cover check.
    let (seed_name, seed_span) = *own
        .iter()
        .min_by_key(|(name, _)| corpus_defs(index, name).len())?;
    let seeds = || {
        corpus_defs(index, seed_name)
            .iter()
            .filter(|site| probe(site.span == seed_span))
    };
    let mut hits = seeds();
    let first = hits.next()?;
    if hits.next().is_none() {
        return Some(first.blob.clone());
    }
    // Two files carry this (name, span): only the whole named-def set tells
    // them apart.
    let covers = |blob: &ContentId| {
        own.iter().all(|(name, span)| {
            corpus_defs(index, name)
                .iter()
                .any(|site| probe(&site.blob == blob && site.span == *span))
        })
    };
    seeds()
        .find(|site| covers(&site.blob))
        .map(|site| site.blob.clone())
}

/// The scip-resolved corpus target of one call site: the site's occurrence
/// (the shared `site_occurrence` convention) -> its symbol's definition
/// occurrence -> the containing DefSite. None = scip has no corpus CALL
/// target: an external library symbol, an unresolved reference, no occurrence
/// at the site, the target document outside the corpus — OR a `local `
/// symbol, the rust-analyzer adaptation documented on the arm above (a local
/// binding is df-owned; the enclosing fn is NOT the callee).
fn scip_call_target<'a>(
    index: &ScipIndex,
    joined: &[Option<(ContentId, Vec<u8>)>],
    doc_ix: usize,
    site: &CallSite,
    callee: &str,
    def_index: &'a DefIndex,
) -> Option<(ContentId, Span, &'a str)> {
    let doc = &index.documents[doc_ix];
    let (_, content) = joined[doc_ix].as_ref()?;
    let occ = site_occurrence(doc, content, site.span, callee)?;
    if index.symbol(occ).starts_with("local ") {
        return None;
    }
    let (def_doc_ix, def_range) = definition_of(index, doc_ix, occ)?;
    let def_doc = &index.documents[def_doc_ix];
    let (def_blob, def_content) = joined[def_doc_ix].as_ref()?;
    let ident = byte_range_cached(def_doc, def_content, def_range, def_doc.position_encoding)?;
    let (name, def_site) = containing_def_site(def_index, def_blob.clone(), ident)?;
    Some((def_blob.clone(), def_site.span, name))
}

impl Resolve<CallF> for RustSource {
    fn resolve(&self, output: &RyiOutput, cx: &ProjectCx) -> Vec<ProjectEdge<CallF>> {
        let Some(call) = &output.call else {
            return Vec::new();
        };
        let Some(def_index) = cx.indexes.def_index.get() else {
            return Vec::new();
        };
        // The scip leg: the corpus index + the rev-correct reader + this
        // file's own document (found by content hash). Any missing piece ->
        // pure name-match (v5-shaped).
        let scip = cx
            .indexes
            .scip_index
            .get()
            .zip(cx.reader)
            .and_then(|(index, reader)| {
                let joined = cx
                    .indexes
                    .joined_documents
                    .get_or_init(|| join_documents(index, reader));
                let blob = own_blob(cx, output)?;
                let doc_ix = joined
                    .iter()
                    .position(|j| j.as_ref().map_or(false, |(b, _)| *b == blob))?;
                Some((index, joined, doc_ix))
            });
        // Per FILE, never per site: the join is over the whole corpus index.
        let own = own_file_blob(output, def_index);
        let paths = cx.indexes.paths.get();
        let own_path = own
            .as_ref()
            .zip(paths)
            .and_then(|(blob, paths)| paths.get(blob));
        let modules = cx.indexes.rust_modules.get();
        let checker = cx.indexes.rust_checker.get();
        // Sorted once per file: the mirror lookup runs per closure-caller site,
        // and a per-site scan of the def table is the shape kink 1 was.
        let named = named_def_spans(call);
        let mut edges = Vec::new();
        for site in &call.aux.sites {
            // The caller is the innermost covering CallF def (the 4a
            // caller-binding discipline); a module-level site has no caller
            // node and emits no row.
            let Some(caller) = covering_def(call, site.span) else {
                continue;
            };
            let callee = output.strings.lookup(site.callee);
            let method = call
                .aux
                .receivers
                .iter()
                .any(|receiver| receiver.call_site == site.span);
            let name_t = (!method)
                .then(|| {
                    let from = own_path?;
                    let modules = modules?;
                    let written = site
                        .callee_path
                        .map(|id| output.strings.lookup(id))
                        .unwrap_or(callee);
                    let segments = written.split("::").map(str::to_string).collect::<Vec<_>>();
                    let (name, qualifier) = segments.split_last()?;
                    let _ = (name, qualifier);
                    let bound = modules
                        .binding_at(from, &segments, Some(site.span.start), FamilyTag::Call)
                        .ok()?;
                    bound.target_name?;
                    Some((
                        bound.target_blob,
                        bound.target_span,
                        if segments.len() == 1 && bound.target_path != from && bound.kind != crate::read::lang::rust_module_facts::ResolvedImportKind::Star {
                            CallEdgeKind::ImportResolve
                        } else {
                            CallEdgeKind::NameResolve
                        },
                        ResolutionOrigin::ModulePlane,
                    ))
                })
                .flatten();
            let callable = |blob: &ContentId, span: Span| {
                !modules.is_some_and(|m| m.is_collapsed(blob, span) || m.is_alias(blob, span))
            };
            let name_t = name_t.filter(|(blob, span, _, _)| callable(blob, *span));
            // The syntax tier's whole answer for this site: the name match and
            // scip folded the way they fold when no checker runs.
            let syntax_t = |name_t: Option<(ContentId, Span, CallEdgeKind, ResolutionOrigin)>| {
                let scip_t = scip
                    .as_ref()
                    .and_then(|(index, joined, doc_ix)| {
                        scip_call_target(index, joined, *doc_ix, site, callee, def_index)
                    })
                    .filter(|target| callable(&target.0, target.1));
                // Agreement is judged at (blob, name): the name-match binds the
                // call FACET while scip can name the type facet.
                match (name_t, scip_t) {
                    (Some((blob, span, _, origin)), Some(s)) if blob == s.0 && callee == s.2 => {
                        Some(((blob, span), CallEdgeKind::NameResolve, origin))
                    }
                    (_, Some(s)) => Some((
                        (s.0, s.1),
                        CallEdgeKind::ScipOverride,
                        ResolutionOrigin::Scip,
                    )),
                    (Some((blob, span, kind, origin)), None) => Some(((blob, span), kind, origin)),
                    (None, None) => None,
                }
            };
            // The CHECKER tier: rust-analyzer's own answer for this site wins
            // over both the name match and scip, and only where it has one.
            match checker
                .zip(own_path)
                .and_then(|(index, path)| index.call_at(path, site.span, callee))
                .filter(|answer| match answer {
                    CheckerAnswer::Corpus(blob, span) => callable(blob, *span),
                    CheckerAnswer::External(_) => true,
                }) {
                Some(CheckerAnswer::Corpus(dst_blob, dst_span)) => {
                    // Off `witness` the syntax fold's scip leg never runs here.
                    let leg = cx.witness.then(|| syntax_t(name_t)).flatten();
                    let agreed = leg.as_ref().and_then(|((blob, span), _, origin)| {
                        (*blob == dst_blob && *span == dst_span).then_some(*origin)
                    });
                    push_call_edge(
                        &mut edges,
                        call,
                        &named,
                        caller,
                        site.span,
                        dst_blob,
                        dst_span,
                        CallEdgeKind::CheckerResolve,
                        ResolutionOrigin::Checker,
                        agreed,
                    );
                    if let (None, Some(((blob, span), kind, origin))) = (agreed, leg) {
                        push_call_edge(
                            &mut edges, call, &named, caller, site.span, blob, span, kind, origin,
                            None,
                        );
                    }
                    continue;
                }
                // No corpus definition IS this callee, so no name-match leg
                // may invent one; `call_drops` reads the same answer.
                Some(CheckerAnswer::External(_)) => continue,
                None => {}
            }
            let Some(((dst_blob, dst_span), kind, origin)) = syntax_t(name_t) else {
                continue;
            };
            push_call_edge(
                &mut edges, call, &named, caller, site.span, dst_blob, dst_span, kind, origin, None,
            );
        }
        edges
    }
}

/// One site's edge, plus the mirror row a closure caller needs: nothing names
/// `closure@<n>` as a callee, so a walk over named defs stops at one.
#[allow(clippy::too_many_arguments)]
fn push_call_edge(
    edges: &mut Vec<ProjectEdge<CallF>>,
    call: &FamilyBundle<CallF>,
    named: &[(Span, NodeRef)],
    caller: NodeRef,
    site: Span,
    dst_blob: ContentId,
    dst_span: Span,
    kind: CallEdgeKind,
    origin: ResolutionOrigin,
    // A second leg that reached this same target, under `--witness`.
    agreed: Option<ResolutionOrigin>,
) {
    let witness = |edge: ProjectEdge<CallF>| match agreed {
        Some(extra) => edge.witnessed_by(extra),
        None => edge,
    };
    if call.node(caller).name.is_none() {
        if let Some(enclosing) = enclosing_named_def(named, site) {
            edges.push(
                witness(ProjectEdge::new(
                    enclosing,
                    dst_blob.clone(),
                    dst_span,
                    CallEdgeKind::NameResolve,
                    origin,
                ))
                .with_call_site(site),
            );
        }
    }
    edges.push(
        witness(ProjectEdge::new(caller, dst_blob, dst_span, kind, origin)).with_call_site(site),
    );
}

/// One `unresolved` row per site the `Resolve<CallF>` pass dropped. The reason
/// reads the corpus def count for the callee's name: none, or more than one.
/// std::prelude::v1's callable items: a bare name here is never a corpus
/// reference, so its drop says `external`.
const PRELUDE_ITEMS: &[&str] = &[
    "Box", "Err", "Ok", "Option", "Result", "Some", "String", "Vec", "drop", "format",
];

pub fn call_drops(
    output: &RyiOutput,
    cx: &ProjectCx,
    edges: &[ProjectEdge<CallF>],
) -> Vec<ResolveDrop> {
    let (Some(call), Some(def_index)) = (&output.call, cx.indexes.def_index.get()) else {
        return Vec::new();
    };
    let modules = cx.indexes.rust_modules.get();
    let checker = cx.indexes.rust_checker.get();
    let own_path = own_file_blob(output, def_index)
        .as_ref()
        .zip(cx.indexes.paths.get())
        .and_then(|(blob, paths)| paths.get(blob));
    let bound: BTreeSet<(u32, u32)> = edges
        .iter()
        .filter_map(|edge| edge.call_site.map(|span| (span.start, span.end())))
        .collect();
    // An unbound member site is a receiver-policy drop when the plane cannot
    // answer for the receiver; corpus-impl-known types keep the def counts.
    let methods: BTreeSet<(u32, u32)> = call
        .aux
        .receivers
        .iter()
        .map(|receiver| (receiver.call_site.start, receiver.call_site.end()))
        .collect();
    call.aux
        .sites
        .iter()
        .filter(|site| !bound.contains(&(site.span.start, site.span.end())))
        .map(|site| {
            let callee = output.strings.lookup(site.callee);
            let qualifier = site
                .callee_path
                .map(|id| output.strings.lookup(id))
                .and_then(|path| module_qualifier(path));
            let external_prefix = qualifier
                .as_ref()
                .zip(own_path)
                .and_then(|(qualifier, from)| {
                    let segments: Vec<String> = qualifier
                        .iter()
                        .map(|segment| segment.to_string())
                        .collect();
                    matches!(
                        modules.map(|m| m.module_call(from, &segments, callee)),
                        Some(crate::read::lang::rust_module_facts::ModuleCallTarget::External)
                    )
                    .then_some(())
                });
            let names_reason = modules.zip(own_path).and_then(|(modules, from)| {
                let written = site.callee_path.map(|id| output.strings.lookup(id)).unwrap_or(callee);
                modules.binding_at(from, &written.split("::").map(str::to_string).collect::<Vec<_>>(), Some(site.span.start), FamilyTag::Call).err()
            });
            let checker_external = match checker
                .zip(own_path)
                .and_then(|(index, path)| index.call_at(path, site.span, callee))
            {
                Some(CheckerAnswer::External(qualified)) => Some(qualified),
                _ => None,
            };
            let reason = if checker_external.is_some() || names_reason == Some(UnresolvedReason::External) {
                UnresolvedReason::External
            } else if methods.contains(&(site.span.start, site.span.end())) {
                if checker.is_none() {
                    UnresolvedReason::NeedsTypes
                } else {
                    UnresolvedReason::Inferred
                }
            } else if checker.is_none() && names_reason == Some(UnresolvedReason::NeedsTypes) {
                UnresolvedReason::NeedsTypes
            } else if external_prefix.is_some()
                || (qualifier.is_none() && PRELUDE_ITEMS.contains(&callee))
            {
                UnresolvedReason::External
            } else if let Some(reason) = names_reason {
                reason
            } else if corpus_defs(def_index, callee).is_empty() {
                UnresolvedReason::NoCorpusDef
            } else {
                UnresolvedReason::Ambiguous
            };
            // The checker's crate-qualified path, else the path as written.
            let detail = checker_external.unwrap_or_else(|| {
                site.callee_path.map_or_else(
                    || callee.to_string(),
                    |id| output.strings.lookup(id).to_string(),
                )
            });
            ResolveDrop {
                span: site.span,
                reason,
                detail,
            }
        })
        .collect()
}

/// Every NAMED CallF def as (span, ref), sorted by (start, end) for the
/// `enclosing_named_def` binary search.
fn named_def_spans(defs: &FamilyBundle<CallF>) -> Vec<(Span, NodeRef)> {
    let mut sorted: Vec<(Span, NodeRef)> = defs
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.name.is_some())
        .map(|(ix, node)| (node.span, NodeRef(ix as u32)))
        .collect();
    sorted.sort_by_key(|(span, _)| (span.start, span.end()));
    sorted
}

/// The innermost NAMED def covering `site`. `covering_def` takes the innermost
/// def of any kind, which is the closure wherever one is in the way.
fn enclosing_named_def(sorted: &[(Span, NodeRef)], site: Span) -> Option<NodeRef> {
    let cut = sorted.partition_point(|(span, _)| span.start <= site.start);
    let mut best: Option<(Span, NodeRef)> = None;
    for &(span, r) in &sorted[..cut] {
        if site.end() <= span.end()
            && best.map_or(true, |(b, _)| span.end() - span.start < b.end() - b.start)
        {
            best = Some((span, r));
        }
    }
    best.map(|(_, r)| r)
}

// ════════════════════════════════════════════════════════════════════════════
// CallF: callable definitions (nodes) + call sites (aux).
//
// Ports v5 `rust_call_defs_from` (defs, incl. the nested-fn/closure walker) +
// `rust_call_sites_from` (sites). v5's `mint_sym`/`lambda_sym`/`end` line are
// deleted: a def is span + kind + name. The def span COVERS its body (ident
// start -> block end) so the seam's span-containment can bind a site's caller;
// the parity line reads `line_of(span.start)` = the ident line (v5's `def.line`).
// Lambda defs (closures) keep kind=Lambda, name=None (v5's empty name).
// ════════════════════════════════════════════════════════════════════════════

/// Descends inline `mod name { .. }`: the SITE half walks the whole file, so a
/// callable declared in one needs a def or the file reports uses without them.
pub(super) fn scm_call_defs(
    query: &hafley_scm::QueryExt,
    src: &[u8],
    arena: &hafley_scm::MatchArena,
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    for row in hafley_scm::lang::rust::call_definition_rows_from_arena(query, arena, src) {
        let mut node = Node::new(
            Span {
                start: row.range.start,
                len: row.range.end - row.range.start,
            },
            match row.kind {
                CallDefinitionKind::Free => CallKind::Free,
                CallDefinitionKind::Method => CallKind::Method,
                CallDefinitionKind::Lambda => CallKind::Lambda,
            },
        );
        if let Some(name) = row.name {
            node = node.with_name(strings.intern(&name));
        }
        sink.nodes.push(node);
    }
}

/// The crate's metadata rows, interned and appended onto the CallF aux.
fn syn_call_metadata(
    tree: &tree_sitter::Tree,
    source: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let defs: BTreeSet<(u32, u32)> = sink
        .nodes
        .iter()
        .map(|node| (node.span.start, node.span.end()))
        .collect();
    let defs = defs.into_iter().collect::<Vec<_>>();
    let owners = call_metadata_rows_from_tree(tree, source, &defs);
    for row in owners {
        sink.aux.method_owners.push(MethodOwner {
            span: Span {
                start: row.start,
                len: row.end - row.start,
            },
            self_type: row.self_type.map(|name| strings.intern(&name)),
            trait_name: row.trait_name.map(|name| strings.intern(&name)),
        });
    }
}
/// Project the CallF family: one def node per callable (Free / Method / Lambda
/// / ConstInit) + one site per call expression. Port of v5
/// `rust_call_{defs,sites}_from` + `CallCollector`.
pub(super) fn project_call(
    tree: &tree_sitter::Tree,
    source: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    // Defs snapshot before the walk: a CONST_INIT is minted only when its
    // initializer's calls escape the engine's own def spans.
    let defs: Vec<std::ops::Range<u32>> = sink
        .nodes
        .iter()
        .map(|node| node.span.start..node.span.end())
        .collect();
    let rows = call_site_rows_from_tree(tree, source, &defs);
    sink.aux
        .expected_types
        .extend(rows.expected_types.iter().map(|(range, ty)| {
            (
                Span {
                    start: range.start,
                    len: range.end - range.start,
                },
                strings.intern(ty),
            )
        }));
    // Mint the CONST_INIT defs in walk order, before metadata reads the node set.
    for row in rows.const_inits {
        let span = Span {
            start: row.range.start,
            len: row.range.end - row.range.start,
        };
        sink.nodes
            .push(Node::new(span, CONST_INIT).with_name(strings.intern(&row.name)));
    }
    syn_call_metadata(tree, source, strings, sink);
    for site in rows.sites {
        sink.aux.sites.push(CallSite {
            span: Span {
                start: site.range.start,
                len: site.range.end - site.range.start,
            },
            callee: strings.intern(&site.callee),
            callee_path: site.callee_path.map(|path| strings.intern(&path)),
        });
    }

    module_specifiers(tree, source, strings, sink);
    super::super::rust_receivers::collect_receivers_from_tree(tree, source, strings, sink);
}

/// Intern SCM's module rows into the CallF specifier vocabulary.
fn module_specifiers(
    tree: &tree_sitter::Tree,
    source: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    sink.aux.specifiers.extend(
        hafley_scm::lang::rust::module_specifier_rows_from_tree(tree, source)
            .into_iter()
            .map(|row| Specifier {
                span: Span {
                    start: row.range.start,
                    len: row.range.end - row.range.start,
                },
                name: strings.intern(&row.name),
                kind: match row.kind {
                    hafley_scm::lang::rust::ModuleSpecifierKind::Module => SpecifierKind::Module,
                    hafley_scm::lang::rust::ModuleSpecifierKind::ModulePath => {
                        SpecifierKind::ModulePath
                    }
                    hafley_scm::lang::rust::ModuleSpecifierKind::Named => SpecifierKind::Named,
                    hafley_scm::lang::rust::ModuleSpecifierKind::Namespace => {
                        SpecifierKind::Namespace
                    }
                    hafley_scm::lang::rust::ModuleSpecifierKind::Reexport => {
                        SpecifierKind::Reexport
                    }
                },
                module: Some(strings.intern(&row.module)),
                imported: None,
                type_only: false,
            }),
    );
}

pub(super) fn splice_macro_expansions(
    src: &str,
    strings: &mut Strings,
    bundle: &mut FamilyBundle<CallF>,
) {
    use hafley_scm::lang::rust::ExpandedCallKind;

    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let rows = hafley_scm::lang::rust::expanded_call_rows(
        src,
        hafley_scm::lang::rust::call_query(),
        &language,
    );
    for row in rows.defs {
        let kind = match row.kind {
            ExpandedCallKind::Free => CallKind::Free,
            ExpandedCallKind::Method => CallKind::Method,
            ExpandedCallKind::Lambda => CallKind::Lambda,
            ExpandedCallKind::ConstInit => CONST_INIT,
        };
        let mut node = Node::new(
            Span {
                start: row.range.start,
                len: row.range.end - row.range.start,
            },
            kind,
        );
        if let Some(name) = row.name {
            node = node.with_name(strings.intern(&name));
        }
        bundle.nodes.push(node);
    }
    for row in rows.sites {
        bundle.aux.sites.push(CallSite {
            span: Span {
                start: row.range.start,
                len: row.range.end - row.range.start,
            },
            callee: strings.intern(&row.callee),
            callee_path: row.callee_path.map(|path| strings.intern(&path)),
        });
    }
    for (range, name) in rows.macros {
        bundle.aux.macro_sites.push(MacroSite {
            span: Span {
                start: range.start,
                len: range.end - range.start,
            },
            macro_name: strings.intern(&name),
            source: MacroSiteSource::Mbe,
        });
    }
}
