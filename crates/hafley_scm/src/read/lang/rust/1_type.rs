use super::*;
use crate::read::lang::rust_type_refs::type_probe_key;

// ════════════════════════════════════════════════════════════════════════════
// TypeF: entity nodes + arrow-type sigs + the const facet.
//
// Ports v5 `rust_entities_from` (the entity half) + `rust_fn_type` (the arrow-
// type payload) + `rust_const_values_from` (Const entities + ConstValue rows).
// The name-resolved type EDGES (field/impl/variant/uses/generic) are bound by
// `Resolve<TypeF>`; phase 1 stays pure-content span nodes.
//
// v5 stores `parent`/`sym`/`mint_sym`; v6 drops them (a node is span+kind+name;
// the parent linkage is span-containment at the seam). v5 maps Union -> Struct
// (EntityKind has no union); v6 has no union kind either, so the same mapping.
// ════════════════════════════════════════════════════════════════════════════

/// Project the TypeF family: one entity node per type/function declaration, an
/// arrow-type sig per callable param/return type reference, and the const facet
/// (Const entities + ConstValue rows). Port of v5 `rust_entities_from` +
/// `rust_const_values_from`.
pub(super) fn project_types(
    tree: &tree_sitter::Tree,
    source: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let rows = hafley_scm::lang::rust::type_entity_rows_from_tree(tree, source);
    for row in rows.entities {
        let span = Span {
            start: row.range.start,
            len: row.range.end - row.range.start,
        };
        let kind = match row.kind {
            hafley_scm::lang::rust::TypeEntityKind::Struct => TypeEntityKind::Struct,
            hafley_scm::lang::rust::TypeEntityKind::Enum => TypeEntityKind::Enum,
            hafley_scm::lang::rust::TypeEntityKind::Alias => TypeEntityKind::Alias,
            hafley_scm::lang::rust::TypeEntityKind::Trait => TRAIT,
            hafley_scm::lang::rust::TypeEntityKind::Function => TypeEntityKind::Function,
            hafley_scm::lang::rust::TypeEntityKind::Method => TypeEntityKind::Method,
        };
        push_entity_raw(sink, strings, span, &row.name, kind);
        sink.aux
            .sigs
            .extend(row.sigs.into_iter().map(|sig| TypeSig {
                owner: span,
                slot: match sig.slot {
                    hafley_scm::lang::rust::SignatureSlot::Param => SigSlot::Param,
                    hafley_scm::lang::rust::SignatureSlot::Ret => SigSlot::Ret,
                },
                pos: sig.pos,
                ty: strings.intern(&sig.name),
            }));
    }
    const_values(tree, source, strings, sink);
    for row in rows.docs {
        sink.aux.docs.push(DocFact {
            owner: Span {
                start: row.range.start,
                len: row.range.end - row.range.start,
            },
            parent: row.parent.map(|name| strings.intern(&name)),
            text: strings.intern(&row.text),
            tags: row
                .sections
                .into_iter()
                .map(|section| DocTag {
                    tag: strings.intern("section"),
                    arg: Some(strings.intern(&section.heading)),
                    text: strings.intern(&section.body),
                })
                .collect(),
        });
    }
    // The candidates walk runs AFTER every entity is in the bundle so an
    // impl-owned candidate finds its in-file self-type entity regardless of
    // item order (v5's text-keyed pass has no order sensitivity; spans do).
    edge_candidates_from_tree(tree, source, strings, sink);
    impl_self_type_candidates(rows.impl_self_heads, strings, sink);
}

/// `impl Foo` and `impl Bar for Foo` reference `Foo` from the owner `Foo`: the
/// typedecl oracle walks every path under an impl and keys the block on its
/// self type, so the head is a row of its own. Bare heads only, the owner rule
/// `edge_candidates` applies (a qualified head is owned by its qualifier); the
/// owner is the in-file entity, else the `ImplOwner` minted at the head span.
fn impl_self_type_candidates(
    heads: Vec<hafley_scm::lang::rust::ImplSelfHeadRow>,
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    for head in heads {
        let head_span = Span {
            start: head.range.start,
            len: head.range.end - head.range.start,
        };
        let owner = sink
            .nodes
            .iter()
            .find(|node| {
                node.name
                    .map_or(false, |id| strings.lookup(id) == head.name)
            })
            .map(|node| node.span)
            .or_else(|| {
                sink.aux
                    .impl_owners
                    .iter()
                    .find(|owner| owner.span == head_span)
                    .map(|owner| owner.span)
            });
        let Some(owner) = owner else {
            continue;
        };
        sink.aux.candidates.push(TypeEdgeCandidate {
            owner,
            to: strings.intern(&head.name),
            kind: TypeEdgeKind::Uses,
        });
    }
}

fn push_entity_raw(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    span: Span,
    name: &str,
    kind: TypeEntityKind,
) {
    sink.nodes
        .push(Node::new(span, kind).with_name(strings.intern(name)));
}

// ── const facet: Const entities + ConstValue rows ───────────────────────────

