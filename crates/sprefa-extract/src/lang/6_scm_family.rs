//! Kotlin CallF definitions from captures and sites from SCM emissions.

use std::collections::BTreeMap;

use crate::family::{CallF, CallKind, CallSite};
use crate::rows::{FamilyBundle, Node};
use crate::shape::{Span, Strings};

#[derive(Default)]
struct SiteCapture {
    direct: Option<(String, Span)>,
    operators: Vec<(String, Span)>,
}

#[derive(Default)]
struct DefCapture {
    name: Option<String>,
    kind: Option<String>,
    body_end: Option<u32>,
}

/// Map definition captures and emitted call sites onto CallF rows.
pub(crate) fn project_kotlin_call(
    src: &[u8],
    query: &hafley_scm::QueryExt,
    arena: &hafley_scm::MatchArena,
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let names = &query.names;
    let mut defs = BTreeMap::new();
    let mut sites: BTreeMap<Span, SiteCapture> = BTreeMap::new();
    let mut scopes: BTreeMap<Span, (String, Option<String>)> = BTreeMap::new();

    for row in &arena.rows {
        let mut def_span = None;
        let mut def_name = None;
        let mut def_body = None;
        let mut def_scope = None;
        for span in &arena.spans[row.spans.start as usize..row.spans.end as usize] {
            let label = names[span.name as usize].as_ref();
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
    }

    let site_relation = query.relation_id("call.site").expect("Kotlin query emits call.site");
    let group_key = query.field_id("group").expect("call.site has group");
    let span_key = query.field_id("span").expect("call.site has span");
    let callee_key = query.field_id("callee").expect("call.site has callee");
    for emitted in arena.emitted.iter().filter(|fact| fact.relation == site_relation) {
        let group_bytes = emitted.get(arena, group_key).and_then(hafley_scm::EmittedValue::bytes)
            .expect("call.site group is a source span");
        let span_bytes = emitted.get(arena, span_key).and_then(hafley_scm::EmittedValue::bytes)
            .expect("call.site span is a source span");
        let group = capture_span(group_bytes.start, group_bytes.end);
        let span = capture_span(span_bytes.start, span_bytes.end);
        let entry = sites.entry(group).or_default();
        let callee = emitted.get(arena, callee_key).expect("call.site has callee");
        let text = callee.text(src, query).expect("Kotlin call name is utf8");
        match callee {
            hafley_scm::EmittedValue::Bytes(_) => entry.direct = Some((text.to_string(), span)),
            hafley_scm::EmittedValue::Literal(_) => entry.operators.push((text.to_string(), span)),
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
        if let Some((callee, span)) = captures.direct {
            push_site(span, &callee, strings, sink);
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
