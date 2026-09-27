use std::path::Path;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, QueryPredicateArg, Tree};

const MATCH_LIMIT: u32 = 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capture {
    pub label: String,
    pub text: String,
    pub kind: String,
    pub start: usize,
    pub end: usize,
    pub parent: Option<(usize, usize)>,
    pub parent_kind: Option<String>,
    pub ancestors: Vec<(usize, usize)>,
    pub ancestor_kinds: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryMatch {
    pub pattern_index: usize,
    pub captures: Vec<Capture>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryFailure {
    Syntax(String),
    UnknownPredicate(String),
    PredicateArity { operator: String, got: usize },
    UnboundCapture(String),
    MatchLimit { path: String },
}

#[derive(Clone, Debug)]
struct Predicate {
    operator: String,
    capture: Option<String>,
    target: Option<String>,
    stop_by: String,
}

#[derive(Clone, Debug)]
struct MatchedPattern {
    found: QueryMatch,
    predicates: Vec<Predicate>,
}

pub fn run_query(
    language: Language,
    source: &str,
    query_source: &str,
    path: &Path,
) -> Result<Vec<QueryMatch>, QueryFailure> {
    run_query_with_limit(language, source, query_source, path, MATCH_LIMIT)
}

pub fn run_query_with_limit(
    language: Language,
    source: &str,
    query_source: &str,
    path: &Path,
    match_limit: u32,
) -> Result<Vec<QueryMatch>, QueryFailure> {
    let query = Query::new(&language, query_source)
        .map_err(|error| QueryFailure::Syntax(error.to_string()))?;
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|error| QueryFailure::Syntax(error.to_string()))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| QueryFailure::Syntax("parser cancelled".into()))?;
    let mut cursor = QueryCursor::new();
    cursor.set_match_limit(match_limit);
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    let mut found = Vec::new();
    let mut predicate_error = None;
    while let Some(found_match) = matches.next() {
        let captures = found_match
            .captures
            .iter()
            .map(|capture| {
                own_capture(
                    capture.node,
                    query.capture_names()[capture.index as usize],
                    source,
                )
            })
            .collect();
        let predicates = query
            .general_predicates(found_match.pattern_index)
            .iter()
            .map(|predicate| decode_predicate(predicate, query.capture_names()))
            .collect::<Result<Vec<_>, _>>();
        let predicates = match predicates {
            Ok(predicates) => predicates,
            Err(error) => {
                predicate_error.get_or_insert(error);
                Vec::new()
            }
        };
        found.push(MatchedPattern {
            found: QueryMatch {
                pattern_index: found_match.pattern_index,
                captures,
            },
            predicates,
        });
    }
    if cursor.did_exceed_match_limit() {
        return Err(QueryFailure::MatchLimit {
            path: path.display().to_string(),
        });
    }
    if let Some(error) = predicate_error {
        return Err(error);
    }

    let named: Vec<Capture> = found
        .iter()
        .flat_map(|matched| matched.found.captures.iter().cloned())
        .collect();
    let mut output = Vec::new();
    for matched in found {
        let mut accepted = true;
        for predicate in &matched.predicates {
            let capture_name = predicate
                .capture
                .as_deref()
                .ok_or_else(|| QueryFailure::UnboundCapture("predicate focus".into()))?;
            let focused = matched
                .found
                .captures
                .iter()
                .find(|capture| capture.label == capture_name)
                .ok_or_else(|| QueryFailure::UnboundCapture(capture_name.into()))?;
            let target_name = predicate
                .target
                .as_deref()
                .ok_or_else(|| QueryFailure::UnboundCapture("predicate target".into()))?;
            let targets = named.iter().filter(|capture| capture.label == target_name);
            let relation_matches = targets.into_iter().any(|target| {
                relation_holds(focused, target, &predicate.operator, &predicate.stop_by)
            });
            accepted &= relation_matches;
        }
        if accepted {
            output.push(matched.found);
        }
    }
    Ok(output)
}

