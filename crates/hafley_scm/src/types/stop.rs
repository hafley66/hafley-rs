pub enum Stop {
    Neighbor,
    End,
    Rule(tree_sitter::Query),
}
