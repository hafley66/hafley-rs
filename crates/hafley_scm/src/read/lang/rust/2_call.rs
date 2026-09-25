use super::*;

// ════════════════════════════════════════════════════════════════════════════
// Resolve<CallF> for RustSource, two
// legs per the user rulings (scip-override ALLOWED; the v5-shaped name-match
// stays primary):
//   NameResolve — callee name -> unique def. Same-file WINS via the span-join
//     (def_named in THIS CallF bundle -> its span -> the DefIndex gives the
//     blob); cross-file a UNIQUE corpus blob (CallF facet preferred);
//     ambiguous/absent -> NO ROW.
//   ScipOverride — scip's occurrence resolution for the site disagrees with
//     the name-match outcome: scip's corpus target WINS the edge, the
//     name-match is displaced. Needs the corpus scip index
//     (cx.indexes.scip_index) AND the rev-correct reader (cx.reader); either
//     absent -> pure name-match. scip-EXTERNAL never displaces and never
//     mints.
// RUST-ANALYZER ADAPTATION (the honest per-indexer difference, mirrored by
// the ratchet and logged in the ledger): a `local ` symbol at a call site is
// a LOCAL BINDING (`let func = |x| ..; func(..)`) — df-owned, not a call-
// graph def (rust-analyzer names no closure symbol; scip's answer is the
// binding, and the 4c containing_def_site join would misroute it to the
// ENCLOSING fn, minting a false self-edge). Local-symbol sites are treated
// as scip-external: NO v6 edge. Method resolution stays NAME-ONLY per the 4a
// ADDENDUM (receiver typing out of scope). `callee_path` rides phase 1 as
// collected (rust fills it); the resolution key stays the trailing segment —
// no path-qualified matching is invented (unexercised by the fixtures and
// unratchetable where scip already arbitrates).
// The arm learns its own blob by the DefIndex span-join (`own_blob`) and its
// scip document by content hash (`join_documents`). Per-site edges, no dedup.
// A site outside every CallF def (module level) emits no row.
// ════════════════════════════════════════════════════════════════════════════

/// The SAME-FILE def named `callee`, extracted so `rust_modules.rs` can run it
/// before an import-binding leg: a local def shadows an import.
fn same_file_call_match(
    output: &RyiOutput,
    index: &DefIndex,
    own: Option<&ContentId>,
    callee: &str,
) -> Option<(ContentId, Span)> {
    let call = output.call.as_ref()?;
    // A plain `f()` names a free fn, never a method of some impl in the file.
    let span = call
        .nodes
        .iter()
        .filter(|node| node.name.is_some_and(|id| output.strings.lookup(id) == callee))
        .map(|node| node.span)
        .find(|span| !call.aux.method_owners.iter().any(|owner| owner.span == *span))?;
    // Every def spliced out of one macro expansion carries the macro call's
    // span, so a span several names share cannot name one target.
    let shared = call.nodes.iter().any(|node| {
        node.span == span
            && node
                .name
                .is_some_and(|id| output.strings.lookup(id) != callee)
    });
    if shared {
        return None;
    }
    // The span join must land on THIS file's DefSite: a byte-identical
    // (name, span) def can exist in two files.
    let blob = own?;
    corpus_defs(index, callee)
        .iter()
        .find(|site| site.span == span && &site.blob == blob)
        .map(|site| (site.blob.clone(), site.span))
}

impl RustSource {
    /// The name-match target of one callee (the NameResolve leg). Pub so the
    /// scip ratchet re-runs it to classify overrides — same discipline as
    /// `type_edge_candidates`. Mirror of `TsSource::call_name_match`
    /// (the post-4d dedup sweep owns unifying the per-lang copies).
    pub fn call_name_match(
        output: &RyiOutput,
        index: &DefIndex,
        callee: &str,
    ) -> Option<(ContentId, Span)> {
        let own = own_file_blob(output, index);
        Self::call_name_match_in(output, index, own.as_ref(), callee)
    }

