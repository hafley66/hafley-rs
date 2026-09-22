use tree_sitter::{QueryPredicate, QueryPredicateArg};

use crate::types::{Predicate, PredicateKind, QueryExtError, Stop, Walk};

/// One `QueryPredicate` -> one `Predicate`, appending its kind string to `kinds`
/// when that string is new. `kinds` is the deduped kind table the stage returns.
pub fn parse_into_predicate(
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
        "contains?" => {
            if args.is_empty() || args.iter().any(|arg| !matches!(arg, QueryPredicateArg::String(_))) {
                return Err(arity(found.args.len()));
            }
            let start = literals.len() as u16;
            for arg in args {
                let QueryPredicateArg::String(literal) = arg else { unreachable!() };
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
        "has-ancestor?" | "has-parent?" => {
            if args.is_empty() || args.iter().any(|arg| !matches!(arg, QueryPredicateArg::String(_))) {
                return Err(arity(found.args.len()));
            }
            let mut stop = Stop::End;
            let mut kind_args = args;
            if bare == "has-ancestor?" && kind_args.len() > 1 {
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
                let QueryPredicateArg::String(kind) = arg else { unreachable!() };
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
        "has?" => {
            let (kind, stop) = match args {
                [QueryPredicateArg::String(kind)] => (kind, Stop::End),
                [QueryPredicateArg::String(kind), QueryPredicateArg::String(stop)] => {
                    let stop = if stop.as_ref() == "neighbor" {
                        Stop::Neighbor
                    } else {
                        Stop::End
                    };
                    (kind, stop)
                }
                _ => return Err(arity(found.args.len())),
            };
            let index = match kinds.iter().position(|seen| seen.as_ref() == kind.as_ref()) {
                Some(index) => index,
                None => {
                    kinds.push(kind.clone());
                    kinds.len() - 1
                }
            };
            let start = predicate_kinds.len() as u16;
            predicate_kinds.push(index as u16);
            Ok(Predicate {
                pattern,
                capture,
                kind: PredicateKind::Node {
                    kinds: start..predicate_kinds.len() as u16,
                    walk: Walk::Descendant,
                    stop,
                },
                negated,
            })
        }
        _ => Err(QueryExtError::UnknownOperator(operator.to_string())),
    }
}
