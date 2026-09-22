use tree_sitter::Node;

use super::dispatch_by_direction::is_hit;
use crate::types::Stop;

/// ts: a `TreeCursor` over all children for `Stop::Neighbor`, an iterative preorder
/// over every strict descendant for `Stop::End`. Named and anonymous alike; self skipped.
pub fn descendant_holds(node: Node, stop: &Stop, kind_ids: &[u32]) -> bool {
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return false;
    }
    if matches!(stop, Stop::Neighbor) {
        loop {
            if is_hit(cursor.node(), kind_ids) {
                return true;
            }
            if !cursor.goto_next_sibling() {
                return false;
            }
        }
    }
    let mut depth = 1usize;
    loop {
        if is_hit(cursor.node(), kind_ids) {
            return true;
        }
        if cursor.goto_first_child() {
            depth += 1;
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return false;
            }
            depth -= 1;
            if depth == 0 {
                return false;
            }
        }
    }
}