    /// `call_name_match` with the file's own blob already in hand: the blob is
    /// a per-FILE fact, and finding it costs a corpus-index join per call.
    pub fn call_name_match_in(
        output: &RyiOutput,
        index: &DefIndex,
        own: Option<&ContentId>,
        callee: &str,
    ) -> Option<(ContentId, Span)> {
        if let Some(found) = same_file_call_match(output, index, own, callee) {
            return Some(found);
        }
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
            .find(|s| s.family == FamilyTag::Call)
            .unwrap_or(&sites[0]);
        Some((blob.clone(), site.span))
    }

    /// The name-match target of a callee written `a::b::f` for MODULES `a::b`:
    /// only defs whose file spells a module path ending in `a::b` are candidates.
    pub fn call_name_match_in_module(
        index: &DefIndex,
        paths: &PathIndex,
        from: &str,
        qualifier: &[&str],
        callee: &str,
    ) -> Option<(ContentId, Span)> {
        let want = module_target(from, qualifier)?;
        let sites: Vec<&DefSite> = corpus_defs(index, callee)
            .iter()
            .filter(|site| {
                paths
                    .get(&site.blob)
                    .is_some_and(|path| want.covers(&module_segments(path)))
            })
            .collect();
        let mut blobs: Vec<&ContentId> = Vec::new();
        for site in &sites {
            if !blobs.contains(&&site.blob) {
                blobs.push(&site.blob);
            }
        }
        let [blob] = blobs.as_slice() else {
            return None;
        };
        let site = sites
            .iter()
            .find(|s| s.family == FamilyTag::Call)
            .unwrap_or(&sites[0]);
        Some(((*blob).clone(), site.span))
    }
}

/// The type an associated-call path names: the LAST uppercase-leading
/// segment before the callee (`ast::MethodCallExpr::cast` -> `MethodCallExpr`).
/// All-lowercase paths are module-qualified, not associated.
fn assoc_path_type(callee_path: Option<&str>) -> Option<String> {
    let path = callee_path?;
    let segments: Vec<&str> = path.split("::").collect();
    if segments.len() < 2 {
        return None;
    }
    segments[..segments.len() - 1]
        .iter()
        .rev()
        .find(|segment| {
            segment
                .chars()
                .next()
                .is_some_and(|first| first.is_uppercase())
        })
        .map(|segment| (*segment).to_string())
}

/// The enclosing impl's self type for a `Self::f()` site: the caller def's
/// span lies inside a method def whose method-owner row names it.
fn self_impl_type(
    call: &FamilyBundle<CallF>,
    strings: &Strings,
    caller: NodeRef,
) -> Option<String> {
    let caller_span = &call.node(caller).span;
    call.aux
        .method_owners
        .iter()
        .find(|owner| {
            owner.self_type.is_some()
                && owner.span.start <= caller_span.start
                && caller_span.end() <= owner.span.end()
        })
        .and_then(|owner| owner.self_type.map(|name| strings.lookup(name).to_string()))
}

/// A `callee_path`'s leading segments when every one is MODULE-shaped, else
/// None: receiver typing is out of scope, so `Widget::build` keeps the name leg.
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

/// The module path a file spells: minus `.rs`, `src` dropped, `mod`/`lib`/`main`
/// collapsing to the directory, `-` read as `_` (`crates/ide-db` is `ide_db`).
pub fn module_segments(path: &str) -> Vec<String> {
    let stem = path.strip_suffix(".rs").unwrap_or(path);
    let mut segments: Vec<String> = stem
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != "." && *segment != "src")
        .map(|segment| segment.replace('-', "_"))
        .collect();
    if matches!(
        segments.last().map(String::as_str),
        Some("mod" | "lib" | "main")
    ) {
        segments.pop();
    }
    segments
}

/// What a resolved qualifier demands of a candidate file: a module-path suffix,
/// and under `crate::read::` the caller's own crate directory as a path prefix.
pub struct ModuleTarget {
    pub suffix: Vec<String>,
    pub crate_root: Option<String>,
}

