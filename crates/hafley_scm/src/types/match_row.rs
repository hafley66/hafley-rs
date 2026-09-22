use std::ops::Range;

pub struct MatchRow {
    pub file: u16,
    pub pattern: u16,
    pub spans: Range<u32>,
}
