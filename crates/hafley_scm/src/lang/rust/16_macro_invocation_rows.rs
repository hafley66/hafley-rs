//! Macro invocation spans reused by phase-1 extraction and SCIP resolution.

use std::ops::Range;

use syn::spanned::Spanned;
use syn::visit::Visit;

use super::call_metadata_rows::span_range;
use super::syntax::parse_rust_file;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacroInvocationRow {
    pub range: Range<u32>,
    pub name: String,
}

struct Collector {
    rows: Vec<MacroInvocationRow>,
}

impl<'ast> Visit<'ast> for Collector {
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.rows.push(MacroInvocationRow {
            range: span_range(mac.span()),
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

pub fn macro_invocation_rows_from_parsed(parsed: &syn::File) -> Vec<MacroInvocationRow> {
    let mut collector = Collector {
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
                        range: node.start_byte() as u32..end as u32,
                        name,
                    });
                }
            }
            "macro_definition" => {
                if node.child_by_field_name("name").is_some() {
                    rows.push(MacroInvocationRow {
                        range: node.start_byte() as u32..node.end_byte() as u32,
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

pub fn macro_invocation_rows(content: &[u8]) -> Vec<MacroInvocationRow> {
    let Ok(text) = std::str::from_utf8(content) else {
        return Vec::new();
    };
    let Ok(parsed) = parse_rust_file(text) else {
        return Vec::new();
    };
    macro_invocation_rows_from_parsed(&parsed)
}