impl ModuleTarget {
    /// `crate::read::a::b` is anchored at the crate root: root ++ suffix EXACTLY,
    /// so `crate::read::tests` never also names `src/context/tests.rs`.
    pub fn covers(&self, candidate: &[String]) -> bool {
        if let Some(root) = &self.crate_root {
            let mut anchored = module_segments(root);
            anchored.extend(self.suffix.iter().cloned());
            return candidate == anchored.as_slice();
        }
        candidate.ends_with(&self.suffix)
    }
}

/// `qualifier` read from `from`'s position: `crate` restarts at the crate root,
/// `self` extends the caller's module, `super` pops one, else absolute suffix.
pub fn module_target(from: &str, qualifier: &[&str]) -> Option<ModuleTarget> {
    let own = module_segments(from);
    let normalize = |rest: &[&str]| -> Vec<String> {
        rest.iter()
            .map(|segment| segment.replace('-', "_"))
            .collect()
    };
    match qualifier[0] {
        "crate" => Some(ModuleTarget {
            suffix: normalize(&qualifier[1..]),
            crate_root: crate_root_of(from),
        }),
        "self" | "super" => {
            let mut base = own;
            let mut rest = qualifier;
            while let Some(head) = rest.first() {
                match *head {
                    "self" => {}
                    "super" => {
                        base.pop()?;
                    }
                    _ => break,
                }
                rest = &rest[1..];
            }
            base.extend(normalize(rest));
            Some(ModuleTarget {
                suffix: base,
                crate_root: None,
            })
        }
        _ => Some(ModuleTarget {
            suffix: normalize(qualifier),
            crate_root: None,
        }),
    }
}

