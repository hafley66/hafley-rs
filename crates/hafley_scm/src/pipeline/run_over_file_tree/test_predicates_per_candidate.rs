use tree_sitter::QueryMatch;

use crate::types::{PredicateKind, QueryExt};
use crate::walk;

/// Every predicate of the match's own pattern holds. A predicate whose capture is
/// absent from the match constrains nothing, so it folds to true.
pub fn holds_for_candidate(
    q: &QueryExt,
    found: &QueryMatch,
    src: &[u8],
    kind_ids: &[Vec<usize>],
) -> bool {
    q.predicates
        .iter()
        .filter(|p| p.pattern as usize == found.pattern_index)
        .all(|p| {
            found
                .captures()
                .iter()
                .filter(|capture| capture.index as u16 == p.capture)
                .all(|capture| {
                    let result = match &p.kind {
                        PredicateKind::Node { kinds, .. } => q.predicate_kinds
                            [kinds.start as usize..kinds.end as usize]
                            .iter()
                            .any(|kind| walk::holds(p, capture.node, &kind_ids[*kind as usize])),
                        PredicateKind::NthChild { index, of } => {
                            crate::walk::_1_ts_nth_child::holds(capture.node, *index, of.map(|of| kind_ids[of as usize].as_slice()))
                        },
                        PredicateKind::Contains { literals } => {
                            src.get(capture.node.byte_range()).is_some_and(|text| {
                                q.literals[literals.start as usize..literals.end as usize]
                                    .iter()
                                    .all(|literal| {
                                        text.windows(literal.len())
                                            .any(|part| part == literal.as_ref())
                                    })
                            })
                        }
                    };
                    result != p.negated
                })
        })
}
