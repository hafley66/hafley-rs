use tree_sitter::Query;

use super::parse_into_predicate::parse_into_predicate;
use crate::types::{Predicate, QueryExtError};

type ParsedPredicates = (Vec<Predicate>, Vec<Box<str>>, Vec<u16>, Vec<Box<[u8]>>);

/// ts: `user.general_predicates(i)` per pattern; unknown operator or bad arity is an error.
pub fn read_and_parse_predicates(
    user: &Query,
) -> Result<ParsedPredicates, QueryExtError> {
    let mut predicates = Vec::new();
    let mut kinds = Vec::new();
    let mut predicate_kinds = Vec::new();
    let mut literals = Vec::new();
    for pattern in 0..user.pattern_count() {
        for found in user.general_predicates(pattern) {
            predicates.push(parse_into_predicate(
                pattern as u16,
                found,
                &mut kinds,
                &mut predicate_kinds,
                &mut literals,
            )?);
        }
    }
    Ok((predicates, kinds, predicate_kinds, literals))
}