/// The crate directory holding `path`: the prefix ending at the segment before
/// the first `src`. None where the file sits outside a Cargo layout.
pub fn crate_root_of(path: &str) -> Option<String> {
    let (root, _) = path.split_once("/src/")?;
    Some(root.to_string())
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
            let qualifier = site
                .callee_path
                .map(|id| output.strings.lookup(id))
                .and_then(module_qualifier);
            // The receiver leg (the go twin): a method site whose receiver's
            // type the compiler could see in scope binds ONLY through the
            // corpus (T, m) impl table; the name-match never runs for it.
            let recv_named: Option<String> = call
                .aux
                .receivers
                .iter()
                .find(|r| r.call_site == site.span)
                .and_then(|r| match &r.outcome {
                    ReceiverOutcome::Named(name) => Some(output.strings.lookup(*name).to_string()),
                    _ => None,
                });
            let recv_t = recv_named.as_ref().and_then(|ty| {
                modules
                    .and_then(|m| {
                        m.impl_target(ty, callee, own_path)
                            // The receiver names a corpus trait (`dyn T`,
                            // `impl T`, a bound param): the trait's own fn
                            // def (class 6).
                            .or_else(|| {
                                m.is_trait(ty)
                                    .then_some(())
                                    .and_then(|()| m.trait_fn_target(ty, callee, own_path))
                            })
                            // No impl defines the method; a trait the type
                            // implements provides a default body (class 4).
                            .or_else(|| m.trait_default_target(ty, callee, own_path))
                    })
                    .map(|(blob, span)| (blob, span, CallEdgeKind::NameResolve))
            });
            // The receiver was SEEN in scope (typed, or a plain call to a
            // scope-bound name): even unbound, the name-match legs never run.
            let recv_known = call.aux.receivers.iter().any(|r| {
                r.call_site == site.span
                    && matches!(
                        r.outcome,
                        ReceiverOutcome::Named(_) | ReceiverOutcome::Shadowed
                    )
            });
            // Phase 1 saw the receiver but could not name its type: a member
            // call on an untyped receiver, so the name-match legs never run.
            let recv_inferred = call.aux.receivers.iter().any(|r| {
                r.call_site == site.span && matches!(r.outcome, ReceiverOutcome::Inferred)
            });
            // The associated leg: `T::f()` / `a::T::f()` names T's impl block;
            // `Self::f()` names the enclosing impl's self type via the file's
            // own method-owner rows.
            let assoc_t = (qualifier.is_none() && recv_named.is_none())
                .then_some(())
                .and_then(|()| {
                    assoc_path_type(site.callee_path.map(|id| output.strings.lookup(id))).and_then(
                        |ty| {
                            modules
                                .and_then(|m| m.impl_target(&ty, callee, own_path))
                                .map(|(blob, span)| (blob, span, CallEdgeKind::NameResolve))
                                // 0 impls and a variant of the enum: the path names
                                // the enum itself.
                                .or_else(|| {
                                    modules
                                        .and_then(|m| m.variant_ctor_target(&ty, callee))
                                        .map(|(blob, span)| (blob, span, CallEdgeKind::NameResolve))
                                })
                                // Trait dispatch: T a corpus trait (class 12, impl
                                // first), else a trait-provided fn with no impl
                                // override (class 8's zero-impl arm).
                                .or_else(|| {
                                    modules.and_then(|m| {
                                        m.trait_impl_target(&ty, callee, own_path)
                                            .or_else(|| m.trait_fn_target(&ty, callee, own_path))
                                            .or_else(|| {
                                                m.trait_default_target(&ty, callee, own_path)
                                            })
                                            .map(|(blob, span)| {
                                                (blob, span, CallEdgeKind::NameResolve)
                                            })
                                    })
                                })
                        },
                    )
                });
            let type_path = qualifier.is_none()
                && recv_named.is_none()
                && assoc_path_type(site.callee_path.map(|id| output.strings.lookup(id))).is_some();
            let self_t = (qualifier.is_none()
                && recv_named.is_none()
                && site
                    .callee_path
                    .map(|id| output.strings.lookup(id).split("::").next() == Some("Self"))
                    .unwrap_or(false))
            .then_some(())
            .and_then(|()| self_impl_type(call, &output.strings, caller))
            .and_then(|ty| {
                modules
                    .and_then(|m| {
                        m.impl_target(&ty, callee, own_path)
                            .or_else(|| m.variant_ctor_target(&ty, callee))
                    })
                    .map(|(blob, span)| (blob, span, CallEdgeKind::NameResolve))
            });
            // Each leg names ITSELF: `kind` is `name_resolve` for nearly all
            // of them, so only the origin separates the receiver plane from the
            // module plane from the corpus-wide guess.
            let tag = |found: Option<(ContentId, Span, CallEdgeKind)>, origin| {
                found.map(|(blob, span, kind)| (blob, span, kind, origin))
            };
            let name_t: Option<(ContentId, Span, CallEdgeKind, ResolutionOrigin)> = if recv_t
                .is_some()
            {
                tag(recv_t, ResolutionOrigin::Receiver)
            } else if recv_known {
                // A KNOWN receiver type with no corpus impl target is
                // definitive (std, an external crate, trait dispatch).
                None
            } else if recv_inferred {
                None
            } else {
                match (qualifier, own_path, paths) {
                    (Some(qualifier), Some(from), Some(paths)) => {
                        let segments: Vec<String> = qualifier
                            .iter()
                            .map(|segment| segment.to_string())
                            .collect();
                        match modules
                            .map(|m| m.module_call(from, &segments, callee))
                            .unwrap_or(crate::read::lang::rust_modules::ModuleCallTarget::Miss)
                        {
                            crate::read::lang::rust_modules::ModuleCallTarget::Target(blob, span) => {
                                Some((
                                    blob,
                                    span,
                                    CallEdgeKind::NameResolve,
                                    ResolutionOrigin::ModulePlane,
                                ))
                            }
                            _ => RustSource::call_name_match_in_module(
                                def_index, paths, from, &qualifier, callee,
                            )
                            .map(|(blob, span)| {
                                (
                                    blob,
                                    span,
                                    CallEdgeKind::NameResolve,
                                    ResolutionOrigin::ModulePlane,
                                )
                            }),
                        }
                    }
                    // `Vec::new()`: a type-qualified path whose type has no corpus
                    // impl is external; no name match may bind it.
                    _ if assoc_t.is_none() && self_t.is_none() && type_path => None,
                    _ => tag(assoc_t, ResolutionOrigin::SelfType)
                        .or_else(|| tag(self_t, ResolutionOrigin::SelfType))
                        .or_else(|| {
                            same_file_call_match(output, def_index, own.as_ref(), callee).map(
                                |(blob, span)| {
                                    (
                                        blob,
                                        span,
                                        CallEdgeKind::NameResolve,
                                        ResolutionOrigin::SameFile,
                                    )
                                },
                            )
                        })
                        .or_else(|| {
                            import_bound_target(modules, own_path, callee).map(|(blob, span)| {
                                (
                                    blob,
                                    span,
                                    CallEdgeKind::ImportResolve,
                                    ResolutionOrigin::ModulePlane,
                                )
                            })
                        })
                        .or_else(|| {
                            RustSource::call_name_match_in(output, def_index, own.as_ref(), callee)
                                .map(|(blob, span)| {
                                    (
                                        blob,
                                        span,
                                        CallEdgeKind::NameResolve,
                                        ResolutionOrigin::CorpusUnique,
                                    )
                                })
                        }),
                }
            };
            // A def coordinate several names share is one macro expansion's
            // collapsed span: it names nothing, so no name match binds there.
            // A `type X = ..` coordinate names no CALLABLE: `X(..)` constructs
            // the aliased item, and the alias's own def is not it.
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
                    CheckerAnswer::External => true,
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
                Some(CheckerAnswer::External) => continue,
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
    let inferred: BTreeSet<(u32, u32)> = call
        .aux
        .receivers
        .iter()
        .filter(|r| match &r.outcome {
            ReceiverOutcome::Inferred | ReceiverOutcome::Shadowed => true,
            // Two conflicting declarations traced: the def counts below tell
            // the story.
            ReceiverOutcome::Named(ty) => {
                !modules.is_some_and(|m| m.is_impl_known(output.strings.lookup(*ty)))
            }
            ReceiverOutcome::Ambiguous => false,
        })
        .map(|r| (r.call_site.start, r.call_site.end()))
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
                        Some(crate::read::lang::rust_modules::ModuleCallTarget::External)
                    )
                    .then_some(())
                });
            let checker_external = matches!(
                checker
                    .zip(own_path)
                    .and_then(|(index, path)| index.call_at(path, site.span, callee)),
                Some(CheckerAnswer::External)
            );
            let reason = if checker_external {
                UnresolvedReason::External
            } else if inferred.contains(&(site.span.start, site.span.end())) {
                UnresolvedReason::Inferred
            } else if external_prefix.is_some()
                || (qualifier.is_none() && PRELUDE_ITEMS.contains(&callee))
            {
                UnresolvedReason::External
            } else if corpus_defs(def_index, callee).is_empty() {
                UnresolvedReason::NoCorpusDef
            } else {
                UnresolvedReason::Ambiguous
            };
            let detail = site.callee_path.map_or_else(
                || callee.to_string(),
                |id| output.strings.lookup(id).to_string(),
            );
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

