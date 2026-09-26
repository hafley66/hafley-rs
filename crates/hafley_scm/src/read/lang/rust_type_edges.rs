//! Rust type-edge candidate collection: the unresolved TypeF candidates
//! (field/variant/generic/impl/uses) that `Resolve<TypeF>` binds. Port of v5
//! `edges_from`.

use hafley_scm::lang::rust::{
    type_candidate_rows, tsi_syntax_rows, TsiSyntaxArg, TypeCandidateKind, TypeCandidateOwner,
};
use crate::read::family::{ImplOwner, TypeEdgeCandidate, TypeEdgeKind, TypeF};
use crate::read::rows::FamilyBundle;
use crate::read::shape::{Span, Strings};
use crate::read::tsi::{Arg, FactOut};

// ── type-edge candidates (the Resolve<TypeF> input) ───────────────
//
// A candidate carries an owner SPAN. An impl whose self type is declared
// outside this file points at an `ImplOwner` instead of a node.

/// Collect one file's unresolved type-edge candidates. Port of v5 `edges_from`
/// + `item_edges`.
pub fn edge_candidates(
    parsed: &syn::File,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    for group in type_candidate_rows(parsed, line_starts) {
        let owner = match group.owner {
            TypeCandidateOwner::Declared(range) => Span {
                start: range.start,
                len: range.end - range.start,
            },
            TypeCandidateOwner::Synthetic { range, name } => impl_owner_span(
                sink,
                strings,
                Span { start: range.start, len: range.end - range.start },
                &name,
            ),
            TypeCandidateOwner::Impl {
                primary_name,
                bare_head,
            } => {
                if let Some(span) = entity_span_named(sink, strings, &primary_name) {
                    span
                } else {
                    let Some((range, head)) = bare_head else {
                        continue;
                    };
                    impl_owner_span(
                        sink,
                        strings,
                        Span {
                            start: range.start,
                            len: range.end - range.start,
                        },
                        &head,
                    )
                }
            }
        };
        for row in group.candidates {
            sink.aux.candidates.push(TypeEdgeCandidate {
                owner,
                to: strings.intern(&row.to),
                kind: match row.kind {
                    TypeCandidateKind::Field => TypeEdgeKind::Field,
                    TypeCandidateKind::Variant => TypeEdgeKind::Variant,
                    TypeCandidateKind::Generic => TypeEdgeKind::Generic,
                    TypeCandidateKind::Impl => TypeEdgeKind::Impl,
                    TypeCandidateKind::Uses => TypeEdgeKind::Uses,
                    TypeCandidateKind::Param => TypeEdgeKind::Param,
                    TypeCandidateKind::Returns => TypeEdgeKind::Returns,
                },
            });
        }
    }
    let span = crate::read::trace::phase_span("rust", crate::read::trace::Phase::TsiSyntax);
    let _entered = span.enter();
    let rows = tsi_syntax_rows(parsed, line_starts);
    for name in rows.interned { strings.intern(&name); }
    sink.aux.tsi = rows.facts.into_iter().map(|row| FactOut {
        fact: row.fact,
        relation: row.relation,
        args: row.args.into_iter().map(|arg| match arg {
            TsiSyntaxArg::Id(id) => Arg::Id(id),
            TsiSyntaxArg::Span(digest, start, end) => Arg::Span(digest, start, end),
            TsiSyntaxArg::Text(text) => Arg::Text(text),
            TsiSyntaxArg::Int(number) => Arg::Int(number),
            TsiSyntaxArg::Atom(atom) => Arg::Atom(atom),
        }).collect(),
    }).collect();
    crate::read::trace::record_phase(&span, 0, sink.aux.tsi.len() as u64, 1);
}

/// The span of the TypeF entity interned as `name` in this bundle (the owner
/// leg of an impl-owned candidate: v5's from-text is the self-type name, which
/// the in-file entity carries verbatim).
fn entity_span_named(sink: &FamilyBundle<TypeF>, strings: &Strings, name: &str) -> Option<Span> {
    sink.nodes
        .iter()
        .find(|node| node.name.map_or(false, |id| strings.lookup(id) == name))
        .map(|node| node.span)
}

/// Record an owner the file declares nowhere and hand back its span. Deduped on
/// span, so every impl of one type in one file shares the entry.
fn impl_owner_span(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    span: Span,
    name: &str,
) -> Span {
    if !sink.aux.impl_owners.iter().any(|owner| owner.span == span) {
        let name = strings.intern(name);
        sink.aux.impl_owners.push(ImplOwner { span, name });
    }
    span
}
