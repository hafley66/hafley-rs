/// One field of a checked `#emit!` instruction. IDs index the query's dictionaries.
pub enum EmitSource {
    Capture(u16),
    Literal(u16),
}

pub struct EmitFieldSpec {
    pub key: u16,
    pub source: EmitSource,
}

/// One relation row produced for each match of a query pattern.
pub struct EmitSpec {
    pub pattern: u16,
    pub relation: u16,
    pub fields: Vec<EmitFieldSpec>,
}
