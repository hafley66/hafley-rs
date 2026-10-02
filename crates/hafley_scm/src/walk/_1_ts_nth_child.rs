use tree_sitter::Node;

use super::dispatch_by_direction::is_hit;

/// `of` holds the node ids a minted `(kind) @_` query matched, so supertypes expand the way `has` does.
pub fn holds(node: Node, index: u32, of: Option<&[usize]>) -> bool {
    let fits = |node: Node| of.is_none_or(|ids| is_hit(node, ids));
    if !node.is_named() || node.parent().is_none() || !fits(node) {
        return false;
    }
    let mut position = 1;
    let mut sibling = node.prev_named_sibling();
    while let Some(previous) = sibling {
        if fits(previous) {
            position += 1;
        }
        sibling = previous.prev_named_sibling();
    }
    position == index
}
