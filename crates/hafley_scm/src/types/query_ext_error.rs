#[derive(Debug)]
pub enum QueryExtError {
    Parse(tree_sitter::QueryError),
    /// The `scmpp` paren reader rejected the query text.
    Scmpp(crate::scmpp::ScmppError),
    /// Top-level pattern `pattern` carries `#op` (a relation or `contains?`); `ryii query --scmpp` evaluates it.
    ScmppOnly { pattern: u16, op: String },
    UnknownOperator(String),
    Arity { operator: String, got: usize },
    DuplicateField(String),
    MatchLimit { file: String },
}
