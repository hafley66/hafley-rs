#[derive(Debug)]
pub enum QueryError {
    Parse(String),
    ScmOnly { pattern: u16, op: String },
}
