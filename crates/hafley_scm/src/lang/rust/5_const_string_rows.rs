//! Item-level Rust string consts from the caller's existing syn parse.

use super::call_metadata_rows::span_range;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConstStringRow {
    pub range: std::ops::Range<u32>,
    pub name: String,
    pub value: String,
}

pub fn const_string_rows(parsed: &syn::File) -> Vec<ConstStringRow> {
    let mut rows = Vec::new();
    collect(&parsed.items, &mut rows);
    rows
}

pub fn const_string_rows_from_tree(tree: &tree_sitter::Tree, source: &[u8]) -> Vec<ConstStringRow> {
    let mut rows = Vec::new();
    collect_tree_consts(tree.root_node(), source, &mut rows);
    rows
}

fn collect_tree_consts(node: tree_sitter::Node<'_>, source: &[u8], rows: &mut Vec<ConstStringRow>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "mod_item" => {
                if let Some(body) = child.child_by_field_name("body") {
                    collect_tree_consts(body, source, rows);
                }
            }
            "const_item" => {
                let (Some(name), Some(value)) = (
                    child.child_by_field_name("name"),
                    child.child_by_field_name("value"),
                ) else {
                    continue;
                };
                if !matches!(value.kind(), "string_literal" | "raw_string_literal") {
                    continue;
                }
                let Ok(literal) = std::str::from_utf8(&source[value.byte_range()]) else {
                    continue;
                };
                let Some(value) = decode_string_literal(literal) else {
                    continue;
                };
                let Ok(name) = std::str::from_utf8(&source[name.byte_range()]) else {
                    continue;
                };
                rows.push(ConstStringRow {
                    range: child
                        .child_by_field_name("name")
                        .map(|name| name.start_byte() as u32..name.end_byte() as u32)
                        .unwrap_or_default(),
                    name: name.to_owned(),
                    value,
                });
            }
            _ => {}
        }
    }
}

pub(super) fn decode_string_literal(literal: &str) -> Option<String> {
    let raw_start = literal.find('"')?;
    if literal.starts_with('r') {
        let hashes = literal[..raw_start]
            .bytes()
            .filter(|byte| *byte == b'#')
            .count();
        let closing = format!("\"{}", "#".repeat(hashes));
        return literal
            .get(raw_start + 1..literal.len().checked_sub(closing.len())?)
            .map(str::to_owned);
    }
    let body = literal.get(raw_start + 1..literal.len().checked_sub(1)?)?;
    let mut chars = body.chars().peekable();
    let mut out = String::new();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next()? {
            '\\' => out.push('\\'),
            '\'' => out.push('\''),
            '"' => out.push('"'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '0' => out.push('\0'),
            'x' => {
                let digits: String = chars.by_ref().take(2).collect();
                out.push(char::from_u32(u32::from_str_radix(&digits, 16).ok()?)?);
            }
            'u' => {
                if chars.next()? != '{' {
                    return None;
                }
                let digits: String = chars.by_ref().take_while(|ch| *ch != '}').collect();
                let digits = digits.chars().filter(|ch| *ch != '_').collect::<String>();
                out.push(char::from_u32(u32::from_str_radix(&digits, 16).ok()?)?);
            }
            '\n' => {
                while chars.peek().is_some_and(|ch| ch.is_whitespace()) {
                    chars.next();
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

fn collect(items: &[syn::Item], rows: &mut Vec<ConstStringRow>) {
    for item in items {
        if let syn::Item::Mod(module) = item {
            if let Some((_, inner)) = &module.content {
                collect(inner, rows);
            }
            continue;
        }
        let syn::Item::Const(item) = item else {
            continue;
        };
        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(value),
            ..
        }) = &*item.expr
        else {
            continue;
        };
        rows.push(ConstStringRow {
            range: span_range(item.ident.span()),
            name: item.ident.to_string(),
            value: value.value(),
        });
    }
}
