//! Item-level Rust string consts from the caller's existing syn parse.

use syn::spanned::Spanned;

use super::call_metadata_rows::span_range;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConstStringRow {
    pub range: std::ops::Range<u32>,
    pub name: String,
    pub value: String,
}

pub fn const_string_rows(parsed: &syn::File, line_starts: &[u32]) -> Vec<ConstStringRow> {
    let mut rows = Vec::new();
    collect(&parsed.items, line_starts, &mut rows);
    rows
}

fn collect(items: &[syn::Item], line_starts: &[u32], rows: &mut Vec<ConstStringRow>) {
    for item in items {
        if let syn::Item::Mod(module) = item {
            if let Some((_, inner)) = &module.content {
                collect(inner, line_starts, rows);
            }
            continue;
        }
        let syn::Item::Const(item) = item else { continue };
        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(value),
            ..
        }) = &*item.expr else {
            continue;
        };
        rows.push(ConstStringRow {
            range: span_range(line_starts, item.ident.span()),
            name: item.ident.to_string(),
            value: value.value(),
        });
    }
}
