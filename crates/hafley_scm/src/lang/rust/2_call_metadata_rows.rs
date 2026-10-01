//! The Rust CallF metadata rows, name texts, and byte bridge in one walk.

use syn::spanned::Spanned;

/// Byte offset of the start of each 1-based line: line N starts at `out[N-1]`.
pub fn build_line_starts(src: &str) -> Vec<u32> {
    let mut out = vec![0u32];
    for (byte_off, byte) in src.bytes().enumerate() {
        if byte == b'\n' {
            out.push((byte_off + 1) as u32);
        }
    }
    out
}

/// Convert a syn (1-based line, 0-based column) coordinate to a byte offset.
pub fn line_col_to_byte(line_starts: &[u32], line: u32, col: u32) -> u32 {
    line_starts
        .get((line as usize).saturating_sub(1))
        .copied()
        .unwrap_or(0)
        .saturating_add(col)
}

/// Byte range of a proc-macro span under the caller's one line-start table.
pub(super) fn span_range(line_starts: &[u32], span: proc_macro2::Span) -> std::ops::Range<u32> {
    let start = span.start();
    let end = span.end();
    line_col_to_byte(line_starts, start.line as u32, start.column as u32)
        ..line_col_to_byte(line_starts, end.line as u32, end.column as u32)
}

/// A def's byte range over `[start.start, end.end)`, the whole callable body.
pub(super) fn def_range(
    line_starts: &[u32],
    start: proc_macro2::Span,
    end: proc_macro2::Span,
) -> (u32, u32) {
    let start_byte = line_col_to_byte(
        line_starts,
        start.start().line as u32,
        start.start().column as u32,
    );
    let end_byte = line_col_to_byte(line_starts, end.end().line as u32, end.end().column as u32);
    (start_byte, start_byte + end_byte.saturating_sub(start_byte))
}

/// Render a syn::Path as `a::b::c`.
pub fn path_string(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

/// The trailing path name, or None for a primitive / `Self`.
pub fn path_name(path: &syn::Path) -> Option<String> {
    let parts: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    if parts.is_empty() {
        return None;
    }
    let name = parts.join("::");
    (!is_noise_type(&name)).then_some(name)
}

/// Primitive + `Self` filter: a reference to these carries no declaration.
fn is_noise_type(name: &str) -> bool {
    matches!(
        name,
        "Self"
            | "bool"
            | "char"
            | "str"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
    )
}

/// The primary (wrapper-peeled) path name of a type; primitives are None.
pub fn primary_type(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Group(t) => primary_type(&t.elem),
        syn::Type::Paren(t) => primary_type(&t.elem),
        syn::Type::Path(t) => path_name(&t.path),
        syn::Type::Ptr(t) => primary_type(&t.elem),
        syn::Type::Reference(t) => primary_type(&t.elem),
        _ => None,
    }
}

/// One method def's owner: byte range, primary self type, trait path.
pub struct CallOwnerRow {
    pub start: u32,
    pub end: u32,
    pub self_type: Option<String>,
    pub trait_name: Option<String>,
}

/// Walks the file once, emitting owner rows for every impl/trait method in
/// source order.
pub fn call_metadata_rows(parsed: &syn::File, line_starts: &[u32]) -> Vec<CallOwnerRow> {
    let mut out = Vec::new();
    visit(&parsed.items, line_starts, &mut out);
    out
}

fn visit(items: &[syn::Item], line_starts: &[u32], out: &mut Vec<CallOwnerRow>) {
    for item in items {
        match item {
            syn::Item::Impl(item) => {
                let self_type = primary_type(&item.self_ty);
                let trait_name = item.trait_.as_ref().map(|(path, _)| path_string(path));
                for child in &item.items {
                    if let syn::ImplItem::Fn(method) = child {
                        let range =
                            def_range(line_starts, method.sig.ident.span(), method.block.span());
                        push_owner(out, range, self_type.clone(), trait_name.clone());
                    }
                }
            }
            syn::Item::Trait(item) => {
                for child in &item.items {
                    if let syn::TraitItem::Fn(method) = child {
                        let range = method.default.as_ref().map_or_else(
                            || def_range(line_starts, method.sig.ident.span(), method.sig.span()),
                            |body| def_range(line_starts, method.sig.ident.span(), body.span()),
                        );
                        push_owner(out, range, None, Some(item.ident.to_string()));
                    }
                }
            }
            syn::Item::Mod(item) => {
                if let Some((_, inner)) = &item.content {
                    visit(inner, line_starts, out);
                }
            }
            _ => {}
        }
    }
}

fn push_owner(
    out: &mut Vec<CallOwnerRow>,
    range: (u32, u32),
    self_type: Option<String>,
    trait_name: Option<String>,
) {
    out.push(CallOwnerRow {
        start: range.0,
        end: range.1,
        self_type,
        trait_name,
    });
}

/// One variant's def range: the ident alone; None when wider (mbe-expanded).
pub fn variant_def_range(line_starts: &[u32], variant: &syn::Variant) -> Option<(u32, u32)> {
    let range = def_range(line_starts, variant.ident.span(), variant.ident.span());
    (range.1 - range.0 == variant.ident.to_string().len() as u32).then_some(range)
}

/// Attributes of every syn item form that can carry them.
pub fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Fn(i) => &i.attrs,
        syn::Item::Impl(i) => &i.attrs,
        syn::Item::Trait(i) => &i.attrs,
        syn::Item::Mod(i) => &i.attrs,
        syn::Item::Struct(i) => &i.attrs,
        syn::Item::Enum(i) => &i.attrs,
        syn::Item::Const(i) => &i.attrs,
        syn::Item::Static(i) => &i.attrs,
        syn::Item::Type(i) => &i.attrs,
        syn::Item::Use(i) => &i.attrs,
        syn::Item::Macro(i) => &i.attrs,
        syn::Item::TraitAlias(i) => &i.attrs,
        syn::Item::Union(i) => &i.attrs,
        syn::Item::ExternCrate(i) => &i.attrs,
        syn::Item::ForeignMod(i) => &i.attrs,
        _ => &[],
    }
}
