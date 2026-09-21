use tree_sitter::{Language, Query};

use super::super::split_predicates_into_kind_queries::mint_kind_query_text;
use crate::types::QueryExtError;

/// ts: one `Query::new` per deduped kind, built once per language.
pub fn query_new_per_kind(
    language: &Language,
    kinds: &[Box<str>],
) -> Result<Vec<Query>, QueryExtError> {
    kinds
        .iter()
        .map(|kind| Query::new(language, &mint_kind_query_text(kind)).map_err(QueryExtError::Parse))
        .collect()
}