/// A proc_macro2 span pair -> v6 byte Span covering `[start.start, end.end)`.
/// The def span covers the whole callable body for span-containment resolution.
pub fn def_span(
    line_starts: &[u32],
    start: proc_macro2::Span,
    end: proc_macro2::Span,
) -> Span {
    let start_lc = start.start();
    let end_lc = end.end();
    let start_byte = line_col_to_byte(line_starts, start_lc.line as u32, start_lc.column as u32);
    let end_byte = line_col_to_byte(line_starts, end_lc.line as u32, end_lc.column as u32);
    Span {
        start: start_byte,
        len: end_byte.saturating_sub(start_byte),
    }
}

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
    parsed: &syn::File,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let defs: BTreeSet<(u32, u32)> = sink
        .nodes
        .iter()
        .map(|node| (node.span.start, node.span.end()))
        .collect();
    let (cfg, owners) = call_metadata_rows(parsed, line_starts, &defs);
    for row in cfg {
        sink.aux.cfg_scopes.push(CfgScope {
            span: Span {
                start: row.start,
                len: row.end - row.start,
            },
            cfg: strings.intern(&row.predicate),
        });
    }
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
    parsed: &syn::File,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    // Defs snapshot before the walk: a CONST_INIT is minted only when its
    // initializer's calls escape the engine's own def spans.
    let defs: Vec<std::ops::Range<u32>> = sink.nodes.iter().map(|node| node.span.start..node.span.end()).collect();
    let rows = call_site_rows(parsed, line_starts, &defs);
    // Mint the CONST_INIT defs in walk order, before metadata reads the node
    // set: a gated const's cfg row is admitted by its own CONST_INIT node.
    for row in rows.const_inits {
        let span = Span { start: row.range.start, len: row.range.end - row.range.start };
        sink.nodes
            .push(Node::new(span, CONST_INIT).with_name(strings.intern(&row.name)));
    }
    syn_call_metadata(parsed, line_starts, strings, sink);

    for (callee, predicate) in rows.test_only_calls {
        sink.aux.test_only_calls.push(TestOnlyCall {
            callee: strings.intern(&callee),
            cfg: strings.intern(&predicate),
        });
    }
    for site in rows.sites {
        sink.aux.sites.push(CallSite {
            span: Span { start: site.range.start, len: site.range.end - site.range.start },
            callee: strings.intern(&site.callee),
            callee_path: site.callee_path.map(|path| strings.intern(&path)),
        });
    }

    module_specifiers(parsed, line_starts, strings, sink);
    super::super::rust_receivers::collect_receivers(parsed, line_starts, strings, sink);
}

