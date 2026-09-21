use tree_sitter::{QueryPredicate, QueryPredicateArg};

use crate::types::{Predicate, QueryExtError, Stop, Walk};

/// One `QueryPredicate` -> one `Predicate`, appending its kind string to `kinds`
/// when that string is new. `kinds` is the deduped kind table the stage returns.
pub fn parse_into_predicate(
    pattern: u16,
    found: &QueryPredicate,
    kinds: &mut Vec<Box<str>>,
) -> Result<Predicate, QueryExtError> {
    let operator = found.operator.as_ref();
    let (negated, bare) = match operator.strip_prefix("not-") {
        Some(rest) => (true, rest),
        None => (false, operator),
    };
    let walk = match bare {
        "has-ancestor?" => Walk::Ancestor,
        "has?" => Walk::Descendant,
        _ => return Err(QueryExtError::UnknownOperator(operator.to_string())),
    };
    let arity = |got: usize| QueryExtError::Arity {
        operator: operator.to_string(),
        got,
    };
    let (capture, kind, stop) = match &*found.args {
        [QueryPredicateArg::Capture(capture), QueryPredicateArg::String(kind)] => {
            (*capture, kind, Stop::End)
        }
        [QueryPredicateArg::Capture(capture), QueryPredicateArg::String(kind), QueryPredicateArg::String(stop)] =>
        {
            // The third argument is `neighbor` or `end`; absent or `end` is `Stop::End`.
            let stop = if stop.as_ref() == "neighbor" {
                Stop::Neighbor
            } else {
                Stop::End
            };
            (*capture, kind, stop)
        }
        args => return Err(arity(args.len())),
    };
    let kind = match kinds.iter().position(|seen| seen.as_ref() == kind.as_ref()) {
        Some(index) => index,
        None => {
            kinds.push(kind.clone());
            kinds.len() - 1
        }
    };
    Ok(Predicate {
        pattern,
        capture: capture as u16,
        kind: kind as u16,
        walk,
        stop,
        negated,
    })
}
