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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImplMethodsRow {
    pub self_type: String,
    pub trait_name: Option<String>,
    pub methods: Vec<(String, Range<u32>)>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ModuleResolutionRows {
    pub uses: Vec<UseBindingRow>,
    pub stars: Vec<StarImportRow>,
    pub inline_mods: Vec<String>,
    pub mod_decls: Vec<(String, Option<String>)>,
    pub mod_scopes: std::collections::BTreeMap<String, Vec<(String, Option<String>)>>,
    pub enums: Vec<EnumVariantsRow>,
    pub traits: Vec<TraitMethodsRow>,
    pub aliases: Vec<Range<u32>>,
    pub impls: Vec<ImplMethodsRow>,
}

pub fn module_resolution_rows(parsed: &syn::File) -> ModuleResolutionRows {
    let mut rows = ModuleResolutionRows::default();
    collect(&parsed.items, &mut rows, &[]);
    rows
}

fn collect(
    items: &[syn::Item],
    rows: &mut ModuleResolutionRows,
    scope: &[(String, Option<String>)],
) {
    for item in items {
        match item {
            syn::Item::Use(item) => {
                let reexport = !matches!(item.vis, syn::Visibility::Inherited);
                walk_use_tree(&item.tree, reexport, &mut Vec::new(), rows);
            }
            syn::Item::Trait(item) => {
                let methods = item
                    .items
                    .iter()
                    .filter_map(|child| {
                        let syn::TraitItem::Fn(method) = child else {
                            return None;
                        };
                        let end = method
                            .default
                            .as_ref()
                            .map_or(method.sig.span(), |body| body.span());
                        let (start, end) = def_range(method.sig.ident.span(), end);
                        Some(TraitMethodRow {
                            name: method.sig.ident.to_string(),
                            range: start..end,
                            default: method.default.is_some(),
                        })
                    })
                    .collect();
                rows.traits.push(TraitMethodsRow {
                    name: item.ident.to_string(),
                    methods,
                });
            }
            syn::Item::Mod(item) => match &item.content {
                Some((_, inner)) => {
                    rows.inline_mods.push(item.ident.to_string());
                    let mut nested = scope.to_vec();
                    nested.push((item.ident.to_string(), mod_path_attr(&item.attrs)));
                    collect(inner, rows, &nested);
                }
                None => {
                    let name = scope
                        .iter()
                        .map(|(name, _)| name.as_str())
                        .chain(std::iter::once(item.ident.to_string().as_str()))
                        .collect::<Vec<_>>()
                        .join("::");
                    rows.mod_scopes.insert(name.clone(), scope.to_vec());
                    rows.mod_decls.push((name, mod_path_attr(&item.attrs)));
                }
            },
            syn::Item::Enum(item) => {
                let variants = item
                    .variants
                    .iter()
                    .filter_map(|variant| {
                        variant_def_range(variant)
                            .map(|(start, end)| (variant.ident.to_string(), start..end))
                    })
                    .collect();
                rows.enums.push(EnumVariantsRow {
                    name: item.ident.to_string(),
                    variants,
                });
            }
            syn::Item::Type(item) => rows.aliases.push(span_range(item.ident.span())),
            syn::Item::Impl(item) => {
                if let Some(self_type) = principal_ty(&item.self_ty) {
                    let trait_name = item.trait_.as_ref().and_then(|(path, _)| {
                        path.segments
                            .last()
                            .map(|segment| segment.ident.to_string())
                    });
                    let methods = item
                        .items
                        .iter()
                        .filter_map(|child| {
                            let syn::ImplItem::Fn(method) = child else {
                                return None;
                            };
                            let (start, end) =
                                def_range(method.sig.ident.span(), method.block.span());
                            Some((method.sig.ident.to_string(), start..end))
                        })
                        .collect();
                    rows.impls.push(ImplMethodsRow {
                        self_type,
                        trait_name,
                        methods,
                    });
                }
            }
            _ => {}
        }
    }
}

/// Principal receiver name, peeling pointer and reference wrappers and one
/// `Result<T, _>` or `Option<T>` layer.
pub fn principal_ty(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Reference(r) => principal_ty(&r.elem),
        syn::Type::Ptr(p) => principal_ty(&p.elem),
        syn::Type::Paren(p) => principal_ty(&p.elem),
        syn::Type::Group(g) => principal_ty(&g.elem),
        syn::Type::Path(p) => {
            let segment = p.path.segments.last()?;
            let ident = segment.ident.to_string();
            if matches!(ident.as_str(), "Result" | "Option") {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return principal_ty(inner);
                    }
                }
            }
            Some(ident)
        }
        syn::Type::TraitObject(t) => single_bound_trait(&t.bounds),
        syn::Type::ImplTrait(t) => single_bound_trait(&t.bounds),
        _ => None,
    }
}

fn single_bound_trait(
    bounds: &syn::punctuated::Punctuated<syn::TypeParamBound, syn::token::Plus>,
) -> Option<String> {
    if bounds.len() != 1 {
        return None;
    }
    match bounds.first()? {
        syn::TypeParamBound::Trait(tb) => Some(tb.path.segments.last()?.ident.to_string()),
        _ => None,
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
            push_leaf(
                prefix,
                &leaf.ident.to_string(),
                Some(leaf.rename.to_string()),
                reexport,
                rows,
            );
        }
        syn::UseTree::Glob(_) => rows.stars.push(StarImportRow {
            qualifier: prefix.clone(),
            reexport,
        }),
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
        let Some((last, rest)) = prefix.split_last() else {
            return;
        };
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
