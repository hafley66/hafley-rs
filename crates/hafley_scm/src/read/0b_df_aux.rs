use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DfFAux {
    pub functions: Vec<Option<NameId>>,
    pub param_fields: Vec<(NodeRef, String)>,
    pub params: Vec<DfParam>,
    pub args: Vec<DfArg>,
    pub fields: Vec<DfField>,
    pub lits: Vec<DfLit>,
    pub loops: Vec<DfLoop>,
    pub nests: Vec<DfNest>,
    pub allocates: Vec<DfAllocates>,
    /// Pending (node, start, end, kind) spans for `template`/`concat` rows,
    /// whose text is a source SLICE the per-node lift doesn't hold. The ts
    /// DfF projector drains this into `lits` once, at the end of the walk, where
    /// the file content is in hand (the same shape as v5's `lit_spans`).
    pub lit_spans: Vec<(NodeRef, u32, u32, &'static str)>,
    /// Pending `(index into loops, start, end)`, drained into
    /// `loops[index].collection` by the ts and rust projectors (`lit_spans` shape).
    pub loop_collection_spans: Vec<(usize, u32, u32)>,
    /// Scratch allocator-call spans, claimed by the INNERMOST enclosing callable:
    /// the rust closure arm and `project_df` roll their range up and truncate.
    pub allocator_hits: Vec<Span>,
}
