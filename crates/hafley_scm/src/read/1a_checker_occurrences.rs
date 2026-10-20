//! `ryi slow`'s `symbol`/`occurrence` rows, lifted from checker `resolved_edge`
//! rows: one symbol per callee definition span, one reference per call site.

use std::collections::BTreeSet;

use crate::read::types::ResolutionOrigin;
use crate::read::shape::FamilyTag;
use crate::read::wire::{FlatFact, SpanOut};

/// The checker's symbol identity: the definition's path, name and name span.
pub fn checker_symbol(path: &str, name: Option<&str>, start: u32, end: u32) -> String {
    format!("checker {path}#{}@{start}:{end}", name.unwrap_or(""))
}

/// A `ref` occurrence spans the whole call site; rows of other origins are skipped.
pub fn checker_occurrences(facts: &[FlatFact]) -> Vec<FlatFact> {
    let checker = ResolutionOrigin::Checker.as_str();
    let mut defs = BTreeSet::new();
    let mut refs = BTreeSet::new();
    let mut rows = Vec::new();
    for fact in facts {
        let FlatFact::ResolvedEdge {
            caller_path,
            caller_site_start,
            caller_site_end,
            callee_path,
            callee_name,
            callee_start,
            callee_end,
            resolution_origin,
            ..
        } = fact
        else {
            continue;
        };
        if resolution_origin != checker {
            continue;
        }
        let symbol = checker_symbol(callee_path, callee_name.as_deref(), *callee_start, *callee_end);
        if defs.insert(symbol.clone()) {
            rows.push(FlatFact::SymbolRow {
                symbol: symbol.clone(),
                path: callee_path.clone(),
                kind: "callee".to_string(),
            });
            rows.push(FlatFact::OccurrenceRow {
                symbol: symbol.clone(),
                path: callee_path.clone(),
                start: *callee_start,
                end: *callee_end,
                role: "def".to_string(),
                exported: false,
                decl_start: *callee_start,
                decl_end: *callee_end,
            });
        }
        if refs.insert((symbol.clone(), caller_path.clone(), *caller_site_start, *caller_site_end)) {
            rows.push(FlatFact::OccurrenceRow {
                symbol,
                path: caller_path.clone(),
                start: *caller_site_start,
                end: *caller_site_end,
                role: "ref".to_string(),
                exported: false,
                decl_start: *caller_site_start,
                decl_end: *caller_site_end,
            });
        }
    }
    rows
}

/// One `unresolved` row of reason `no_checker_edge` per call site (path, span,
/// written callee) that no `checker`-origin `resolved_edge` in `facts` starts from.
pub fn checker_unresolved(sites: &[(String, u32, u32, String)], facts: &[FlatFact]) -> Vec<FlatFact> {
    let checker = ResolutionOrigin::Checker.as_str();
    let answered: BTreeSet<(&str, u32, u32)> = facts
        .iter()
        .filter_map(|fact| match fact {
            FlatFact::ResolvedEdge {
                caller_path,
                caller_site_start,
                caller_site_end,
                resolution_origin,
                ..
            } if resolution_origin == checker => {
                Some((caller_path.as_str(), *caller_site_start, *caller_site_end))
            }
            _ => None,
        })
        .collect();
    let mut seen = BTreeSet::new();
    sites
        .iter()
        .filter(|(path, start, end, _)| {
            !answered.contains(&(path.as_str(), *start, *end)) && seen.insert((path, *start, *end))
        })
        .map(|(path, start, end, callee)| FlatFact::Unresolved {
            family: FamilyTag::Call,
            path: Some(path.clone()),
            span: SpanOut::new(*start, *end),
            reason: "no_checker_edge".to_string(),
            detail: callee.clone(),
        })
        .collect()
}
