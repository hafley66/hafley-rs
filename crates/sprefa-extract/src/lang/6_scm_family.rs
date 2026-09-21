//! Kotlin CallF projection from the captures in `queries/kotlin/call.scm`.

use std::collections::{BTreeMap, BTreeSet};

use tree_sitter::{Node as TsNode, Query, QueryCursor, StreamingIterator};

use super::ast_rule::{query_ast_rule, AstRule, AstRuleRequest};
use super::kotlin::{kt_child_kind, kt_first_child, kt_text, node_span};
use super::scm_lower::lower_scm;
use crate::family::{CallF, CallKind, CallSite};
use crate::rows::{FamilyBundle, Node};
use crate::shape::{Span, Strings};

const KOTLIN_CALL_SCM: &str = include_str!("../../queries/kotlin/call.scm");

#[derive(Default)]
struct SiteCapture {
    callee: Option<(String, Span)>,
    receiver: Option<Span>,
}

/// Lower the bundled query through L1, execute its span captures, then map the
/// native grouped captures onto CallF rows from the existing Kotlin parse.
pub(crate) fn project_kotlin_call(
    path: &str,
    root: TsNode<'_>,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let selected = lowered_spans(path, src);
    let language = root.language();
    let query =
        Query::new(&language, KOTLIN_CALL_SCM).expect("the bundled Kotlin CallF query compiles");
    let names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root, src);
    let mut defs = BTreeMap::new();
    let mut sites: BTreeMap<Span, (TsNode<'_>, SiteCapture)> = BTreeMap::new();
    let mut class_names = Vec::new();

    while let Some(found) = matches.next() {
        let mut def_span = None;
        let mut def_name = None;
        let mut site_span = None;
        let mut site_callee = None;
        let mut site_receiver = None;
        for capture in found.captures {
            let node = capture.node;
            match names[capture.index as usize] {
                "def.span" => def_span = Some(node),
                "def.name" => def_name = Some(node),
                "site.span" => site_span = Some(node),
                "site.callee" => site_callee = Some(node),
                "site.receiver" => site_receiver = Some(node),
                _ => {}
            }
        }
        if let Some(node) = def_span {
            let span = node_span(node);
            if selected.contains(&span) {
                defs.entry(span).or_insert_with(|| {
                    let name = def_name.map(|name| kt_text(name, src).to_string());
                    (node, name)
                });
            }
        } else if let Some(name) = def_name {
            if let Some(owner) = ancestor(name, &["class_declaration"]) {
                class_names.push((node_span(owner), kt_text(name, src).to_string()));
            }
        }
        if let Some(node) = site_span {
            let span = node_span(node);
            if selected.contains(&span) {
                let entry = sites
                    .entry(span)
                    .or_insert_with(|| (node, SiteCapture::default()));
                if let Some(callee) = site_callee {
                    entry.1.callee = Some((kt_text(callee, src).to_string(), node_span(callee)));
                }
                if let Some(receiver) = site_receiver {
                    entry.1.receiver = Some(node_span(receiver));
                }
            }
        }
    }
    drop(matches);
    assert!(
        !cursor.did_exceed_match_limit(),
        "Kotlin CallF query exceeded the tree-sitter match limit"
    );

    for (_, (node, name)) in defs {
        match node.kind() {
            "function_declaration" => {
                let kind = function_kind(node);
                let span = function_span(node);
                let name = name.unwrap_or_default();
                sink.nodes
                    .push(Node::new(span, kind).with_name(strings.intern(&name)));
            }
            "primary_constructor" | "secondary_constructor" => {
                if let Some(name) = containing_name(node_span(node), &class_names) {
                    sink.nodes.push(
                        Node::new(node_span(node), CallKind::Method)
                            .with_name(strings.intern(name)),
                    );
                }
            }
            "lambda_literal" if ancestor(node, &["function_declaration"]).is_some() => {
                sink.nodes
                    .push(Node::new(node_span(node), CallKind::Lambda));
            }
            _ => {}
        }
    }

    for (_, (node, captures)) in sites {
        map_site(node, captures, src, strings, sink);
    }
}

/// L1 supplies the definition and site candidate spans. Native query execution
/// retains capture grouping, which the AstRule representation does not store.
fn lowered_spans(path: &str, src: &[u8]) -> BTreeSet<Span> {
    let program = lower_scm(KOTLIN_CALL_SCM).expect("the bundled Kotlin CallF query lowers");
    let rule = AstRule::Any(
        ["def.span", "site.span"]
            .into_iter()
            .map(|name| AstRule::Matches(name.to_string()))
            .collect(),
    );
    let request = AstRuleRequest {
        id: "kotlin-call".into(),
        rule,
        utils: program.utils,
        constraints: program.constraints,
        fix: None,
    };
    query_ast_rule(path, src, &request)
        .expect("the lowered Kotlin CallF query executes")
        .into_iter()
        .map(|row| row.span)
        .collect()
}

fn function_kind(node: TsNode<'_>) -> CallKind {
    let mut parent = node.parent();
    while let Some(scope) = parent {
        match scope.kind() {
            "function_declaration" => return CallKind::Free,
            "class_declaration" | "object_declaration" => return CallKind::Method,
            _ => parent = scope.parent(),
        }
    }
    CallKind::Free
}

fn function_span(node: TsNode<'_>) -> Span {
    let start = node.start_byte();
    let end = kt_first_child(node, "function_body")
        .unwrap_or(node)
        .end_byte();
    Span {
        start: start as u32,
        len: (end - start) as u32,
    }
}

