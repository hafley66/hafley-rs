use tree_sitter::Node;

use super::ts_ancestor_holds::ancestor_holds;
use super::ts_descendant_holds::descendant_holds;
use crate::types::{Predicate, Walk};

pub fn holds(p: &Predicate, node: Node, kind_ids: &[u32]) -> bool {
    match p.walk {
        Walk::Ancestor => ancestor_holds(node, &p.stop, kind_ids),
        Walk::Descendant => descendant_holds(node, &p.stop, kind_ids),
    }
}

/// The one membership test. Iterative walks only, so the id table is read, never grown.
pub fn is_hit(node: Node, kind_ids: &[u32]) -> bool {
    kind_ids.binary_search(&(node.id() as u32)).is_ok()
}
