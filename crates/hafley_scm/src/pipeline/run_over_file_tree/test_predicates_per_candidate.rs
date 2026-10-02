use tree_sitter::{QueryCapture, QueryMatch};

use crate::types::{PredicateKind, QueryExt};
use crate::walk::_2_ts_related_captures::{nth_captures, related_captures};

// Related bindings remain local until all predicates accept the candidate.
// Negated relations never export captures from the excluded node.
pub fn captures_for_candidate<'tree>(
    q: &QueryExt,
    found: &QueryMatch<'_, 'tree>,
    src: &[u8],
    kind_ids: &[Vec<u32>],
) -> Option<Vec<QueryCapture<'tree>>> {
    let mut captures = found.captures().to_vec();
    for predicate in q
        .predicates
        .iter()
        .filter(|p| p.pattern as usize == found.pattern_index)
    {
        let targets = captures
            .iter()
            .filter(|capture| capture.index as u16 == predicate.capture)
            .map(|capture| capture.node)
            .collect::<Vec<_>>();
        for node in targets {
            let result = match &predicate.kind {
                PredicateKind::Node { .. } => {
                    related_captures(&predicate.kind, node, q, src, kind_ids)
                }
                PredicateKind::NthChild { index, kind, query } => {
                    nth_captures(node, *index, *kind, query.as_ref(), q, src)
                }
                PredicateKind::Contains { literals } => src
                    .get(node.byte_range())
                    .filter(|text| {
                        q.literals[literals.start as usize..literals.end as usize]
                            .iter()
                            .all(|literal| {
                                literal.is_empty()
                                    || text
                                        .windows(literal.len())
                                        .any(|part| part == literal.as_ref())
                            })
                    })
                    .map(|_| Vec::new()),
            };
            if result.is_some() == predicate.negated {
                return None;
            }
            if !predicate.negated {
                for capture in result.unwrap() {
                    if !captures.iter().any(|seen| {
                        seen.index == capture.index && seen.node.id() == capture.node.id()
                    }) {
                        captures.push(capture);
                    }
                }
            }
        }
    }
    Some(captures)
}
