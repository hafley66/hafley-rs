#[derive(Debug)]
pub enum QueryExtError {
    Parse(tree_sitter::QueryError),
    /// The `scmpp` paren reader rejected the query text.
    Scmpp(crate::scmpp::ScmppError),
    /// Top-level pattern `pattern` carries `#op` (a relation or `contains?`); `ryii query --scmpp` evaluates it.
    ScmppOnly { pattern: u16, op: String },
    UnknownOperator { pattern: u16, operator: String },
    Arity { operator: String, got: usize },
    DuplicateField(String),
    MatchLimit { file: String },
}

impl std::fmt::Display for QueryExtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "{error}"),
            Self::Scmpp(error) => write!(f, "{error}"),
            Self::ScmppOnly { pattern, op } => {
                write!(f, "pattern {pattern}: #{op} runs only under ryii query --scmpp")
            }
            Self::UnknownOperator { pattern, operator } => {
                write!(f, "pattern {pattern}: unknown predicate #{operator}")
            }
            Self::Arity { operator, got } => write!(f, "predicate #{operator} got {got} arguments"),
            Self::DuplicateField(key) => write!(f, "emission repeats field {key}"),
            Self::MatchLimit { file } => write!(f, "query match limit exceeded on '{file}'"),
        }
    }
}

impl std::error::Error for QueryExtError {}
