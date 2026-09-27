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
    let trailing = mac
        .path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_default();
    if trailing != "macro_rules" {
        return trailing;
    }
    mac.tokens
        .clone()
        .into_iter()
        .find_map(|token| match token {
            proc_macro2::TokenTree::Ident(ident) => Some(ident.to_string()),
            _ => None,
        })
        .unwrap_or(trailing)
}

pub fn macro_invocation_rows_from_parsed(
    parsed: &syn::File,
    line_starts: &[u32],
) -> Vec<MacroInvocationRow> {
    let mut collector = Collector {
        line_starts,
        rows: Vec::new(),
    };
    collector.visit_file(parsed);
    collector
        .rows
        .sort_by_key(|row| row.range.end - row.range.start);
    collector.rows
}

pub fn macro_invocation_rows_from_tree(
    tree: &tree_sitter::Tree,
    source: &[u8],
) -> Vec<MacroInvocationRow> {
    let mut rows = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "macro_invocation" => {
                if let Some(path) = node.child_by_field_name("macro") {
                    let name = std::str::from_utf8(&source[path.byte_range()])
                        .expect("Rust macro paths are UTF-8")
                        .rsplit("::")
                        .next()
                        .unwrap_or_default()
                        .to_owned();
                    let end = if node.end_byte() > node.start_byte()
                        && source.get(node.end_byte() - 1) == Some(&b';')
                    {
                        node.end_byte() - 1
                    } else {
                        node.end_byte()
                    };
                    rows.push(MacroInvocationRow {
                        range: syn_compatible_byte(source, node.start_byte())
                            ..syn_compatible_byte(source, end),
                        name,
                    });
                }
            }
            "macro_definition" => {
                if node.child_by_field_name("name").is_some() {
                    rows.push(MacroInvocationRow {
                        range: syn_compatible_byte(source, node.start_byte())
                            ..syn_compatible_byte(source, node.end_byte()),
                        name: "macro_rules".to_owned(),
                    });
                }
            }
            _ => {}
        }
        let mut cursor = node.walk();
        let mut children = node.named_children(&mut cursor).collect::<Vec<_>>();
        children.reverse();
        stack.extend(children);
    }
    rows.sort_by_key(|row| row.range.end - row.range.start);
    rows
}

fn syn_compatible_byte(source: &[u8], offset: usize) -> u32 {
    let line_start = source[..offset]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |newline| newline + 1);
    let column = std::str::from_utf8(&source[line_start..offset])
        .expect("Rust source is UTF-8")
        .chars()
        .count();
    (line_start + column) as u32
}

pub fn macro_invocation_rows(content: &[u8]) -> Vec<MacroInvocationRow> {
    let Ok(text) = std::str::from_utf8(content) else {
        return Vec::new();
    };
    let Ok(parsed) = syn::parse_file(text) else {
        return Vec::new();
    };
    macro_invocation_rows_from_parsed(&parsed, &build_line_starts(text))
}
