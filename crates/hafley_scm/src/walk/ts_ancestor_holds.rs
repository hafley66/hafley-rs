use tree_sitter::Node;

use super::dispatch_by_direction::is_hit;
use crate::types::Stop;

/// ts: `Node::parent()` once for `Stop::Neighbor`, in a loop to the root for `Stop::End`.
/// Strict: the node itself is never tested.
pub fn ancestor_holds(node: Node, stop: &Stop, kind_ids: &[u32]) -> bool {
    let mut parent = node.parent();
    while let Some(up) = parent {
        if is_hit(up, kind_ids) {
            return true;
        }
        if matches!(stop, Stop::Neighbor) {
            return false;
        }
        parent = up.parent();
    }
    false
}
