#[derive(Debug)]
pub enum QueryExtError {
    Parse(tree_sitter::QueryError),
    /// A relation predicate's pattern, compiled or evaluated through `scmpp`.
    Scmpp(crate::scmpp::ScmppError),
    UnknownOperator(String),
    Arity { operator: String, got: usize },
    DuplicateField(String),
    MatchLimit { file: String },
}
