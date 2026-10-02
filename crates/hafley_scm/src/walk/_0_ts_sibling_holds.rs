use tree_sitter::Node;

use super::dispatch_by_direction::is_hit;
use crate::types::{Stop, Walk};

pub fn sibling_holds(node: Node, walk: &Walk, stop: &Stop, kind_ids: &[usize]) -> bool {
    let mut sibling = match walk {
        Walk::Precedes => node.next_named_sibling(),
        _ => node.prev_named_sibling(),
    };
    while let Some(related) = sibling {
        if is_hit(related, kind_ids) {
            return true;
        }
        if matches!(stop, Stop::Neighbor) {
            return false;
        }
        sibling = match walk {
            Walk::Precedes => related.next_named_sibling(),
            _ => related.prev_named_sibling(),
        };
    }
    false
}
