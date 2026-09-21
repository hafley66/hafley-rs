use std::ops::Range;

/// The user's `.scm` compiled once per language, plus one minted query per node kind
/// a predicate names. Built once, run over many files.
pub struct QueryExt {
    pub user: tree_sitter::Query,
    pub kinds: Vec<tree_sitter::Query>,
    pub predicates: Vec<Predicate>,
    pub names: Vec<Box<str>>,
}

/// One `(#op? @capture "kind" ["stop"])` clause tree-sitter handed back unevaluated.
pub struct Predicate {
    pub pattern: u16,
    pub capture: u16,
    pub kind: u16,
    pub walk: Walk,
    pub stop: Stop,
    pub negated: bool,
}

pub enum Walk {
    Ancestor,
    Descendant,
    // TODO ast-grep overlap, earmarked, not built: Precedes, Follows (sibling walks),
    // NthChild(u32) over named siblings, ByteRange(u32, u32), Parent (Ancestor with Stop::Neighbor)
}

pub enum Stop {
    Neighbor,
    End,
    // TODO ast-grep `stopBy: rule`: Rule(u16) stops the walk at a second kind query
}

// TODO ast-grep overlap, earmarked: `any-` prefix, `#set!` as settings, `field:` on has/inside,
// `pattern:` $META text (is .scm), `fix:` rewrite (soopy), YAML rules, explain plan

/// Every match of every file in one run. Rows index into `spans` by range; no lifetimes.
#[derive(Default)]
pub struct MatchArena {
    pub files: Vec<Box<str>>,
    pub spans: Vec<CapturedSpan>,
    pub rows: Vec<MatchRow>,
}

pub struct MatchRow {
    pub file: u16,
    pub pattern: u16,
    pub spans: Range<u32>,
}

pub struct CapturedSpan {
    pub name: u16,
    pub bytes: Range<u32>,
}

#[derive(Debug)]
pub enum QueryExtError {
    Parse(tree_sitter::QueryError),
    UnknownOperator(String),
    Arity { operator: String, got: usize },
    MatchLimit { file: String },
}
