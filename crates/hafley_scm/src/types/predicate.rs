use std::ops::Range;

pub enum PredicateKind {
    Node {
        kinds: Range<u16>,
        walk: super::Walk,
        stop: super::Stop,
        query: Option<tree_sitter::Query>,
        field: Option<u16>,
    },
    NthChild {
        index: u32,
        kind: Option<u16>,
        query: Option<tree_sitter::Query>,
    },
    Contains {
        literals: Range<u16>,
    },
}

/// One general predicate tree-sitter handed back unevaluated.
pub struct Predicate {
    pub pattern: u16,
    pub capture: u16,
    pub kind: PredicateKind,
    pub negated: bool,
}
