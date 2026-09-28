//! Scope facts are selected by language queries and lowered here without a
//! language-specific syntax walk. `__parent` links a nested product to its owner.

use std::sync::OnceLock;
use std::collections::BTreeSet;

use super::extract_lang::RyiLang;
use crate::read::shape::Span;
use crate::read::tsi::{Arg, FactOut};
use crate::read::types::TsiNames;

static RUST_QUERY: OnceLock<hafley_scm::QueryExt> = OnceLock::new();
static TS_QUERY: OnceLock<hafley_scm::QueryExt> = OnceLock::new();
static TSX_QUERY: OnceLock<hafley_scm::QueryExt> = OnceLock::new();

struct Scope {
    span: Span,
    kind: String,
    id: u32,
}

struct Binding {
    span: Span,
    name: String,
}

fn span(value: &hafley_scm::EmittedValue) -> Option<Span> {
    let bytes = value.bytes()?;
    Some(Span { start: bytes.start, len: bytes.end - bytes.start })
}

fn field<'a>(
    fact: &'a hafley_scm::EmittedFact,
    arena: &'a hafley_scm::MatchArena,
    query: &hafley_scm::QueryExt,
    name: &str,
) -> Option<&'a hafley_scm::EmittedValue> {
    fact.get(arena, query.field_id(name)?)
}

fn owner(scopes: &[Scope], position: u32) -> Option<usize> {
    scopes
        .iter()
        .enumerate()
        .filter(|(_, scope)| scope.span.start <= position && position < scope.span.end())
        .min_by_key(|(_, scope)| scope.span.len)
        .map(|(index, _)| index)
}

/// Append scope products to type facts. IDs and fact ordinals are shifted past
/// the existing syntax rows before the wire sees them.
pub fn append(lang: RyiLang, source: &[u8], existing: &mut Vec<FactOut>) {
    let (query_slot, text, origin) = match lang {
        RyiLang::Rust => (&RUST_QUERY, include_str!("8a_rust_scope.scm"), "rust"),
        RyiLang::TypeScript => (&TS_QUERY, include_str!("8b_ts_scope.scm"), "ts"),
        RyiLang::Tsx => (&TSX_QUERY, include_str!("8b_ts_scope.scm"), "ts"),
        _ => return,
    };
    let language = lang.tree_sitter_language();
    let Some(tree) = hafley_scm::cst::parse(&language, source) else { return };
    let query = query_slot.get_or_init(|| hafley_scm::build(&language, text).expect("scope query compiles"));
    let mut arena = hafley_scm::MatchArena::default();
    if hafley_scm::run(query, "", source, &tree, u32::MAX, &mut arena).is_err() { return }

    let mut scopes = Vec::new();
    let mut bindings = Vec::new();
    let mut writes = Vec::new();
    let mut references = Vec::new();
    for fact in &arena.emitted {
        let relation = &query.relations[fact.relation as usize];
        let Some(value) = field(fact, &arena, query, "span") else { continue };
        let Some(site) = span(value) else { continue };
        match relation.as_ref() {
            "scope" => {
                let kind = field(fact, &arena, query, "kind")
                    .and_then(|value| value.text(source, query))
                    .unwrap_or("");
                scopes.push(Scope { span: site, kind: kind.to_string(), id: 0 });
            }
            "binding" | "write" | "reference" => {
                let Some(name) = field(fact, &arena, query, "name")
                    .and_then(|value| value.text(source, query)) else { continue };
                let row = Binding { span: site, name: name.to_string() };
                match relation.as_ref() {
                    "binding" => bindings.push(row),
                    "write" => writes.push(row),
                    _ => references.push(row),
                }
            }
            _ => {}
        }
    }
    scopes.sort_by_key(|scope| (scope.span.start, std::cmp::Reverse(scope.span.end())));
    bindings.sort_by_key(|row| row.span.start);
    writes.sort_by_key(|row| row.span.start);

    let mut names = TsiNames::new(origin);
    for scope in &mut scopes {
        scope.id = names.anonymous(scope.span);
        names.fact("tsi.product", vec![Arg::Id(scope.id)]);
    }
    for index in 0..scopes.len() {
        let Some(parent) = scopes.iter().enumerate()
            .filter(|(other, scope)| *other != index
                && scope.span.start <= scopes[index].span.start
                && scopes[index].span.end() <= scope.span.end()
                && scope.span.len > scopes[index].span.len)
            .min_by_key(|(_, scope)| scope.span.len)
            .map(|(_, scope)| scope.id) else { continue };
        names.edge(scopes[index].id, "__parent", parent, scopes[index].span.start as i64);
    }
    for scope in &scopes {
        if scope.kind != "closure" && scope.kind != "arrow" { continue }
        let callable = names.anonymous(scope.span);
        names.fact("tsi.callable", vec![Arg::Id(callable)]);
        names.fact("tsi.input", vec![Arg::Id(callable), Arg::Int(-1), Arg::Id(scope.id)]);
    }
    let mut targets = Vec::new();
    for binding in &bindings {
        let Some(index) = owner(&scopes, binding.span.start) else {
            targets.push(None);
            continue;
        };
        let target = names.anonymous(binding.span);
        names.edge(scopes[index].id, &binding.name, target, binding.span.start as i64);
        targets.push(Some((index, target)));
    }
    for write in &writes {
        let Some(index) = owner(&scopes, write.span.start) else { continue };
        let Some(binding) = bindings.iter().rev().find(|binding| {
            binding.name == write.name && binding.span.start < write.span.start
                && owner(&scopes, binding.span.start) == Some(index)
        }) else { continue };
        let target = names.anonymous(write.span);
        names.edge(scopes[index].id, &binding.name, target, write.span.start as i64);
    }
    let mut captured = BTreeSet::new();
    for reference in &references {
        if bindings.iter().any(|binding| binding.span == reference.span) { continue }
        let Some((closure_index, closure)) = scopes.iter().enumerate()
            .filter(|(_, scope)| (scope.kind == "closure" || scope.kind == "arrow")
                && scope.span.start <= reference.span.start
                && reference.span.end() <= scope.span.end())
            .min_by_key(|(_, scope)| scope.span.len) else { continue };
        let selected = bindings.iter().enumerate()
            .filter(|(_, binding)| binding.name == reference.name
                && binding.span.start < closure.span.start)
            .filter(|(index, _)| targets[*index].is_some_and(|(scope_index, _)| {
                scopes[scope_index].span.start <= closure.span.start
                    && closure.span.end() <= scopes[scope_index].span.end()
            }))
            .max_by_key(|(_, binding)| binding.span.start);
        let Some((binding_index, binding)) = selected else { continue };
        if !captured.insert((closure_index, binding.name.clone())) { continue }
        let Some((_, target)) = targets[binding_index] else { continue };
        names.edge(closure.id, &binding.name, target, binding.span.start as i64);
    }

    let id_offset = existing.iter().flat_map(|row| row.args.iter())
        .filter_map(|arg| match arg { Arg::Id(id) => Some(*id), _ => None })
        .max().map_or(0, |id| id + 1);
    let fact_offset = existing.iter().map(|row| row.fact).max().map_or(0, |fact| fact + 1);
    for mut row in names.into_facts() {
        row.fact += fact_offset;
        for arg in &mut row.args {
            if let Arg::Id(id) = arg { *id += id_offset; }
        }
        existing.push(row);
    }
}
