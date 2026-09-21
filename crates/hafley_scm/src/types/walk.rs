pub enum Walk {
    Ancestor,
    Descendant,
    // TODO ast-grep overlap, earmarked, not built: Precedes, Follows (sibling walks),
    // NthChild(u32) over named siblings, ByteRange(u32, u32), Parent (Ancestor with Stop::Neighbor)
}
