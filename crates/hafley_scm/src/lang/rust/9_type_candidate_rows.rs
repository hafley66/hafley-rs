//! Unresolved Rust type references from the caller's existing syn parse.

use std::ops::Range;

use syn::visit::Visit;
use syn::{Fields, GenericParam, Path, Type, TypeParamBound, WherePredicate};

use super::call_metadata_rows::{path_name, primary_type, span_range};
use super::type_entity_rows::strip_type;
use super::type_refs::{collect_path_args, type_refs};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeCandidateKind {
    Field,
    Variant,
    Generic,
    Impl,
    Uses,
    Param,
    Returns,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeCandidateRow {
    pub to: String,
    pub kind: TypeCandidateKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypeCandidateOwner {
    Declared(Range<u32>),
    Synthetic {
        range: Range<u32>,
        name: String,
    },
    Impl {
        primary_name: String,
        bare_head: Option<(Range<u32>, String)>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeCandidateGroup {
    pub owner: TypeCandidateOwner,
    pub candidates: Vec<TypeCandidateRow>,
}

pub fn type_candidate_rows(parsed: &syn::File, line_starts: &[u32]) -> Vec<TypeCandidateGroup> {
    let mut groups = Vec::new();
    let file_types: Vec<String> = parsed.items.iter().filter_map(|item| match item {
        syn::Item::Struct(item) => Some(item.ident.to_string()),
        syn::Item::Enum(item) => Some(item.ident.to_string()),
        syn::Item::Union(item) => Some(item.ident.to_string()),
        syn::Item::Type(item) => Some(item.ident.to_string()),
        syn::Item::Trait(item) => Some(item.ident.to_string()),
        _ => None,
    }).collect();
    collect(&parsed.items, line_starts, &file_types, &[], &mut groups);
    groups
}

fn collect(
    items: &[syn::Item],
    line_starts: &[u32],
    file_types: &[String],
    shadowed: &[String],
    groups: &mut Vec<TypeCandidateGroup>,
) {
    for item in items {
        let first = groups.len();
        match item {
            syn::Item::Struct(item) => {
                let mut candidates = Vec::new();
                generic_candidates(&item.generics, &mut candidates);
                field_candidates(&item.fields, &mut candidates);
                retain_non_generic(&item.generics, &mut candidates);
                groups.push(declared(item.ident.span(), line_starts, candidates));
            }
            syn::Item::Enum(item) => {
                let mut candidates = Vec::new();
                let mut variant_groups = Vec::new();
                generic_candidates(&item.generics, &mut candidates);
                for variant in &item.variants {
                    candidates.push(TypeCandidateRow {
                        to: format!("{}::{}", item.ident, variant.ident),
                        kind: TypeCandidateKind::Variant,
                    });
                    field_candidates(&variant.fields, &mut candidates);
                    let mut variant_candidates = Vec::new();
                    field_candidates(&variant.fields, &mut variant_candidates);
                    variant_candidates.retain(|candidate| candidate.to.contains("::"));
                    retain_non_generic(&item.generics, &mut variant_candidates);
                    if !variant_candidates.is_empty() {
                        variant_groups.push(TypeCandidateGroup {
                            owner: TypeCandidateOwner::Synthetic {
                                range: span_range(line_starts, variant.ident.span()),
                                name: variant.ident.to_string(),
                            },
                            candidates: variant_candidates,
                        });
                    }
                }
                retain_non_generic(&item.generics, &mut candidates);
                groups.push(declared(item.ident.span(), line_starts, candidates));
                groups.extend(variant_groups);
            }
            syn::Item::Union(item) => {
                let mut candidates = Vec::new();
                generic_candidates(&item.generics, &mut candidates);
                field_candidates(&Fields::Named(item.fields.clone()), &mut candidates);
                retain_non_generic(&item.generics, &mut candidates);
                groups.push(declared(item.ident.span(), line_starts, candidates));
            }
            syn::Item::Type(item) => {
                let mut candidates = Vec::new();
                generic_candidates(&item.generics, &mut candidates);
                candidates.extend(type_refs(&item.ty).into_iter().map(|to| TypeCandidateRow {
                    to,
                    kind: TypeCandidateKind::Uses,
                }));
                retain_non_generic(&item.generics, &mut candidates);
                groups.push(declared(item.ident.span(), line_starts, candidates));
            }
            syn::Item::Fn(item) => {
                let mut candidates = signature_candidates(&item.sig);
                candidates.extend(body_type_candidates(&item.block));
                retain_non_generic(&item.sig.generics, &mut candidates);
                groups.push(declared(
                    item.sig.ident.span(),
                    line_starts,
                    candidates,
                ));
            }
            syn::Item::Trait(item) => {
                let mut candidates = Vec::new();
                generic_candidates(&item.generics, &mut candidates);
                for bound in &item.supertraits {
                    bound_candidate(bound, &mut candidates);
                }
                retain_non_generic(&item.generics, &mut candidates);
                groups.push(declared(item.ident.span(), line_starts, candidates));
                for child in &item.items {
                    if let syn::TraitItem::Fn(method) = child {
                        let mut candidates = signature_candidates(&method.sig);
                        if let Some(body) = &method.default {
                            candidates.extend(body_type_candidates(body));
                        }
                        retain_non_generic(&item.generics, &mut candidates);
                        retain_non_generic(&method.sig.generics, &mut candidates);
                        if method.default.is_some() {
                            groups.push(declared(method.sig.ident.span(), line_starts, candidates));
                        } else {
                            groups.push(TypeCandidateGroup {
                                owner: TypeCandidateOwner::Synthetic {
                                    range: span_range(line_starts, method.sig.ident.span()),
                                    name: method.sig.ident.to_string(),
                                },
                                candidates,
                            });
                        }
                    }
                }
            }
            syn::Item::Impl(item) => {
                let Some(primary_name) = primary_type(&item.self_ty) else {
                    continue;
                };
                let bare_head = bare_self_head(&item.self_ty, line_starts);
                let mut candidates = Vec::new();
                generic_candidates(&item.generics, &mut candidates);
                if let Some((_, path, _)) = &item.trait_ {
                    if let Some(to) = path_name(path) {
                        candidates.push(TypeCandidateRow {
                            to,
                            kind: TypeCandidateKind::Impl,
                        });
                    }
                    arg_candidates(path, &mut candidates);
                }
                if let Type::Path(path) = strip_type(&item.self_ty) {
                    arg_candidates(&path.path, &mut candidates);
                }
                retain_non_generic(&item.generics, &mut candidates);
                groups.push(TypeCandidateGroup {
                    owner: TypeCandidateOwner::Impl {
                        primary_name,
                        bare_head,
                    },
                    candidates,
                });
                for child in &item.items {
                    if let syn::ImplItem::Fn(method) = child {
                        let mut candidates = signature_candidates(&method.sig);
                        candidates.extend(body_type_candidates(&method.block));
                        retain_non_generic(&item.generics, &mut candidates);
                        retain_non_generic(&method.sig.generics, &mut candidates);
                        groups.push(declared(
                            method.sig.ident.span(),
                            line_starts,
                            candidates,
                        ));
                    }
                    if let syn::ImplItem::Type(assoc) = child {
                        if matches!(&assoc.ty, Type::Path(path)
                            if path.qself.is_none() && path.path.segments.len() == 1
                                && path.path.segments.first().is_some_and(|segment| {
                                    matches!(segment.arguments, syn::PathArguments::None)
                                })) {
                            continue;
                        }
                        let mut candidates: Vec<_> = type_refs(&assoc.ty).into_iter().map(|to| TypeCandidateRow {
                            to,
                            kind: TypeCandidateKind::Uses,
                        }).collect();
                        retain_non_generic(&item.generics, &mut candidates);
                        retain_non_generic(&assoc.generics, &mut candidates);
                        groups.push(TypeCandidateGroup {
                            owner: TypeCandidateOwner::Synthetic {
                                range: span_range(line_starts, assoc.ident.span()),
                                name: assoc.ident.to_string(),
                            },
                            candidates,
                        });
                    }
                }
            }
            syn::Item::Mod(item) => {
                if let Some((_, inner)) = &item.content {
                    let mut inner_shadowed = shadowed.to_vec();
                    for imported in external_imported_locals(inner) {
                        if file_types.contains(&imported) && !inner_shadowed.contains(&imported) {
                            inner_shadowed.push(imported);
                        }
                    }
                    collect(inner, line_starts, file_types, &inner_shadowed, groups);
                }
            }
            _ => {}
        }
        for group in &mut groups[first..] {
            group.candidates.retain(|candidate| !shadowed.contains(&candidate.to));
        }
    }
}

#[derive(Default)]
struct BodyTypeWalk {
    candidates: Vec<TypeCandidateRow>,
}

impl<'ast> Visit<'ast> for BodyTypeWalk {
    fn visit_pat_type(&mut self, pat: &'ast syn::PatType) {
        self.candidates.extend(type_refs(&pat.ty).into_iter().map(|to| TypeCandidateRow {
            to,
            kind: TypeCandidateKind::Uses,
        }));
        syn::visit::visit_pat_type(self, pat);
    }

    fn visit_expr_closure(&mut self, _: &'ast syn::ExprClosure) {}
    fn visit_item(&mut self, _: &'ast syn::Item) {}
}

fn body_type_candidates(body: &syn::Block) -> Vec<TypeCandidateRow> {
    let mut walk = BodyTypeWalk::default();
    walk.visit_block(body);
    walk.candidates
}

fn external_imported_locals(items: &[syn::Item]) -> Vec<String> {
    fn names(tree: &syn::UseTree, out: &mut Vec<String>) {
        match tree {
            syn::UseTree::Path(path) => names(&path.tree, out),
            syn::UseTree::Group(group) => {
                for member in &group.items { names(member, out); }
            }
            syn::UseTree::Name(name) => out.push(name.ident.to_string()),
            syn::UseTree::Rename(rename) => out.push(rename.rename.to_string()),
            syn::UseTree::Glob(_) => {}
        }
    }
    let mut out = Vec::new();
    for item in items {
        let syn::Item::Use(item) = item else { continue };
        let syn::UseTree::Path(root) = &item.tree else { continue };
        if matches!(root.ident.to_string().as_str(), "crate" | "self" | "super") { continue; }
        names(&root.tree, &mut out);
    }
    out
}

fn signature_candidates(sig: &syn::Signature) -> Vec<TypeCandidateRow> {
    let mut candidates = Vec::new();
    generic_candidates(&sig.generics, &mut candidates);
    let bounds: Vec<(String, String)> = sig.generics.params.iter().filter_map(|param| {
        let GenericParam::Type(param) = param else { return None };
        let trait_name = param.bounds.iter().find_map(|bound| {
            let TypeParamBound::Trait(bound) = bound else { return None };
            path_name(&bound.path)
        })?;
        Some((param.ident.to_string(), trait_name))
    }).chain(sig.generics.where_clause.iter().flat_map(|clause| clause.predicates.iter()).filter_map(|pred| {
        let WherePredicate::Type(pred) = pred else { return None };
        let Type::Path(ty) = &pred.bounded_ty else { return None };
        let ident = ty.path.get_ident()?.to_string();
        let trait_name = pred.bounds.iter().find_map(|bound| {
            let TypeParamBound::Trait(bound) = bound else { return None };
            path_name(&bound.path)
        })?;
        Some((ident, trait_name))
    })).collect();
    for arg in &sig.inputs {
        if let syn::FnArg::Typed(arg) = arg {
            candidates.extend(type_refs(&arg.ty).into_iter().map(|to| TypeCandidateRow {
                to: projection_trait(&to, &bounds),
                kind: TypeCandidateKind::Param,
            }));
        }
    }
    if let syn::ReturnType::Type(_, ty) = &sig.output {
        candidates.extend(type_refs(ty).into_iter().map(|to| TypeCandidateRow {
            to: projection_trait(&to, &bounds),
            kind: TypeCandidateKind::Returns,
        }));
    }
    retain_non_generic(&sig.generics, &mut candidates);
    candidates
}

fn retain_non_generic(generics: &syn::Generics, candidates: &mut Vec<TypeCandidateRow>) {
    let names: Vec<String> = generics.params.iter().filter_map(|param| {
        let GenericParam::Type(param) = param else { return None };
        Some(param.ident.to_string())
    }).collect();
    candidates.retain(|candidate| !names.contains(&candidate.to));
}

fn projection_trait(name: &str, bounds: &[(String, String)]) -> String {
    let Some((head, slot)) = name.split_once("::") else { return name.to_string() };
    bounds.iter().find(|(param, _)| param == head)
        .map_or_else(|| name.to_string(), |(_, trait_name)| format!("{trait_name}::{slot}"))
}

pub fn bare_self_head(ty: &Type, line_starts: &[u32]) -> Option<(Range<u32>, String)> {
    match strip_type(ty) {
        Type::Path(path) if path.qself.is_none() && path.path.segments.len() == 1 => {
            path.path.segments.first().map(|seg| {
                (
                    span_range(line_starts, seg.ident.span()),
                    seg.ident.to_string(),
                )
            })
        }
        _ => None,
    }
}

fn declared(
    span: proc_macro2::Span,
    line_starts: &[u32],
    candidates: Vec<TypeCandidateRow>,
) -> TypeCandidateGroup {
    TypeCandidateGroup {
        owner: TypeCandidateOwner::Declared(span_range(line_starts, span)),
        candidates,
    }
}

fn field_candidates(fields: &Fields, candidates: &mut Vec<TypeCandidateRow>) {
    for field in fields.iter() {
        candidates.extend(type_refs(&field.ty).into_iter().map(|to| TypeCandidateRow {
            to,
            kind: TypeCandidateKind::Field,
        }));
    }
}

fn generic_candidates(generics: &syn::Generics, candidates: &mut Vec<TypeCandidateRow>) {
    for param in &generics.params {
        if let GenericParam::Type(param) = param {
            for bound in &param.bounds {
                bound_candidate(bound, candidates);
            }
        }
    }
    if let Some(where_clause) = &generics.where_clause {
        for pred in &where_clause.predicates {
            if let WherePredicate::Type(pred) = pred {
                for bound in &pred.bounds {
                    bound_candidate(bound, candidates);
                }
            }
        }
    }
}

fn bound_candidate(bound: &TypeParamBound, candidates: &mut Vec<TypeCandidateRow>) {
    if let TypeParamBound::Trait(bound) = bound {
        if let Some(to) = path_name(&bound.path) {
            candidates.push(TypeCandidateRow {
                to,
                kind: TypeCandidateKind::Generic,
            });
        }
        arg_candidates(&bound.path, candidates);
    }
}

fn arg_candidates(path: &Path, candidates: &mut Vec<TypeCandidateRow>) {
    let mut args = Vec::new();
    collect_path_args(path, &mut args);
    args.sort();
    args.dedup();
    candidates.extend(args.into_iter().map(|to| TypeCandidateRow {
        to,
        kind: TypeCandidateKind::Generic,
    }));
}