fn decode_predicate(
    predicate: &tree_sitter::QueryPredicate,
    capture_names: &[&str],
) -> Result<Predicate, QueryFailure> {
    let base = predicate.operator.as_ref();
    let (negated, operator) = base
        .strip_prefix("not-")
        .map(|operator| (true, operator))
        .unwrap_or((false, base));
    let operator = operator.strip_suffix('?').unwrap_or(operator);
    if !matches!(operator, "inside" | "has" | "precedes" | "follows") {
        return Err(QueryFailure::UnknownPredicate(base.into()));
    }
    let expected = if matches!(operator, "inside" | "has") {
        2..=3
    } else {
        2..=3
    };
    if !expected.contains(&predicate.args.len()) {
        return Err(QueryFailure::PredicateArity {
            operator: base.into(),
            got: predicate.args.len(),
        });
    }
    let capture = match predicate.args.first() {
        Some(QueryPredicateArg::Capture(index)) => capture_names
            .get(*index as usize)
            .map(|name| name.to_string()),
        _ => None,
    };
    let target = match predicate.args.get(1) {
        Some(QueryPredicateArg::String(name)) => Some(name.to_string()),
        _ => None,
    };
    let stop_by = match predicate.args.get(2) {
        Some(QueryPredicateArg::String(value)) => value.to_string(),
        None => "end".into(),
        _ => "end".into(),
    };
    Ok(Predicate {
        operator: if negated {
            format!("not-{operator}")
        } else {
            operator.into()
        },
        capture,
        target,
        stop_by,
    })
}

fn own_capture(node: Node<'_>, label: &str, source: &str) -> Capture {
    let mut ancestors = Vec::new();
    let mut ancestor_kinds = Vec::new();
    let mut parent = node.parent();
    while let Some(ancestor) = parent {
        ancestors.push((ancestor.start_byte(), ancestor.end_byte()));
        ancestor_kinds.push(ancestor.kind().to_string());
        parent = ancestor.parent();
    }
    Capture {
        label: label.into(),
        text: source[node.start_byte()..node.end_byte()].into(),
        kind: node.kind().to_string(),
        start: node.start_byte(),
        end: node.end_byte(),
        parent: node
            .parent()
            .map(|parent| (parent.start_byte(), parent.end_byte())),
        parent_kind: node.parent().map(|parent| parent.kind().to_string()),
        ancestors,
        ancestor_kinds,
    }
}

fn relation_holds(subject: &Capture, target: &Capture, operator: &str, stop_by: &str) -> bool {
    let relation = match operator {
        "inside" => subject.ancestors.contains(&(target.start, target.end)),
        "has" => target.ancestors.contains(&(subject.start, subject.end)),
        "precedes" => subject.parent == target.parent && subject.end <= target.start,
        "follows" => subject.parent == target.parent && subject.start >= target.end,
        "not-inside" => !subject.ancestors.contains(&(target.start, target.end)),
        "not-has" => !target.ancestors.contains(&(subject.start, subject.end)),
        "not-precedes" => !(subject.parent == target.parent && subject.end <= target.start),
        "not-follows" => !(subject.parent == target.parent && subject.start >= target.end),
        _ => false,
    };
    if stop_by == "neighbor" {
        match operator {
            "inside" => subject.parent == Some((target.start, target.end)),
            "not-inside" => subject.parent != Some((target.start, target.end)),
            "has" => target.parent == Some((subject.start, subject.end)),
            "not-has" => target.parent != Some((subject.start, subject.end)),
            _ => relation,
        }
    } else {
        relation
    }
}

#[allow(dead_code)]
fn _parse_tree(language: Language, source: &str) -> Result<Tree, QueryFailure> {
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|error| QueryFailure::Syntax(error.to_string()))?;
    parser
        .parse(source, None)
        .ok_or_else(|| QueryFailure::Syntax("parser cancelled".into()))
}
