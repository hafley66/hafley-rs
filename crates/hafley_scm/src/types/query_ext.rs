/// The user's `.scm` compiled once per language, plus one minted query per node kind
/// a predicate names. Built once, run over many files.
pub struct QueryExt {
    pub user: tree_sitter::Query,
    pub kinds: Vec<tree_sitter::Query>,
    pub predicates: Vec<super::Predicate>,
    pub names: Vec<Box<str>>,
    pub predicate_kinds: Vec<u16>,
    pub literals: Vec<Box<[u8]>>,
}