/// Intern SCM's module rows into the CallF specifier vocabulary.
fn module_specifiers(
    parsed: &syn::File,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    sink.aux.specifiers.extend(
        hafley_scm::lang::rust::module_specifier_rows(parsed, line_starts)
            .into_iter()
            .map(|row| Specifier {
                span: Span {
                    start: row.range.start,
                    len: row.range.end - row.range.start,
                },
                name: strings.intern(&row.name),
                kind: match row.kind {
                    hafley_scm::lang::rust::ModuleSpecifierKind::Named => SpecifierKind::Named,
                    hafley_scm::lang::rust::ModuleSpecifierKind::Namespace => SpecifierKind::Namespace,
                    hafley_scm::lang::rust::ModuleSpecifierKind::Reexport => SpecifierKind::Reexport,
                },
                module: Some(strings.intern(&row.module)),
                imported: None,
            }),
    );
}

pub(super) fn splice_macro_expansions(src: &str, strings: &mut Strings, bundle: &mut FamilyBundle<CallF>) {
    use hafley_scm::lang::rust::ExpandedCallKind;

    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let rows = hafley_scm::lang::rust::expanded_call_rows(src, rust_call_query(), &language);
    for row in rows.defs {
        let kind = match row.kind {
            ExpandedCallKind::Free => CallKind::Free,
            ExpandedCallKind::Method => CallKind::Method,
            ExpandedCallKind::Lambda => CallKind::Lambda,
            ExpandedCallKind::ConstInit => CONST_INIT,
        };
        let mut node = Node::new(Span { start: row.range.start, len: row.range.end - row.range.start }, kind);
        if let Some(name) = row.name {
            node = node.with_name(strings.intern(&name));
        }
        bundle.nodes.push(node);
    }
    for row in rows.sites {
        bundle.aux.sites.push(CallSite {
            span: Span { start: row.range.start, len: row.range.end - row.range.start },
            callee: strings.intern(&row.callee),
            callee_path: row.callee_path.map(|path| strings.intern(&path)),
        });
    }
    for (range, name) in rows.macros {
        bundle.aux.macro_sites.push(MacroSite {
            span: Span { start: range.start, len: range.end - range.start },
            macro_name: strings.intern(&name),
            source: MacroSiteSource::Mbe,
        });
    }
}
