use tree_sitter::{Query, QueryPredicate, QueryPredicateArg};

use super::parse_into_predicate::parse_into_predicate;
use crate::types::{CallSiteEmit, Predicate, QueryExtError};

type ParsedPredicates = (
    Vec<Predicate>,
    Vec<Box<str>>,
    Vec<u16>,
    Vec<Box<[u8]>>,
    Vec<CallSiteEmit>,
    Vec<Box<str>>,
);

/// ts: `user.general_predicates(i)` per pattern; unknown operator or bad arity is an error.
pub fn read_and_parse_predicates(
    user: &Query,
) -> Result<ParsedPredicates, QueryExtError> {
    let mut predicates = Vec::new();
    let mut kinds = Vec::new();
    let mut predicate_kinds = Vec::new();
    let mut literals = Vec::new();
    let mut call_site_emits = Vec::new();
    let mut call_site_literals = Vec::new();
    for pattern in 0..user.pattern_count() {
        for found in user.general_predicates(pattern) {
            if found.operator.as_ref() == "emit-call-site!" {
                call_site_emits.push(parse_call_site_emit(
                    pattern as u16,
                    found,
                    &mut call_site_literals,
                )?);
                continue;
            }
            predicates.push(parse_into_predicate(
                pattern as u16,
                found,
                &mut kinds,
                &mut predicate_kinds,
                &mut literals,
            )?);
        }
    }
    Ok((predicates, kinds, predicate_kinds, literals, call_site_emits, call_site_literals))
}

fn parse_call_site_emit(
    pattern: u16,
    found: &QueryPredicate,
    literals: &mut Vec<Box<str>>,
) -> Result<CallSiteEmit, QueryExtError> {
    let bad_args = || QueryExtError::Arity {
        operator: found.operator.to_string(),
        got: found.args.len(),
    };
    let [QueryPredicateArg::Capture(group), QueryPredicateArg::Capture(span), callee] =
        found.args.as_ref()
    else {
        return Err(bad_args());
    };
    let (callee_capture, callee_literal) = match callee {
        QueryPredicateArg::Capture(capture) => (Some(*capture as u16), None),
        QueryPredicateArg::String(text) => {
            let index = literals.len() as u16;
            literals.push(text.clone());
            (None, Some(index))
        }
    };
    Ok(CallSiteEmit {
        pattern,
        group: *group as u16,
        span: *span as u16,
        callee_capture,
        callee_literal,
    })
}
