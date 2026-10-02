use tree_sitter::Node;

pub fn holds(node: Node, index: u32, kind: Option<u16>) -> bool {
    if !node.is_named() || node.parent().is_none() || kind.is_some_and(|kind| node.kind_id() != kind) {
        return false;
    }
    let mut position = 1;
    let mut sibling = node.prev_named_sibling();
    while let Some(previous) = sibling {
        if kind.is_none_or(|kind| previous.kind_id() == kind) {
            position += 1;
        }
        sibling = previous.prev_named_sibling();
    }
    position == index
}
