use tree_sitter::{Language, QueryPredicate, QueryPredicateArg};

use crate::types::{Predicate, PredicateKind, QueryExtError, Stop, Walk};

/// One `QueryPredicate` -> one `Predicate`, appending its kind string to `kinds`
/// when that string is new. `kinds` is the deduped kind table the stage returns.
/// `#has?` / `#not-has?`: `@capture kind+ [neighbor|end]`.
/// `#has-ancestor?` / `#not-has-ancestor?`: `@capture kind+ [neighbor|end]`.
/// `#has-parent?` / `#not-has-parent?`: `@capture kind+`.
pub fn parse_into_predicate(
    language: &Language,
    pattern: u16,
    found: &QueryPredicate,
    kinds: &mut Vec<Box<str>>,
    predicate_kinds: &mut Vec<u16>,
    literals: &mut Vec<Box<[u8]>>,
) -> Result<Predicate, QueryExtError> {
    let operator = found.operator.as_ref();
    let (negated, bare) = match operator.strip_prefix("not-") {
        Some(rest) => (true, rest),
        None => (false, operator),
    };
    let arity = |got: usize| QueryExtError::Arity {
        operator: operator.to_string(),
        got,
    };
    let capture = match found.args.first() {
        Some(QueryPredicateArg::Capture(capture)) => *capture as u16,
        _ => return Err(arity(found.args.len())),
    };
    let args = &found.args[1..];
    match bare {
        "nth-child?" => {
            let Some(QueryPredicateArg::String(index)) = args.first() else {
                return Err(arity(found.args.len()));
            };
            let index = index.parse::<u32>().ok().filter(|index| *index > 0)
                .ok_or_else(|| arity(found.args.len()))?;
            let kind = if args.len() == 1 {
                None
            } else if let [_, QueryPredicateArg::String(of), QueryPredicateArg::String(kind)] = args {
                if of.as_ref() != "of" || language.id_for_node_kind(kind, true) == 0 {
                    return Err(QueryExtError::UnknownOperator(format!("{operator} (unknown of kind '{kind}' in pattern {pattern})")));
                }
                Some(language.id_for_node_kind(kind, true))
            } else {
                return Err(arity(found.args.len()));
            };
            Ok(Predicate { pattern, capture, kind: PredicateKind::NthChild { index, kind }, negated })
        }
        "contains?" => {

            if args.is_empty()
                || args
                    .iter()
                    .any(|arg| !matches!(arg, QueryPredicateArg::String(_)))
            {
                return Err(arity(found.args.len()));
            }
            let start = literals.len() as u16;
            for arg in args {
                let QueryPredicateArg::String(literal) = arg else {
                    unreachable!()
                };
                literals.push(literal.as_bytes().into());
            }
            Ok(Predicate {
                pattern,
                capture,
                kind: PredicateKind::Contains {
                    literals: start..literals.len() as u16,
                },
                negated,
            })
        }
        "has?" | "has-ancestor?" | "has-parent?" | "precedes?" | "follows?" => {
            if args.is_empty()
                || args
                    .iter()
                    .any(|arg| !matches!(arg, QueryPredicateArg::String(_)))
            {
                return Err(arity(found.args.len()));
            }
            let mut stop = Stop::End;
            let mut kind_args = args;
            if bare != "has-parent?" && kind_args.len() > 1 {
                if let QueryPredicateArg::String(last) = &kind_args[kind_args.len() - 1] {
                    if last.as_ref() == "neighbor" || last.as_ref() == "end" {
                        stop = if last.as_ref() == "neighbor" {
                            Stop::Neighbor
                        } else {
                            Stop::End
                        };
                        kind_args = &kind_args[..kind_args.len() - 1];
                    }
                }
            }
            if kind_args.is_empty() {
                return Err(arity(found.args.len()));
            }
            let start = predicate_kinds.len() as u16;
            for arg in kind_args {
                let QueryPredicateArg::String(kind) = arg else {
                    unreachable!()
                };
                if language.id_for_node_kind(kind, true) == 0 {
                    return Err(QueryExtError::UnknownOperator(format!(
                        "{operator} (unknown kind '{kind}' in pattern {pattern})"
                    )));
                }
                let index = match kinds.iter().position(|seen| seen.as_ref() == kind.as_ref()) {
                    Some(index) => index,
                    None => {
                        kinds.push(kind.clone());
                        kinds.len() - 1
                    }
                };
                predicate_kinds.push(index as u16);
            }
            let walk = if bare == "has-parent?" {
                Walk::Parent
            } else if bare == "has?" {
                Walk::Descendant
            } else if bare == "precedes?" {
                Walk::Precedes
            } else if bare == "follows?" {
                Walk::Follows
            } else {
                Walk::Ancestor
            };
            Ok(Predicate {
                pattern,
                capture,
                kind: PredicateKind::Node {
                    kinds: start..predicate_kinds.len() as u16,
                    walk,
                    stop,
                },
                negated,
            })
        }
        _ => Err(QueryExtError::UnknownOperator(operator.to_string())),
    }
}
