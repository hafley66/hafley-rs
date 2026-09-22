use tree_sitter::{QueryCursor, StreamingIterator, Tree};

use crate::types::QueryExt;

/// ts: one cursor per kind query, node ids collected and sorted before any candidate is tested.
pub fn kind_cursors_into_sorted_ids(q: &QueryExt, tree: &Tree, src: &[u8]) -> Vec<Vec<u32>> {
    q.kinds
        .iter()
        .map(|kind| {
            let mut cursor = QueryCursor::new();
            let mut found = cursor.matches(kind, tree.root_node(), src);
            let mut ids = Vec::new();
            while let Some(one) = found.next() {
                ids.extend(one.captures.iter().map(|capture| capture.node.id() as u32));
            }
            ids.sort_unstable();
            ids.dedup();
            ids
        })
        .collect()
}
