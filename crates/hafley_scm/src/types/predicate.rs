/// One `(#op? @capture "kind" ["stop"])` clause tree-sitter handed back unevaluated.
pub struct Predicate {
    pub pattern: u16,
    pub capture: u16,
    pub kind: u16,
    pub walk: super::Walk,
    pub stop: super::Stop,
    pub negated: bool,
}