/// Intern item-level string const rows from the same syn parse as the type arm.
fn const_values(
    tree: &tree_sitter::Tree,
    source: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    for row in hafley_scm::lang::rust::const_string_rows_from_tree(tree, source) {
        let span = Span {
            start: row.range.start,
            len: row.range.end - row.range.start,
        };
        push_entity_raw(sink, strings, span, &row.name, TypeEntityKind::Const);
        sink.aux.consts.push(ConstValue {
            owner: span,
            field: None,
            text: strings.intern(&row.value),
            kind: ConstKind::Lit,
        });
    }
}

// ════════════════════════════════════════════════════════════════════════════
// Resolve<TypeF> for RustSource.
// from the ts arm: the candidate row IS the parity target; text dsts STAY
// text — a candidate whose `to` names no corpus node (v5's synthetic
// `Owner::Member` variant text, externals) emits a ZERO dst leg. The
// genuinely-resolved span->blob legs are a v6-only ADDITIVE layer (reported,
// never asserted). Same-file blob leg: the TypeF node named `to` in THIS
// bundle gives the span, the DefIndex span-join gives the blob. Corpus
// fallback: a UNIQUE site only.
// ════════════════════════════════════════════════════════════════════════════

impl RustSource {
    /// The deduped, deterministically-ordered candidate list (v5's BTreeSet
    /// shaping): the aux candidates, deduped on (owner, to, kind). `resolve`
    /// emits its edges in EXACTLY this order, one per candidate — the parity
    /// golden zips the two (the zip discipline: edge i resolves candidate i).
    pub fn type_edge_candidates(output: &RyiOutput) -> Vec<TypeEdgeCandidate> {
        let mut set: BTreeSet<TypeEdgeCandidate> = BTreeSet::new();
        if let Some(types) = &output.types {
            for candidate in &types.aux.candidates {
                set.insert(candidate.clone());
            }
        }
        set.into_iter().collect()
    }
}

/// A candidate is interned AS WRITTEN (`hir::Struct`) and every index keys on a
impl Resolve<TypeF> for RustSource {
    fn resolve(&self, output: &RyiOutput, cx: &ProjectCx) -> Vec<ProjectEdge<TypeF>> {
        let Some(types) = &output.types else {
            return Vec::new();
        };
        let modules = cx.indexes.rust_modules.get();
        let checker = cx.indexes.rust_checker.get();
        let own_path = own_blob(cx, output)
            .zip(cx.indexes.paths.get())
            .and_then(|(blob, paths)| paths.get(&blob).map(str::to_string));
        let mut edges = Vec::new();
        for candidate in RustSource::type_edge_candidates(output) {
            // src: the TypeF entity at the owner span, else the `ImplOwner` at
            // it, addressed past the node vec the way a doc node is addressed.
            let Some(src_ix) = types
                .nodes
                .iter()
                .position(|node| node.span == candidate.owner)
                .or_else(|| {
                    types
                        .aux
                        .impl_owners
                        .iter()
                        .position(|owner| owner.span == candidate.owner)
                        .map(|ix| types.nodes.len() + ix)
                })
            else {
                continue;
            };
            let referenced = output.strings.lookup(candidate.to);
            let zero = (ZERO_CONTENT_ID, Span::empty(), ResolutionOrigin::Unresolved);
            let name_match = || {
                modules
                    .zip(own_path.as_deref())
                    .and_then(|(modules, from)| {
                        let segments = referenced
                            .split("::")
                            .map(str::to_string)
                            .collect::<Vec<_>>();
                        let bound = modules
                            .binding_at(
                                from,
                                &segments,
                                Some(candidate.owner.start),
                                FamilyTag::Type,
                            )
                            .ok()?;
                        bound.target_name?;
                        Some((
                            bound.target_blob,
                            bound.target_span,
                            ResolutionOrigin::ModulePlane,
                        ))
                    })
            };
            // The CHECKER tier answers first; a name one file resolves two ways
            // takes the answer nearest the owner.
            let checked = checker
                .zip(own_path.as_deref())
                .and_then(|(checker, from)| {
                    checker.type_at(
                        from,
                        type_probe_key(referenced, candidate.kind).1,
                        referenced,
                        candidate.owner,
                    )
                });
            if let Some(CheckerAnswer::Corpus(blob, span)) = checked {
                let edge = ProjectEdge::new(
                    NodeRef(src_ix as u32),
                    blob.clone(),
                    span,
                    candidate.kind,
                    ResolutionOrigin::Checker,
                );
                match cx.witness.then(name_match).flatten() {
                    Some((leg_blob, leg_span, leg)) if leg_blob == blob && leg_span == span => {
                        edges.push(edge.witnessed_by(leg));
                    }
                    Some((leg_blob, leg_span, leg)) => {
                        edges.push(edge);
                        edges.push(ProjectEdge::new(
                            NodeRef(src_ix as u32),
                            leg_blob,
                            leg_span,
                            candidate.kind,
                            leg,
                        ));
                    }
                    None => edges.push(edge),
                }
                continue;
            }
            // No corpus declaration IS this type, so no name-match leg may
            // invent one; the zero leg carries the row instead.
            let (dst_blob, dst_span, origin) = match checked {
                Some(CheckerAnswer::External(_)) => zero,
                _ => name_match().unwrap_or(zero),
            };
            edges.push(ProjectEdge::new(
                NodeRef(src_ix as u32),
                dst_blob,
                dst_span,
                candidate.kind,
                origin,
            ));
        }
        edges
    }
}
