//! Macro invocation spans reused by phase-1 extraction and SCIP resolution.

use std::ops::Range;

use syn::spanned::Spanned;
use syn::visit::Visit;

use super::call_metadata_rows::{build_line_starts, line_col_to_byte};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacroInvocationRow {
    pub range: Range<u32>,
    pub name: String,
}

struct Collector<'a> {
    line_starts: &'a [u32],
    rows: Vec<MacroInvocationRow>,
}

impl<'ast> Visit<'ast> for Collector<'_> {
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        let span = mac.span();
        let start = span.start();
        let end = span.end();
        self.rows.push(MacroInvocationRow {
            range: line_col_to_byte(self.line_starts, start.line as u32, start.column as u32)
                ..line_col_to_byte(self.line_starts, end.line as u32, end.column as u32),
            name: macro_name(mac),
        });
        syn::visit::visit_macro(self, mac);
    }
}

fn macro_name(mac: &syn::Macro) -> String {
    let trailing = mac.path.segments.last().map(|segment| segment.ident.to_string()).unwrap_or_default();
    if trailing != "macro_rules" {
        return trailing;
    }
    mac.tokens.clone().into_iter().find_map(|token| match token {
        proc_macro2::TokenTree::Ident(ident) => Some(ident.to_string()),
        _ => None,
    }).unwrap_or(trailing)
}

pub fn macro_invocation_rows_from_parsed(parsed: &syn::File, line_starts: &[u32]) -> Vec<MacroInvocationRow> {
    let mut collector = Collector { line_starts, rows: Vec::new() };
    collector.visit_file(parsed);
    collector.rows.sort_by_key(|row| row.range.end - row.range.start);
    collector.rows
}

pub fn macro_invocation_rows(content: &[u8]) -> Vec<MacroInvocationRow> {
    let Ok(text) = std::str::from_utf8(content) else { return Vec::new(); };
    let Ok(parsed) = syn::parse_file(text) else { return Vec::new(); };
    macro_invocation_rows_from_parsed(&parsed, &build_line_starts(text))
}
