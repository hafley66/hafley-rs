#[derive(Debug)]
pub enum QueryExtError {
    Parse(tree_sitter::QueryError),
    UnknownOperator(String),
    Arity { operator: String, got: usize },
    MatchLimit { file: String },
}
