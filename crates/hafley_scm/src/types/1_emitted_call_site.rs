use std::ops::Range;

/// Byte ranges and a query-owned literal index; no source lifetime enters the arena.
pub struct EmittedCallSite {
    pub file: u16,
    pub group: Range<u32>,
    pub span: Range<u32>,
    pub callee_bytes: Option<Range<u32>>,
    pub callee_literal: Option<u16>,
}
