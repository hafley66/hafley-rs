use tree_sitter::{Language, Query, QueryPredicate, QueryPredicateArg};

use super::_0_ts_nested_arguments::{pattern_query, PATTERN_PREFIX};
use crate::types::{Predicate, PredicateKind, QueryExtError, Stop, Walk};

fn nested(
    language: &Language,
    value: &str,
    patterns: &[String],
) -> Result<Option<Query>, QueryExtError> {
    let Some(index) = value.strip_prefix(PATTERN_PREFIX) else {
        return Ok(None);
    };
    let text = index
        .parse::<usize>()
        .ok()
        .and_then(|index| patterns.get(index))
        .ok_or_else(|| QueryExtError::UnknownOperator(value.to_owned()))?;
    pattern_query(language, text).map(Some)
}

pub fn parse_into_predicate(
    language: &Language,
    pattern: u16,
    found: &QueryPredicate,
    kinds: &mut Vec<Box<str>>,
    predicate_kinds: &mut Vec<u16>,
    literals: &mut Vec<Box<[u8]>>,
    patterns: &[String],
    names: &[Box<str>],
) -> Result<Predicate, QueryExtError> {
    let operator = found.operator.as_ref();
    let (negated, bare) = operator
        .strip_prefix("not-")
        .map_or((false, operator), |rest| (true, rest));
    let arity = || QueryExtError::Arity {
        operator: operator.to_owned(),
        got: found.args.len(),
    };
    let capture = match found.args.first() {
        Some(QueryPredicateArg::Capture(capture)) => *capture as u16,
        Some(QueryPredicateArg::String(name))
            if name.starts_with(super::_0_ts_nested_arguments::CAPTURE_PREFIX) =>
        {
            let name = name
                .strip_prefix(super::_0_ts_nested_arguments::CAPTURE_PREFIX)
                .unwrap();
            names
                .iter()
                .position(|seen| seen.as_ref() == name)
                .ok_or_else(arity)? as u16
        }
        _ => return Err(arity()),
    };

    let args = &found.args[1..];
    let strings = args
        .iter()
        .map(|arg| match arg {
            QueryPredicateArg::String(value) => Ok(value.as_ref()),
            _ => Err(arity()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let kind = match bare {
        "contains?" => {
            if strings.is_empty() {
                return Err(arity());
            }
            let start = literals.len() as u16;
            literals.extend(strings.iter().map(|value| value.as_bytes().into()));
            PredicateKind::Contains {
                literals: start..literals.len() as u16,
            }
        }
        "nth-child?" => {
            let index = strings
                .first()
                .and_then(|index| index.parse::<u32>().ok())
                .filter(|index| *index > 0)
                .ok_or_else(arity)?;
            let mut kind = None;
            let mut query = None;
            if strings.len() != 1 {
                if strings.len() != 3 || strings[1] != "of" {
                    return Err(arity());
                }
                query = nested(language, strings[2], patterns)?;
                if query.is_none() {
                    let id = language.id_for_node_kind(strings[2], true);
                    if id == 0 {
                        return Err(QueryExtError::UnknownOperator(format!(
                            "{operator} (unknown of kind '{}' in pattern {pattern})",
                            strings[2]
                        )));
                    }
                    kind = Some(id);
                }
            }
            PredicateKind::NthChild { index, kind, query }
        }
        "has?" | "has-ancestor?" | "has-parent?" | "precedes?" | "follows?" => {
            let mut stop = Stop::End;
            let mut field = None;
            let mut query = None;
            let start = predicate_kinds.len() as u16;
            let mut i = 0;
            let mut stop_seen = false;
            while i < strings.len() {
                let value = strings[i];
                match value {
                    "stopBy" => {
                        if stop_seen {
                            return Err(arity());
                        }
                        i += 1;
                        let value = *strings.get(i).ok_or_else(arity)?;
                        stop = match value {
                            "neighbor" => Stop::Neighbor,
                            "end" => Stop::End,
                            _ => Stop::Rule(nested(language, value, patterns)?.ok_or_else(arity)?),
                        };
                        stop_seen = true;
                    }
                    "field" => {
                        if field.is_some() {
                            return Err(arity());
                        }
                        i += 1;
                        let name = *strings.get(i).ok_or_else(arity)?;
                        field = Some(
                            language
                                .field_id_for_name(name)
                                .ok_or_else(|| {
                                    QueryExtError::UnknownOperator(format!(
                                        "{operator} (unknown field '{name}' in pattern {pattern})"
                                    ))
                                })?
                                .get(),
                        );
                    }
                    "neighbor" | "end" if i + 1 == strings.len() => {
                        if stop_seen {
                            return Err(arity());
                        }
                        stop = if value == "neighbor" {
                            Stop::Neighbor
                        } else {
                            Stop::End
                        };
                        stop_seen = true;
                    }
                    _ => {
                        if let Some(found) = nested(language, value, patterns)? {
                            if query.is_some() || predicate_kinds.len() as u16 != start {
                                return Err(arity());
                            }
                            query = Some(found);
                        } else {
                            if query.is_some() {
                                return Err(arity());
                            }
                            if language.id_for_node_kind(value, true) == 0 {
                                return Err(QueryExtError::UnknownOperator(format!(
                                    "{operator} (unknown kind '{value}' in pattern {pattern})"
                                )));
                            }
                            let index = kinds
                                .iter()
                                .position(|seen| seen.as_ref() == value)
                                .unwrap_or_else(|| {
                                    kinds.push(value.into());
                                    kinds.len() - 1
                                });
                            predicate_kinds.push(index as u16);
                        }
                    }
                }
                i += 1;
            }
            if query.is_none() && predicate_kinds.len() as u16 == start {
                return Err(arity());
            }
            let walk = match bare {
                "has?" => Walk::Descendant,
                "has-parent?" => Walk::Parent,
                "precedes?" => Walk::Precedes,
                "follows?" => Walk::Follows,
                _ => Walk::Ancestor,
            };
            PredicateKind::Node {
                kinds: start..predicate_kinds.len() as u16,
                walk,
                stop,
                query,
                field,
            }
        }
        _ => return Err(QueryExtError::UnknownOperator(operator.to_owned())),
    };
    Ok(Predicate {
        pattern,
        capture,
        kind,
        negated,
    })
}
