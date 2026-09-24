//! Macro-expanded call definitions and sites, mapped back to source bytes.

use std::ops::Range;

use crate::QueryExt;

use super::call_definition_rows::{call_definition_rows, CallDefinitionKind};
use super::call_metadata_rows::build_line_starts;
use super::call_site_rows::call_site_rows;
use super::syn_macro_expansion_defs::expand_file;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpandedCallDefRow {
    pub range: Range<u32>,
    pub kind: ExpandedCallKind,
    pub name: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpandedCallKind { Free, Method, Lambda, ConstInit }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpandedCallSiteRow {
    pub range: Range<u32>,
    pub callee: String,
    pub callee_path: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExpandedCallRows {
    pub defs: Vec<ExpandedCallDefRow>,
    pub sites: Vec<ExpandedCallSiteRow>,
    pub macros: Vec<(Range<u32>, String)>,
}

pub fn expanded_call_rows(src: &str, query: &QueryExt, language: &tree_sitter::Language) -> ExpandedCallRows {
    let Some(expanded) = expand_file(src) else {
        return ExpandedCallRows::default();
    };
    let Ok(parsed) = syn::parse_file(&expanded.text) else {
        return ExpandedCallRows::default();
    };
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(language).expect("rust grammar");
    let Some(tree) = parser.parse(expanded.text.as_bytes(), None) else {
        return ExpandedCallRows::default();
    };

    let mut defs = call_definition_rows(query, "rust-call", expanded.text.as_bytes(), &tree);
    let def_ranges = defs.iter().map(|row| row.range.clone()).collect::<Vec<_>>();
    let sites = call_site_rows(&parsed, &build_line_starts(&expanded.text), &def_ranges);
    let mut rows = ExpandedCallRows::default();
    for row in defs {
        if expanded.is_macro_span(row.range.clone()) {
            if let Some(range) = expanded.map_span(row.range) {
                rows.defs.push(ExpandedCallDefRow {
                    range,
                    kind: match row.kind {
                        CallDefinitionKind::Free => ExpandedCallKind::Free,
                        CallDefinitionKind::Method => ExpandedCallKind::Method,
                        CallDefinitionKind::Lambda => ExpandedCallKind::Lambda,
                    },
                    name: row.name,
                });
            }
        }
    }
    for row in sites.const_inits {
        if expanded.is_macro_span(row.range.clone()) {
            if let Some(range) = expanded.map_span(row.range) {
                rows.defs.push(ExpandedCallDefRow { range, kind: ExpandedCallKind::ConstInit, name: Some(row.name) });
            }
        }
    }
    for row in sites.sites {
        if expanded.is_macro_span(row.range.clone()) {
            if let Some(range) = expanded.map_span(row.range) {
                rows.sites.push(ExpandedCallSiteRow {
                    range,
                    callee: row.callee,
                    callee_path: row.callee_path,
                });
            }
        }
    }
    rows.macros = expanded.macro_sites().into_iter().map(|(range, name)| (range, name.to_owned())).collect();
    rows
}
