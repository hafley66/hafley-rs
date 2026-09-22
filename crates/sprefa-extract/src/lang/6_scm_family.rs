//! Kotlin CallF projection from the CallF captures in `queries/kotlin/scip.scm`.

use std::collections::BTreeMap;

use tree_sitter::Tree;

use super::kotlin::KOTLIN_SCM;
use crate::family::{CallF, CallKind, CallSite};
use crate::rows::{FamilyBundle, Node};
use crate::shape::{Span, Strings};

#[derive(Default)]
struct SiteCapture {
    callee: Option<(String, Span)>,
    receiver: Option<Span>,
    operators: Vec<(String, Span)>,
}

#[derive(Default)]
struct DefCapture {
    name: Option<String>,
    kind: Option<String>,
    body_end: Option<u32>,
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
    let mut scopes: BTreeMap<Span, (String, Option<String>)> = BTreeMap::new();

    for row in &arena.rows {
        let mut def_span = None;
        let mut def_name = None;
        let mut def_body = None;
        let mut def_scope = None;
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
                "def.span" => def_span = Some(capture_span(span.bytes.start, span.bytes.end)),
                "def.name" => {
                    def_name = Some(
                        std::str::from_utf8(
                            &src[span.bytes.start as usize..span.bytes.end as usize],
                        )
                        .expect("Kotlin definition name is utf8")
                        .to_string(),
                    )
                }
                "def.body" => def_body = Some(span.bytes.end),
                "def.scope" => def_scope = Some(capture_span(span.bytes.start, span.bytes.end)),
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
        let properties = query.user.property_settings(row.pattern as usize);
        if let Some(span) = def_span {
            let def = defs.entry(span).or_insert_with(DefCapture::default);
            def.name = def.name.take().or(def_name.take());
            def.body_end = def.body_end.or(def_body);
            if let Some(kind) = property(properties, "call.def") {
                def.kind = Some(kind.to_string());
            }
        }
        if let Some(span) = def_scope {
            let kind = property(properties, "call.scope")
                .expect("a Kotlin definition scope has call.scope");
            let scope = scopes
                .entry(span)
                .or_insert_with(|| (kind.to_string(), None));
            scope.1 = scope.1.take().or(def_name);
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

    for (span, def) in defs {
        match def.kind.as_deref() {
            Some("function") => {
                let kind = nearest_scope(span, &scopes)
                    .map(|(_, (kind, _))| {
                        if kind == "method" {
                            CallKind::Method
                        } else {
                            CallKind::Free
                        }
                    })
                    .unwrap_or(CallKind::Free);
                let end = def.body_end.unwrap_or(span.end());
                let span = Span {
                    start: span.start,
                    len: end - span.start,
                };
                let name = def.name.unwrap_or_default();
                sink.nodes
                    .push(Node::new(span, kind).with_name(strings.intern(&name)));
            }
            Some("constructor") => {
                if let Some((_, (_, Some(name)))) = nearest_named_scope(span, &scopes) {
                    sink.nodes
                        .push(Node::new(span, CallKind::Method).with_name(strings.intern(name)));
                }
            }
            Some("lambda") => {
                sink.nodes.push(Node::new(span, CallKind::Lambda));
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

/// The query arena stores byte endpoints for each captured fact.
fn capture_span(start: u32, end: u32) -> Span {
    Span {
        start,
        len: end - start,
    }
}

fn property<'a>(properties: &'a [tree_sitter::QueryProperty], key: &str) -> Option<&'a str> {
    properties
        .iter()
        .find(|p| p.key.as_ref() == key)
        .and_then(|p| p.value.as_deref())
}

fn nearest_scope<'a>(
    span: Span,
    scopes: &'a BTreeMap<Span, (String, Option<String>)>,
) -> Option<(&'a Span, &'a (String, Option<String>))> {
    scopes
        .iter()
        .filter(|(owner, _)| {
            owner.len > span.len && owner.start <= span.start && span.end() <= owner.end()
        })
        .min_by_key(|(owner, _)| owner.len)
}

fn nearest_named_scope<'a>(
    span: Span,
    scopes: &'a BTreeMap<Span, (String, Option<String>)>,
) -> Option<(&'a Span, &'a (String, Option<String>))> {
    scopes
        .iter()
        .filter(|(owner, (_, name))| {
            name.is_some() && owner.start <= span.start && span.end() <= owner.end()
        })
        .min_by_key(|(owner, _)| owner.len)
}

fn push_site(span: Span, callee: &str, strings: &mut Strings, sink: &mut FamilyBundle<CallF>) {
    sink.aux.sites.push(CallSite {
        span,
        callee: strings.intern(callee),
        callee_path: None,
    });
}
