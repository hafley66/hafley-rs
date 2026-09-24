//! Rust declared type/callable entities and their signature references.

use syn::spanned::Spanned;

use super::call_metadata_rows::span_range;
use super::type_refs::type_refs;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeEntityKind {
    Struct,
    Enum,
    Alias,
    Trait,
    Function,
    Method,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignatureSlot {
    Param,
    Ret,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignatureRef {
    pub slot: SignatureSlot,
    pub pos: u32,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeEntityRow {
    pub range: std::ops::Range<u32>,
    pub name: String,
    pub kind: TypeEntityKind,
    pub sigs: Vec<SignatureRef>,
}

pub fn type_entity_rows(parsed: &syn::File, line_starts: &[u32]) -> Vec<TypeEntityRow> {
    let mut rows = Vec::new();
    collect(&parsed.items, line_starts, &mut rows);
    rows
}

fn collect(items: &[syn::Item], line_starts: &[u32], rows: &mut Vec<TypeEntityRow>) {
    for item in items {
        match item {
            syn::Item::Struct(item) => rows.push(named(
                item.ident.span(),
                item.ident.to_string(),
                TypeEntityKind::Struct,
                line_starts,
            )),
            syn::Item::Enum(item) => rows.push(named(
                item.ident.span(),
                item.ident.to_string(),
                TypeEntityKind::Enum,
                line_starts,
            )),
            // The existing TypeF vocabulary maps Rust unions onto Struct.
            syn::Item::Union(item) => rows.push(named(
                item.ident.span(),
                item.ident.to_string(),
                TypeEntityKind::Struct,
                line_starts,
            )),
            syn::Item::Type(item) => rows.push(named(
                item.ident.span(),
                item.ident.to_string(),
                TypeEntityKind::Alias,
                line_starts,
            )),
            syn::Item::Trait(item) => {
                rows.push(named(
                    item.ident.span(),
                    item.ident.to_string(),
                    TypeEntityKind::Trait,
                    line_starts,
                ));
                for child in &item.items {
                    if let syn::TraitItem::Fn(method) = child {
                        if method.default.is_some() {
                            rows.push(callable(&method.sig, TypeEntityKind::Method, line_starts));
                        }
                    }
                }
            }
            syn::Item::Fn(item) => rows.push(callable(
                &item.sig,
                TypeEntityKind::Function,
                line_starts,
            )),
            syn::Item::Impl(item) => {
                for child in &item.items {
                    if let syn::ImplItem::Fn(method) = child {
                        rows.push(callable(&method.sig, TypeEntityKind::Method, line_starts));
                    }
                }
            }
            syn::Item::Mod(item) => {
                if let Some((_, inner)) = &item.content {
                    collect(inner, line_starts, rows);
                }
            }
            _ => {}
        }
    }
}

fn named(
    span: proc_macro2::Span,
    name: String,
    kind: TypeEntityKind,
    line_starts: &[u32],
) -> TypeEntityRow {
    TypeEntityRow {
        range: span_range(line_starts, span),
        name,
        kind,
        sigs: Vec::new(),
    }
}

fn callable(sig: &syn::Signature, kind: TypeEntityKind, line_starts: &[u32]) -> TypeEntityRow {
    let mut row = named(sig.ident.span(), sig.ident.to_string(), kind, line_starts);
    let mut pos = 0u32;
    for arg in &sig.inputs {
        if let syn::FnArg::Typed(arg) = arg {
            for name in type_refs(&arg.ty) {
                row.sigs.push(SignatureRef {
                    slot: SignatureSlot::Param,
                    pos,
                    name,
                });
            }
            pos += 1;
        }
    }
    if let syn::ReturnType::Type(_, ty) = &sig.output {
        for name in type_refs(ty) {
            row.sigs.push(SignatureRef {
                slot: SignatureSlot::Ret,
                pos: 0,
                name,
            });
        }
    }
    row
}
