//! Kotlin CallF definitions from captures and sites from SCM emissions.

use std::collections::BTreeMap;

use crate::read::family::{CallF, CallKind, CallSite};
use crate::read::rows::{FamilyBundle, Node};
use crate::read::shape::{Span, Strings};

mod generated {
    include!("kotlin_scm_generated.rs");
}

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

/// Map emitted definitions and call sites onto CallF rows.
pub fn project_kotlin_call(
    src: &[u8],
    query: &hafley_scm::QueryExt,
    arena: &hafley_scm::MatchArena,
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let mut defs = BTreeMap::new();
    let mut sites: BTreeMap<Span, SiteCapture> = BTreeMap::new();
    let mut scopes: BTreeMap<Span, (String, Option<String>)> = BTreeMap::new();

    for emitted in generated::CallDef::rows(arena) {
        let span = emitted.span().bytes().expect("call.def span is captured");
        let span = capture_span(span.start, span.end);
        let def = defs.entry(span).or_insert_with(DefCapture::default);
        def.name = def.name.take().or_else(|| emitted.name()
            .and_then(|name| name.text(src, query))
            .map(str::to_string));
        def.body_end = def.body_end.or_else(|| emitted.body()
            .and_then(hafley_scm::EmittedValue::bytes)
            .map(|body| body.end));
        def.kind = Some(emitted.kind().text(src, query)
            .expect("call.def kind is utf8").to_string());
    }

    for emitted in generated::CallScope::rows(arena) {
        let span = emitted.span().bytes().expect("call.scope span is captured");
        let span = capture_span(span.start, span.end);
        let kind = emitted.kind().text(src, query).expect("call.scope kind is utf8");
        let scope = scopes.entry(span).or_insert_with(|| (kind.to_string(), None));
        scope.1 = scope.1.take().or_else(|| emitted.name()
            .and_then(|name| name.text(src, query))
            .map(str::to_string));
    }

    for emitted in generated::CallSite::rows(arena) {
        let group_bytes = emitted.group().bytes()
            .expect("call.site group is a source span");
        let span_bytes = emitted.span().bytes()
            .expect("call.site span is a source span");
        let group = capture_span(group_bytes.start, group_bytes.end);
        let span = capture_span(span_bytes.start, span_bytes.end);
        let entry = sites.entry(group).or_default();
        let callee = emitted.callee();
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
