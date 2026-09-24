//! Rust module-resolution syntax from an existing syn parse.

use std::ops::Range;

use syn::spanned::Spanned;

use super::call_metadata_rows::{def_range, span_range, variant_def_range};
use super::module_specifier_rows::mod_path_attr;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UseBindingRow {
    pub local: String,
    pub qualifier: Vec<String>,
    pub asked: String,
    pub reexport: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StarImportRow {
    pub qualifier: Vec<String>,
    pub reexport: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraitMethodRow {
    pub name: String,
    pub range: Range<u32>,
    pub default: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraitMethodsRow {
    pub name: String,
    pub methods: Vec<TraitMethodRow>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnumVariantsRow {
    pub name: String,
    pub variants: Vec<(String, Range<u32>)>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ModuleResolutionRows {
    pub uses: Vec<UseBindingRow>,
    pub stars: Vec<StarImportRow>,
    pub inline_mods: Vec<String>,
    pub mod_decls: Vec<(String, Option<String>)>,
    pub enums: Vec<EnumVariantsRow>,
    pub traits: Vec<TraitMethodsRow>,
    pub aliases: Vec<Range<u32>>,
}

pub fn module_resolution_rows(parsed: &syn::File, line_starts: &[u32]) -> ModuleResolutionRows {
    let mut rows = ModuleResolutionRows::default();
    collect(&parsed.items, line_starts, &mut rows);
    rows
}

fn collect(items: &[syn::Item], line_starts: &[u32], rows: &mut ModuleResolutionRows) {
    for item in items {
        match item {
            syn::Item::Use(item) => {
                let reexport = !matches!(item.vis, syn::Visibility::Inherited);
                walk_use_tree(&item.tree, reexport, &mut Vec::new(), rows);
            }
            syn::Item::Trait(item) => {
                let methods = item.items.iter().filter_map(|child| {
                    let syn::TraitItem::Fn(method) = child else { return None };
                    let end = method.default.as_ref().map_or(method.sig.span(), |body| body.span());
                    let (start, end) = def_range(line_starts, method.sig.ident.span(), end);
                    Some(TraitMethodRow {
                        name: method.sig.ident.to_string(),
                        range: start..end,
                        default: method.default.is_some(),
                    })
                }).collect();
                rows.traits.push(TraitMethodsRow { name: item.ident.to_string(), methods });
            }
            syn::Item::Mod(item) => match &item.content {
                Some((_, inner)) => {
                    rows.inline_mods.push(item.ident.to_string());
                    collect(inner, line_starts, rows);
                }
                None => rows.mod_decls.push((item.ident.to_string(), mod_path_attr(&item.attrs))),
            },
            syn::Item::Enum(item) => {
                let variants = item.variants.iter().filter_map(|variant| {
                    variant_def_range(line_starts, variant)
                        .map(|(start, end)| (variant.ident.to_string(), start..end))
                }).collect();
                rows.enums.push(EnumVariantsRow { name: item.ident.to_string(), variants });
            }
            syn::Item::Type(item) => rows.aliases.push(span_range(line_starts, item.ident.span())),
            _ => {}
        }
    }
}

fn walk_use_tree(
    tree: &syn::UseTree,
    reexport: bool,
    prefix: &mut Vec<String>,
    rows: &mut ModuleResolutionRows,
) {
    match tree {
        syn::UseTree::Path(segment) => {
            prefix.push(segment.ident.to_string());
            walk_use_tree(&segment.tree, reexport, prefix, rows);
            prefix.pop();
        }
        syn::UseTree::Group(group) => {
            for member in &group.items {
                walk_use_tree(member, reexport, prefix, rows);
            }
        }
        syn::UseTree::Name(leaf) => {
            push_leaf(prefix, &leaf.ident.to_string(), None, reexport, rows);
        }
        syn::UseTree::Rename(leaf) => {
            push_leaf(prefix, &leaf.ident.to_string(), Some(leaf.rename.to_string()), reexport, rows);
        }
        syn::UseTree::Glob(_) => rows.stars.push(StarImportRow { qualifier: prefix.clone(), reexport }),
    }
}

fn push_leaf(
    prefix: &[String],
    segment: &str,
    alias: Option<String>,
    reexport: bool,
    rows: &mut ModuleResolutionRows,
) {
    let (qualifier, asked) = if segment == "self" {
        let Some((last, rest)) = prefix.split_last() else { return };
        (rest.to_vec(), last.clone())
    } else {
        (prefix.to_vec(), segment.to_string())
    };
    rows.uses.push(UseBindingRow {
        local: alias.unwrap_or_else(|| asked.clone()),
        qualifier,
        asked,
        reexport,
    });
}
