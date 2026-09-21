use tree_sitter::Query;

use super::parse_into_predicate::parse_into_predicate;
use crate::types::{Predicate, QueryExtError};

/// ts: `user.general_predicates(i)` per pattern; unknown operator or bad arity is an error.
pub fn read_and_parse_predicates(
    user: &Query,
) -> Result<(Vec<Predicate>, Vec<Box<str>>), QueryExtError> {
    let mut predicates = Vec::new();
    let mut kinds = Vec::new();
    for pattern in 0..user.pattern_count() {
        for found in user.general_predicates(pattern) {
            predicates.push(parse_into_predicate(pattern as u16, found, &mut kinds)?);
        }
    }
    Ok((predicates, kinds))
}
