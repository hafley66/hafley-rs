pub enum Walk {
    Ancestor,
    Parent,
    Descendant,
    Precedes,
    Follows,
    // NthChild(u32) over named siblings, ByteRange(u32, u32), Parent (Ancestor with Stop::Neighbor)
}
