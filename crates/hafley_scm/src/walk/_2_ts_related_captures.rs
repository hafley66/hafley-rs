use tree_sitter::{Node, Query, QueryCapture, QueryCursor, StreamingIterator};

use crate::types::{PredicateKind, QueryExt, Stop, Walk};

const ROOT_CAPTURE: &str = "__scm_relation_root";

pub fn pattern_captures<'tree>(
    query: &Query,
    node: Node<'tree>,
    src: &[u8],
) -> Option<Vec<QueryCapture<'tree>>> {
    let root = query.capture_index_for_name(ROOT_CAPTURE)?;
    let mut cursor = QueryCursor::new();
    cursor.set_max_start_depth(Some(0));
    let mut matches = cursor.matches(query, node, src);
    while let Some(found) = matches.next() {
        if found
            .captures()
            .iter()
            .any(|capture| capture.index == root && capture.node.id() == node.id())
        {
            return Some(
                found
                    .captures()
                    .iter()
                    .filter(|capture| capture.index != root)
                    .copied()
                    .collect(),
            );
        }
    }
    None
}

fn in_field(node: Node, field: u16) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let mut cursor = parent.walk();
    if !cursor.goto_first_child() {
        return false;
    }
    loop {
        if cursor.node().id() == node.id() {
            return cursor.field_id().is_some_and(|id| id.get() == field);
        }
        if !cursor.goto_next_sibling() {
            return false;
        }
    }
}

fn global_captures<'tree>(
    query: &Query,
    captures: Vec<QueryCapture<'tree>>,
    q: &QueryExt,
) -> Vec<QueryCapture<'tree>> {
    captures
        .into_iter()
        .map(|capture| {
            let name = query.capture_names()[capture.index as usize];
            let index = q
                .names
                .iter()
                .position(|seen| seen.as_ref() == name)
                .expect("related capture interned");
            QueryCapture {
                index: index as u32,
                node: capture.node,
            }
        })
        .collect()
}

pub fn related_captures<'tree>(
    kind: &PredicateKind,
    node: Node<'tree>,
    q: &QueryExt,
    src: &[u8],
    kind_ids: &[Vec<u32>],
) -> Option<Vec<QueryCapture<'tree>>> {
    let PredicateKind::Node {
        kinds,
        walk,
        stop,
        query,
        field,
    } = kind
    else {
        return None;
    };
    if query.is_none() && field.is_none() && !matches!(stop, Stop::Rule(_)) {
        return q.predicate_kinds[kinds.start as usize..kinds.end as usize]
            .iter()
            .any(|index| super::holds(kind, node, &kind_ids[*index as usize]))
            .then(Vec::new);
    }
    let test = |related| {
        if field.is_some_and(|field| !in_field(related, field)) {
            return None;
        }
        if let Some(query) = query {
            pattern_captures(query, related, src)
                .map(|captures| global_captures(query, captures, q))
        } else if q.predicate_kinds[kinds.start as usize..kinds.end as usize]
            .iter()
            .any(|kind| super::dispatch_by_direction::is_hit(related, &kind_ids[*kind as usize]))
        {
            Some(Vec::new())
        } else {
            None
        }
    };
    let stops = |related| match stop {
        Stop::Rule(query) => pattern_captures(query, related, src).is_some(),
        _ => false,
    };
    match walk {
        Walk::Ancestor | Walk::Parent | Walk::Precedes | Walk::Follows => {
            let next = |node: Node<'tree>| match walk {
                Walk::Precedes => node.next_named_sibling(),
                Walk::Follows => node.prev_named_sibling(),
                _ => node.parent(),
            };
            let mut related = next(node);
            while let Some(current) = related {
                if let Some(captures) = test(current) {
                    return Some(captures);
                }
                if matches!(walk, Walk::Parent) || matches!(stop, Stop::Neighbor) || stops(current)
                {
                    return None;
                }
                related = next(current);
            }
        }
        Walk::Descendant => {
            let mut cursor = node.walk();
            if !cursor.goto_first_child() {
                return None;
            }
            let mut depth = 1;
            loop {
                let current = cursor.node();
                if let Some(captures) = test(current) {
                    return Some(captures);
                }
                if stops(current) {
                    return None;
                }
                if !matches!(stop, Stop::Neighbor) && cursor.goto_first_child() {
                    depth += 1;
                    continue;
                }
                while !cursor.goto_next_sibling() {
                    if !cursor.goto_parent() {
                        return None;
                    }
                    depth -= 1;
                    if depth == 0 {
                        return None;
                    }
                }
            }
        }
    }
    None
}

pub fn nth_captures<'tree>(
    node: Node<'tree>,
    index: u32,
    kind: Option<u16>,
    query: Option<&Query>,
    q: &QueryExt,
    src: &[u8],
) -> Option<Vec<QueryCapture<'tree>>> {
    if query.is_none() {
        return super::_1_ts_nth_child::holds(node, index, kind).then(Vec::new);
    }
    if !node.is_named() || node.parent().is_none() {
        return None;
    }

    let eligible = |node: Node<'tree>| {
        kind.is_none_or(|kind| node.kind_id() == kind)
            && query.is_none_or(|query| pattern_captures(query, node, src).is_some())
    };
    if !eligible(node) {
        return None;
    }
    let mut position = 1;
    let mut sibling = node.prev_named_sibling();
    while let Some(previous) = sibling {
        if eligible(previous) {
            position += 1
        }
        sibling = previous.prev_named_sibling();
    }
    if position != index {
        return None;
    }
    Some(query.map_or_else(Vec::new, |query| {
        global_captures(query, pattern_captures(query, node, src).unwrap(), q)
    }))
}
