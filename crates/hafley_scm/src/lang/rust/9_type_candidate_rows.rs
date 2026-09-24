//! Unresolved Rust type references from the caller's existing syn parse.

use std::ops::Range;

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
    collect(&parsed.items, line_starts, &mut groups);
    groups
}

fn collect(items: &[syn::Item], line_starts: &[u32], groups: &mut Vec<TypeCandidateGroup>) {
    for item in items {
        match item {
            syn::Item::Struct(item) => {
                let mut candidates = Vec::new();
                generic_candidates(&item.generics, &mut candidates);
                field_candidates(&item.fields, &mut candidates);
                groups.push(declared(item.ident.span(), line_starts, candidates));
            }
            syn::Item::Enum(item) => {
                let mut candidates = Vec::new();
                generic_candidates(&item.generics, &mut candidates);
                for variant in &item.variants {
                    candidates.push(TypeCandidateRow {
                        to: format!("{}::{}", item.ident, variant.ident),
                        kind: TypeCandidateKind::Variant,
                    });
                    field_candidates(&variant.fields, &mut candidates);
                }
                groups.push(declared(item.ident.span(), line_starts, candidates));
            }
            syn::Item::Union(item) => {
                let mut candidates = Vec::new();
                generic_candidates(&item.generics, &mut candidates);
                field_candidates(&Fields::Named(item.fields.clone()), &mut candidates);
                groups.push(declared(item.ident.span(), line_starts, candidates));
            }
            syn::Item::Type(item) => {
                let mut candidates = Vec::new();
                generic_candidates(&item.generics, &mut candidates);
                candidates.extend(type_refs(&item.ty).into_iter().map(|to| TypeCandidateRow {
                    to,
                    kind: TypeCandidateKind::Uses,
                }));
                groups.push(declared(item.ident.span(), line_starts, candidates));
            }
            syn::Item::Fn(item) => {
                groups.push(declared(
                    item.sig.ident.span(),
                    line_starts,
                    signature_candidates(&item.sig),
                ));
            }
            syn::Item::Trait(item) => {
                let mut candidates = Vec::new();
                generic_candidates(&item.generics, &mut candidates);
                for bound in &item.supertraits {
                    bound_candidate(bound, &mut candidates);
                }
                groups.push(declared(item.ident.span(), line_starts, candidates));
                for child in &item.items {
                    if let syn::TraitItem::Fn(method) = child {
                        if method.default.is_some() {
                            groups.push(declared(
                                method.sig.ident.span(),
                                line_starts,
                                signature_candidates(&method.sig),
                            ));
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
                groups.push(TypeCandidateGroup {
                    owner: TypeCandidateOwner::Impl {
                        primary_name,
                        bare_head,
                    },
                    candidates,
                });
                for child in &item.items {
                    if let syn::ImplItem::Fn(method) = child {
                        groups.push(declared(
                            method.sig.ident.span(),
                            line_starts,
                            signature_candidates(&method.sig),
                        ));
                    }
                }
            }
            syn::Item::Mod(item) => {
                if let Some((_, inner)) = &item.content {
                    collect(inner, line_starts, groups);
                }
            }
            _ => {}
        }
    }
}

fn signature_candidates(sig: &syn::Signature) -> Vec<TypeCandidateRow> {
    let mut candidates = Vec::new();
    for arg in &sig.inputs {
        if let syn::FnArg::Typed(arg) = arg {
            candidates.extend(type_refs(&arg.ty).into_iter().map(|to| TypeCandidateRow {
                to,
                kind: TypeCandidateKind::Param,
            }));
        }
    }
    if let syn::ReturnType::Type(_, ty) = &sig.output {
        candidates.extend(type_refs(ty).into_iter().map(|to| TypeCandidateRow {
            to,
            kind: TypeCandidateKind::Returns,
        }));
    }
    candidates
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
