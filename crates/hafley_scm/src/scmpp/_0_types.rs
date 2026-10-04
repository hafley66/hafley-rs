/// Output of the front end: plain tree-sitter text per level, the join plan, its SQL.
pub struct Compiled {
    pub patterns: Vec<FlatPattern>,
    pub plan: Level,
    pub sql: String,
    /// Capture names the SQL names by id: entry `i` has id `i + 1`, `__root` first.
    pub captures: Vec<Box<str>>,
    /// Relation fields the SQL names by id: entry `i` has id `i + 1`; 0 is no field.
    pub fields: Vec<Box<str>>,
    /// Result columns that hold a JSON array (`rows: list`), in SELECT order.
    pub lists: Vec<Box<str>>,
}

pub struct FlatPattern {
    pub id: u16,
    /// Plain tree-sitter query; the level root is captured as `@__root`.
    pub text: String,
    pub captures: Vec<Box<str>>,
    pub query: tree_sitter::Query,
}

pub struct Level {
    pub pattern: u16,
    pub rels: Vec<Rel>,
    pub conds: Vec<Cond>,
}

pub struct Rel {
    pub from: CapRef,
    pub walk: Walk,
    pub neighbor: bool,
    pub stop: Option<Box<Level>>,
    pub field: Option<Box<str>>,
    pub rows: Rows,
    /// `optional: true`: the outer match stays when no node relates; `rows: each` captures are null then.
    pub optional: bool,
    pub negated: bool,
    /// `None` only for `nth-child N` without `of`.
    pub target: Option<Box<Level>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Walk {
    Parent,
    Ancestor,
    Descendant,
    Precedes,
    Follows,
    NthChild(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rows {
    First,
    Each,
    /// One JSON array column per outer match; zero related nodes is `[]`.
    List,
}

/// `level` is the nesting depth inside one scope; stop and `of` levels open a new scope at 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapRef {
    pub level: u8,
    pub name: Box<str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cond {
    TextEq(CapRef, CapRef),
    TextMatch(CapRef, Box<str>),
    Contains(CapRef, Vec<Box<str>>),
    /// Inner capture and enclosing capture of one name: the same node.
    Same(CapRef, CapRef),
    Not(Box<Cond>),
}

#[derive(Debug)]
pub enum ScmppError {
    Syntax {
        offset: usize,
        message: String,
    },
    Query {
        pattern: u16,
        text: String,
        error: tree_sitter::QueryError,
    },
    Unsupported(String),
}

impl std::fmt::Display for ScmppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Syntax { offset, message } => {
                write!(f, "scm++ syntax at byte {offset}: {message}")
            }
            Self::Query {
                pattern,
                text,
                error,
            } => {
                write!(f, "scm++ level {pattern} `{text}`: {error}")
            }
            Self::Unsupported(message) => write!(f, "scm++: {message}"),
        }
    }
}

impl std::error::Error for ScmppError {}
