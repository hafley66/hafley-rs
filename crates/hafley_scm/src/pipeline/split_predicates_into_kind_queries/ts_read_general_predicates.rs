use tree_sitter::{Query, QueryPredicate, QueryPredicateArg};

use super::parse_into_predicate::parse_into_predicate;
use crate::types::{EmitFieldSpec, EmitSource, EmitSpec, Predicate, QueryExtError};

type ParsedPredicates = (
    Vec<Predicate>,
    Vec<Box<str>>,
    Vec<u16>,
    Vec<Box<[u8]>>,
    Vec<EmitSpec>,
    Vec<Box<str>>,
    Vec<Box<str>>,
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
    let mut emits = Vec::new();
    let mut relations = Vec::new();
    let mut fields = Vec::new();
    let mut emit_literals = Vec::new();
    for pattern in 0..user.pattern_count() {
        for found in user.general_predicates(pattern) {
            if found.operator.as_ref() == "emit!" {
                emits.push(parse_emit(
                    pattern as u16,
                    found,
                    &mut relations,
                    &mut fields,
                    &mut emit_literals,
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
    Ok((predicates, kinds, predicate_kinds, literals, emits, relations, fields, emit_literals))
}

fn intern(value: &str, names: &mut Vec<Box<str>>) -> u16 {
    if let Some(index) = names.iter().position(|name| name.as_ref() == value) {
        return index as u16;
    }
    let index = names.len() as u16;
    names.push(value.into());
    index
}

fn parse_emit(
    pattern: u16,
    found: &QueryPredicate,
    relations: &mut Vec<Box<str>>,
    fields: &mut Vec<Box<str>>,
    literals: &mut Vec<Box<str>>,
) -> Result<EmitSpec, QueryExtError> {
    let bad_args = || QueryExtError::Arity {
        operator: found.operator.to_string(),
        got: found.args.len(),
    };
    if found.args.len() < 3 || found.args.len() % 2 == 0 {
        return Err(bad_args());
    }
    let QueryPredicateArg::String(relation) = &found.args[0] else {
        return Err(bad_args());
    };
    let mut emitted_fields = Vec::new();
    for pair in found.args[1..].chunks_exact(2) {
        let QueryPredicateArg::String(key) = &pair[0] else {
            return Err(bad_args());
        };
        let source = match &pair[1] {
            QueryPredicateArg::Capture(capture) => EmitSource::Capture(*capture as u16),
            QueryPredicateArg::String(text) => {
                let index = intern(text, literals);
                EmitSource::Literal(index)
            }
        };
        let key = intern(key, fields);
        if emitted_fields.iter().any(|field: &EmitFieldSpec| field.key == key) {
            return Err(QueryExtError::DuplicateField(key.to_string()));
        }
        emitted_fields.push(EmitFieldSpec { key, source });
    }
    Ok(EmitSpec {
        pattern,
        relation: intern(relation, relations),
        fields: emitted_fields,
    })
}
