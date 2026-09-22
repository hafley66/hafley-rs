//! Kotlin CallF projection from the CallF captures in `queries/kotlin/scip.scm`.

use std::collections::BTreeMap;

use tree_sitter::{Node as TsNode, Tree};

use super::kotlin::{kt_first_child, kt_text, node_span, KOTLIN_SCM};
use crate::family::{CallF, CallKind, CallSite};
use crate::rows::{FamilyBundle, Node};
use crate::shape::{Span, Strings};

#[derive(Default)]
struct SiteCapture {
    callee: Option<(String, Span)>,
    receiver: Option<Span>,
    operators: Vec<(String, Span)>,
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
    let mut sites: BTreeMap<Span, SiteCapture> = BTreeMap::new();
    let mut class_names = Vec::new();

    for row in &arena.rows {
        let mut def_span = None;
        let mut def_name = None;
        let mut site_span = None;
        let mut site_callee = None;
        let mut site_receiver = None;
        let mut site_operators = Vec::new();
        for span in &arena.spans[row.spans.start as usize..row.spans.end as usize] {
            let label = names[span.name as usize].as_ref();
            if label == "site.operator" {
                let callee = query
                    .user
                    .property_settings(row.pattern as usize)
                    .iter()
                    .find(|property| property.key.as_ref() == "call.callee")
                    .and_then(|property| property.value.as_deref())
                    .expect("a Kotlin operator capture names its call.callee");
                site_operators.push((
                    callee.to_string(),
                    Span {
                        start: span.bytes.start,
                        len: span.bytes.end - span.bytes.start,
                    },
                ));
                continue;
            }
            match label {
                "def.span" => {
                    def_span = Some(captured_node(root, (span.bytes.start, span.bytes.end), label))
                }
                "def.name" => {
                    def_name = Some(captured_node(root, (span.bytes.start, span.bytes.end), label))
                }
                "site.span" => {
                    site_span = Some(Span {
                        start: span.bytes.start,
                        len: span.bytes.end - span.bytes.start,
                    })
                }
                "site.callee" => {
                    let text = std::str::from_utf8(
                        &src[span.bytes.start as usize..span.bytes.end as usize],
                    )
                    .expect("Kotlin identifier capture is utf8");
                    site_callee = Some((
                        text.to_string(),
                        Span {
                            start: span.bytes.start,
                            len: span.bytes.end - span.bytes.start,
                        },
                    ));
                }
                "site.receiver" => {
                    site_receiver = Some(Span {
                        start: span.bytes.start,
                        len: span.bytes.end - span.bytes.start,
                    })
                }
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
        if let Some(span) = site_span {
            let entry = sites.entry(span).or_default();
            if let Some(callee) = site_callee {
                entry.callee = Some(callee);
            }
            if let Some(receiver) = site_receiver {
                entry.receiver = Some(receiver);
            }
            entry.operators.extend(site_operators);
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

    for (_, captures) in sites {
        if let Some((callee, callee_span)) = captures.callee {
            push_site(
                captures.receiver.unwrap_or(callee_span),
                &callee,
                strings,
                sink,
            );
        } else {
            for (callee, span) in captures.operators {
                push_site(span, &callee, strings, sink);
            }
        }
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
