use tree_sitter::QueryCursor;

use crate::types::QueryExtError;

/// ts: the one match-limit check in the crate, read after the iterator is dropped.
/// A true is a named stop, never a silent partial.
pub fn match_limit_check(cursor: &QueryCursor, file: &str) -> Result<(), QueryExtError> {
    if cursor.did_exceed_match_limit() {
        return Err(QueryExtError::MatchLimit {
            file: file.to_string(),
        });
    }
    Ok(())
}
