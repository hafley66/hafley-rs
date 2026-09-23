use tree_sitter::{QueryCursor, StreamingIterator, Tree};

use super::append_to_match_arena::append_match;
use super::test_predicates_per_candidate::holds_for_candidate;
use super::ts_match_limit_check::match_limit_check;
use crate::types::{MatchArena, QueryExt, QueryExtError};

/// ts: user cursor with `set_match_limit`; each kept match appends spans then one row.
pub fn user_cursor_into_arena(
    q: &QueryExt,
    tree: &Tree,
    src: &[u8],
    limit: u32,
    kind_ids: &[Vec<u32>],
    file: u16,
    arena: &mut MatchArena,
) -> Result<(), QueryExtError> {
    let mut cursor = QueryCursor::new();
    cursor.set_match_limit(limit);
    let mut found = cursor.matches(&q.user, tree.root_node(), src);
    while let Some(one) = found.next() {
        if holds_for_candidate(q, one, src, kind_ids) {
            append_match(q, one, file, arena);
        }
    }
    drop(found);
    match_limit_check(&cursor, &arena.files[file as usize])
}
