//! Rust declared type/callable entities and their signature references.

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImplSelfHeadRow {
    pub range: std::ops::Range<u32>,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocSectionRow {
    pub heading: String,
    pub body: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocRow {
    pub range: std::ops::Range<u32>,
    pub parent: Option<String>,
    pub text: String,
    pub sections: Vec<DocSectionRow>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TypeEntityRows {
    pub entities: Vec<TypeEntityRow>,
    pub impl_self_heads: Vec<ImplSelfHeadRow>,
    pub docs: Vec<DocRow>,
}

pub fn type_entity_rows(parsed: &syn::File, line_starts: &[u32]) -> TypeEntityRows {
    let mut rows = TypeEntityRows::default();
    collect(&parsed.items, line_starts, &mut rows);
    rows
}

fn collect(items: &[syn::Item], line_starts: &[u32], rows: &mut TypeEntityRows) {
    for item in items {
        match item {
            syn::Item::Struct(item) => {
                rows.entities.push(named(
                    item.ident.span(),
                    item.ident.to_string(),
                    TypeEntityKind::Struct,
                    line_starts,
                ));
                push_doc(rows, item.ident.span(), &item.attrs, None, line_starts);
            }
            syn::Item::Enum(item) => {
                rows.entities.push(named(
                    item.ident.span(),
                    item.ident.to_string(),
                    TypeEntityKind::Enum,
                    line_starts,
                ));
                push_doc(rows, item.ident.span(), &item.attrs, None, line_starts);
            }
            // The existing TypeF vocabulary maps Rust unions onto Struct.
            syn::Item::Union(item) => {
                rows.entities.push(named(
                    item.ident.span(),
                    item.ident.to_string(),
                    TypeEntityKind::Struct,
                    line_starts,
                ));
                push_doc(rows, item.ident.span(), &item.attrs, None, line_starts);
            }
            syn::Item::Type(item) => rows.entities.push(named(
                item.ident.span(),
                item.ident.to_string(),
                TypeEntityKind::Alias,
                line_starts,
            )),
            syn::Item::Trait(item) => {
                rows.entities.push(named(
                    item.ident.span(),
                    item.ident.to_string(),
                    TypeEntityKind::Trait,
                    line_starts,
                ));
                push_doc(rows, item.ident.span(), &item.attrs, None, line_starts);
                for child in &item.items {
                    if let syn::TraitItem::Fn(method) = child {
                        if method.default.is_some() {
                            rows.entities.push(callable(
                                &method.sig,
                                TypeEntityKind::Method,
                                line_starts,
                            ));
                        }
                    }
                }
            }
            syn::Item::Fn(item) => {
                rows.entities
                    .push(callable(&item.sig, TypeEntityKind::Function, line_starts));
                push_doc(rows, item.sig.ident.span(), &item.attrs, None, line_starts);
            }
            syn::Item::Impl(item) => {
                let parent = super::call_metadata_rows::primary_type(&item.self_ty);
                if let syn::Type::Path(self_path) = strip_type(&item.self_ty) {
                    if self_path.qself.is_none() && self_path.path.segments.len() == 1 {
                        if let Some(segment) = self_path.path.segments.first() {
                            rows.impl_self_heads.push(ImplSelfHeadRow {
                                range: span_range(line_starts, segment.ident.span()),
                                name: segment.ident.to_string(),
                            });
                        }
                    }
                }
                for child in &item.items {
                    if let syn::ImplItem::Fn(method) = child {
                        rows.entities.push(callable(
                            &method.sig,
                            TypeEntityKind::Method,
                            line_starts,
                        ));
                        push_doc(
                            rows,
                            method.sig.ident.span(),
                            &method.attrs,
                            parent.as_deref(),
                            line_starts,
                        );
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

fn push_doc(
    rows: &mut TypeEntityRows,
    span: proc_macro2::Span,
    attrs: &[syn::Attribute],
    parent: Option<&str>,
    line_starts: &[u32],
) {
    let lines: Vec<String> = attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc"))
        .filter_map(|attr| match &attr.meta {
            syn::Meta::NameValue(nv) => match &nv.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) => {
                    let value = s.value();
                    Some(value.strip_prefix(' ').unwrap_or(&value).to_string())
                }
                _ => None,
            },
            _ => None,
        })
        .collect();
    if lines.is_empty() {
        return;
    }
    let text = lines.join("\n");
    rows.docs.push(DocRow {
        range: span_range(line_starts, span),
        parent: parent.map(str::to_owned),
        sections: doc_sections(&text),
        text,
    });
}

fn doc_sections(text: &str) -> Vec<DocSectionRow> {
    let mut sections: Vec<(String, Vec<&str>)> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("# ") {
            sections.push((rest.trim().to_string(), Vec::new()));
        } else if let Some((_, body)) = sections.last_mut() {
            body.push(line);
        }
    }
    sections
        .into_iter()
        .map(|(heading, body)| DocSectionRow {
            heading,
            body: body.join("\n").trim().to_string(),
        })
        .collect()
}

/// Peel syntactic wrappers to the referenced Rust type.
pub fn strip_type(ty: &syn::Type) -> &syn::Type {
    match ty {
        syn::Type::Group(t) => strip_type(&t.elem),
        syn::Type::Paren(t) => strip_type(&t.elem),
        syn::Type::Ptr(t) => strip_type(&t.elem),
        syn::Type::Reference(t) => strip_type(&t.elem),
        other => other,
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
