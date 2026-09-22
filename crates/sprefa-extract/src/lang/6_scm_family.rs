//! Kotlin CallF projection from the CallF captures in `queries/kotlin/scip.scm`.

use std::collections::BTreeMap;

use tree_sitter::{Node as TsNode, Tree};

use super::kotlin::{kt_child_kind, kt_first_child, kt_text, node_span, KOTLIN_SCM};
use crate::family::{CallF, CallKind, CallSite};
use crate::rows::{FamilyBundle, Node};
use crate::shape::{Span, Strings};

#[derive(Default)]
struct SiteCapture {
    callee: Option<(String, Span)>,
    receiver: Option<Span>,
}

/// Build the bundled query once, run it through the shared engine, then map
/// the arena's grouped captures onto CallF rows from the existing Kotlin parse.
pub(crate) fn project_kotlin_call(
    path: &str,
    tree: &Tree,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let root = tree.root_node();
    let language = root.language();
    let query =
        hafley_scm::build(&language, KOTLIN_SCM).expect("the bundled Kotlin CallF query compiles");
    let mut arena = hafley_scm::MatchArena::default();
    // The fresh-cursor default the direct run always had; the engine's limit
    // check cannot fire at u32::MAX.
    hafley_scm::run(&query, path, src, tree, u32::MAX, &mut arena)
        .expect("the Kotlin CallF query never exceeds the engine match limit");
    let names = &query.names;
    let mut defs = BTreeMap::new();
    let mut sites: BTreeMap<Span, (TsNode<'_>, SiteCapture)> = BTreeMap::new();
    let mut class_names = Vec::new();

    for row in &arena.rows {
        let mut def_span = None;
        let mut def_name = None;
        let mut site_span = None;
        let mut site_callee = None;
        let mut site_receiver = None;
        for span in &arena.spans[row.spans.start as usize..row.spans.end as usize] {
            let label = names[span.name as usize].as_ref();
            let node = captured_node(root, (span.bytes.start, span.bytes.end), label);
            match label {
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
            defs.entry(span).or_insert_with(|| {
                let name = def_name.map(|name| kt_text(name, src).to_string());
                (node, name)
            });
        } else if let Some(name) = def_name {
            if let Some(owner) = ancestor(name, &["class_declaration"]) {
                class_names.push((node_span(owner), kt_text(name, src).to_string()));
            }
        }
        if let Some(node) = site_span {
            let span = node_span(node);
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

/// The exact node the engine captured, recovered from the arena's byte range
/// against the file's own parse. The bundled CallF labels always name whole
/// nodes, so the range must land on one exactly; anything else is a defect
/// this module names loudly instead of silently mis-projecting.
fn captured_node<'tree>(root: TsNode<'tree>, range: (u32, u32), label: &str) -> TsNode<'tree> {
    let (start, end) = (range.0 as usize, range.1 as usize);
    let node = root
        .descendant_for_byte_range(start, end)
        .unwrap_or_else(|| panic!("Kotlin CallF: no node for {label} at {range:?}"));
    assert!(
        node.start_byte() == start && node.end_byte() == end,
        "Kotlin CallF: {label} range {range:?} is not an exact node ({}..{})",
        node.start_byte(),
        node.end_byte()
    );
    node
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
