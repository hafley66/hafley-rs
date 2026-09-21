use tree_sitter::QueryMatch;

use crate::types::QueryExt;
use crate::walk;

/// Every predicate of the match's own pattern holds. A predicate whose capture is
/// absent from the match constrains nothing, so it folds to true.
pub fn holds_for_candidate(q: &QueryExt, found: &QueryMatch, kind_ids: &[Vec<u32>]) -> bool {
    q.predicates
        .iter()
        .filter(|p| p.pattern as usize == found.pattern_index)
        .all(|p| {
            found
                .captures
                .iter()
                .filter(|capture| capture.index as u16 == p.capture)
                .all(|capture| {
                    walk::holds(p, capture.node, &kind_ids[p.kind as usize]) != p.negated
                })
        })
}
