//! Rust module and `use` leaves from the caller's existing syn parse.

use syn::spanned::Spanned;

use super::call_metadata_rows::line_col_to_byte;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModuleSpecifierKind {
    Named,
    Namespace,
    Reexport,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleSpecifierRow {
    pub range: std::ops::Range<u32>,
    pub name: String,
    pub kind: ModuleSpecifierKind,
    pub module: String,
}

pub fn module_specifier_rows(parsed: &syn::File, line_starts: &[u32]) -> Vec<ModuleSpecifierRow> {
    let mut rows = Vec::new();
    collect(&parsed.items, line_starts, &mut rows);
    rows
}

fn span_range(line_starts: &[u32], span: proc_macro2::Span) -> std::ops::Range<u32> {
    let start = span.start();
    let end = span.end();
    line_col_to_byte(line_starts, start.line as u32, start.column as u32)
        ..line_col_to_byte(line_starts, end.line as u32, end.column as u32)
}

fn collect(items: &[syn::Item], line_starts: &[u32], out: &mut Vec<ModuleSpecifierRow>) {
    for item in items {
        match item {
            syn::Item::Use(item) => {
                let reexport = !matches!(item.vis, syn::Visibility::Inherited);
                use_tree(&item.tree, reexport, line_starts, &mut Vec::new(), out);
            }
            syn::Item::Mod(item) => match &item.content {
                Some((_, inner)) => collect(inner, line_starts, out),
                None => {
                    let name = item.ident.to_string();
                    out.push(ModuleSpecifierRow {
                        range: span_range(line_starts, item.span()),
                        module: mod_path_attr(&item.attrs).unwrap_or_else(|| name.clone()),
                        name,
                        kind: ModuleSpecifierKind::Named,
                    });
                }
            },
            _ => {}
        }
    }
}

fn use_tree(
    tree: &syn::UseTree,
    reexport: bool,
    line_starts: &[u32],
    prefix: &mut Vec<String>,
    out: &mut Vec<ModuleSpecifierRow>,
) {
    match tree {
        syn::UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            use_tree(&path.tree, reexport, line_starts, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Group(group) => {
            for member in &group.items {
                use_tree(member, reexport, line_starts, prefix, out);
            }
        }
        syn::UseTree::Name(leaf) => push_leaf(
            &leaf.ident.to_string(),
            None,
            span_range(line_starts, leaf.ident.span()),
            reexport,
            prefix,
            out,
        ),
        syn::UseTree::Rename(leaf) => push_leaf(
            &leaf.ident.to_string(),
            Some(leaf.rename.to_string()),
            span_range(line_starts, leaf.span()),
            reexport,
            prefix,
            out,
        ),
        syn::UseTree::Glob(glob) => {
            let Some(last) = prefix.last() else { return };
            out.push(ModuleSpecifierRow {
                range: span_range(line_starts, glob.star_token.span()),
                name: last.clone(),
                kind: if reexport {
                    ModuleSpecifierKind::Reexport
                } else {
                    ModuleSpecifierKind::Namespace
                },
                module: prefix.join("::"),
            });
        }
    }
}

fn push_leaf(
    segment: &str,
    alias: Option<String>,
    range: std::ops::Range<u32>,
    reexport: bool,
    prefix: &[String],
    out: &mut Vec<ModuleSpecifierRow>,
) {
    let (name, module) = if segment == "self" {
        let Some(last) = prefix.last() else { return };
        (alias.unwrap_or_else(|| last.clone()), prefix.join("::"))
    } else {
        let mut segments = prefix.to_vec();
        segments.push(segment.to_string());
        (alias.unwrap_or_else(|| segment.to_string()), segments.join("::"))
    };
    out.push(ModuleSpecifierRow {
        range,
        name,
        kind: if reexport {
            ModuleSpecifierKind::Reexport
        } else {
            ModuleSpecifierKind::Named
        },
        module,
    });
}

/// The literal path from `#[path = "x.rs"]` on a Rust module declaration.
pub fn mod_path_attr(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("path") {
            return None;
        }
        match &attr.meta {
            syn::Meta::NameValue(pair) => match &pair.value {
                syn::Expr::Lit(literal) => match &literal.lit {
                    syn::Lit::Str(text) => Some(text.value()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    })
}