fn ancestor<'tree>(node: TsNode<'tree>, kinds: &[&str]) -> Option<TsNode<'tree>> {
    let mut parent = node.parent();
    while let Some(candidate) = parent {
        if kinds.contains(&candidate.kind()) {
            return Some(candidate);
        }
        parent = candidate.parent();
    }
    None
}

fn containing_name(span: Span, owners: &[(Span, String)]) -> Option<&str> {
    owners
        .iter()
        .filter(|(owner, _)| owner.start <= span.start && span.end() <= owner.end())
        .min_by_key(|(owner, _)| owner.len)
        .map(|(_, name)| name.as_str())
}

fn push_site(span: Span, callee: &str, strings: &mut Strings, sink: &mut FamilyBundle<CallF>) {
    sink.aux.sites.push(CallSite {
        span,
        callee: strings.intern(callee),
        callee_path: None,
    });
}

fn map_site(
    node: TsNode<'_>,
    captures: SiteCapture,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    match node.kind() {
        "call_expression" => {
            if let Some((callee, callee_span)) = captures.callee {
                let span = captures.receiver.unwrap_or(callee_span);
                push_site(span, &callee, strings, sink);
            } else {
                let mut cursor = node.walk();
                let lead = node
                    .children(&mut cursor)
                    .find(|child| child.kind() != "call_suffix");
                if lead.is_some_and(|lead| lead.kind() == "call_expression") {
                    if let Some(suffix) = kt_first_child(node, "call_suffix") {
                        push_site(node_span(suffix), "invoke", strings, sink);
                    }
                }
            }
        }
        "infix_expression" => {
            let mut cursor = node.walk();
            if let Some(name) = node.children(&mut cursor).nth(1) {
                if name.kind() == "simple_identifier" {
                    push_site(node_span(name), kt_text(name, src), strings, sink);
                }
            };
        }
        "additive_expression"
        | "multiplicative_expression"
        | "range_expression"
        | "comparison_expression"
        | "equality_expression" => {
            if let Some(operator) = anonymous_token(node, src) {
                if let Some(callee) = binary_name(operator) {
                    push_site(
                        anonymous_span(node).unwrap_or_else(|| node_span(node)),
                        callee,
                        strings,
                        sink,
                    );
                }
            }
        }
        "check_expression" => {
            let mut cursor = node.walk();
            if let Some(operator) = node
                .children(&mut cursor)
                .find(|child| !child.is_named() && matches!(kt_text(*child, src), "in" | "!in"))
            {
                push_site(node_span(operator), "contains", strings, sink);
            };
        }
        "prefix_expression" => {
            if let Some(callee) = anonymous_token(node, src).and_then(prefix_name) {
                push_site(
                    anonymous_span(node).unwrap_or_else(|| node_span(node)),
                    callee,
                    strings,
                    sink,
                );
            }
        }
        "postfix_expression" => {
            if let Some(callee) = anonymous_token(node, src).and_then(postfix_name) {
                push_site(
                    anonymous_span(node).unwrap_or_else(|| node_span(node)),
                    callee,
                    strings,
                    sink,
                );
            }
        }
        "indexing_expression" => {
            if let Some(suffix) = kt_first_child(node, "indexing_suffix") {
                push_site(node_span(suffix), "get", strings, sink);
            }
        }
        "assignment" => {
            if let Some(callee) = anonymous_token(node, src).and_then(assignment_name) {
                push_site(
                    anonymous_span(node).unwrap_or_else(|| node_span(node)),
                    callee,
                    strings,
                    sink,
                );
            }
            if let Some(lhs) = kt_first_child(node, "directly_assignable_expression") {
                if let Some(suffix) = kt_child_kind(lhs, "indexing_suffix") {
                    push_site(node_span(suffix), "set", strings, sink);
                }
            }
        }
        _ => {}
    }
}

fn anonymous_span(node: TsNode<'_>) -> Option<Span> {
    let mut cursor = node.walk();
    let span = node.children(&mut cursor)
        .find(|child| !child.is_named())
        .map(node_span);
    span
}

fn anonymous_token<'a>(node: TsNode<'_>, src: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    let token = node.children(&mut cursor)
        .find(|child| !child.is_named())
        .map(|child| kt_text(child, src));
    token
}

fn binary_name(operator: &str) -> Option<&'static str> {
    Some(match operator {
        "+" => "plus",
        "-" => "minus",
        "*" => "times",
        "/" => "div",
        "%" => "rem",
        ".." => "rangeTo",
        "..<" => "rangeUntil",
        "==" | "!=" => "equals",
        "<" | ">" | "<=" | ">=" => "compareTo",
        _ => return None,
    })
}

fn prefix_name(operator: &str) -> Option<&'static str> {
    Some(match operator {
        "-" => "unaryMinus",
        "+" => "unaryPlus",
        "!" => "not",
        "++" => "inc",
        "--" => "dec",
        _ => return None,
    })
}

fn postfix_name(operator: &str) -> Option<&'static str> {
    match operator {
        "++" => Some("inc"),
        "--" => Some("dec"),
        _ => None,
    }
}

fn assignment_name(operator: &str) -> Option<&'static str> {
    Some(match operator {
        "+=" => "plusAssign",
        "-=" => "minusAssign",
        "*=" => "timesAssign",
        "/=" => "divAssign",
        "%=" => "remAssign",
        _ => return None,
    })
}
