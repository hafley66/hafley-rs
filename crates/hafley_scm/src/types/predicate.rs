use std::ops::Range;

pub enum PredicateKind {
    Node {
        kinds: Range<u16>,
        walk: super::Walk,
        stop: super::Stop,
    },
    NthChild {
        index: u32,
        kind: Option<u16>,
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
