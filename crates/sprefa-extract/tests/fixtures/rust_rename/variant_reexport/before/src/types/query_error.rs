#[derive(Debug)]
pub enum QueryError {
    Parse(String),
    Only { pattern: u16, op: String },
}
