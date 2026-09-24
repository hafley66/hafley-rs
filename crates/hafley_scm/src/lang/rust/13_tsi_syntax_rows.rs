//! Rust TSI syntax facts from the caller's syn parse.
use std::collections::{BTreeMap, BTreeSet, HashMap};
use syn::punctuated::Punctuated;
use syn::{Fields, GenericArgument, GenericParam, Path, PathArguments, ReturnType, Type, TypeParamBound};
use super::call_metadata_rows::{line_col_to_byte, path_name};
use super::type_entity_rows::strip_type;
use super::type_candidate_rows::bare_self_head;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span { pub start: u32, pub len: u32 }
impl Span {
    fn end(self) -> u32 { self.start + self.len }
    fn empty() -> Self { Self { start: 0, len: 0 } }
}
fn syn_span(line_starts: &[u32], span: proc_macro2::Span) -> Span {
    let start = span.start();
    let end = span.end();
    let start = line_col_to_byte(line_starts, start.line as u32, start.column as u32);
    let end = line_col_to_byte(line_starts, end.line as u32, end.column as u32);
    Span { start, len: end.saturating_sub(start) }
}
fn span_arg(span: Span) -> Arg { Arg::Span(String::new(), span.start, span.end()) }
fn self_ty_head(ty: &Type, line_starts: &[u32]) -> Option<(Span, String)> {
    bare_self_head(ty, line_starts).map(|(range, name)| {
        (Span { start: range.start, len: range.end - range.start }, name)
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arg { Id(u32), Span(String, u32, u32), Text(String), Int(i64), Atom(String) }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TsiFactRow { pub fact: u32, pub relation: String, pub args: Vec<Arg> }
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TsiSyntaxRows { pub facts: Vec<TsiFactRow>, pub interned: Vec<String> }

#[derive(Default)]
struct Strings { ordered: Vec<String> }
impl Strings {
    fn intern(&mut self, text: &str) -> String {
        self.ordered.push(text.to_owned());
        text.to_owned()
    }
}
#[derive(Default)]
struct TsiNames { facts: Vec<TsiFactRow>, seen: HashMap<String, u32>, ids: u32 }
impl TsiNames {
    fn named(&mut self, strings: &mut Strings, text: &str, span: Span) -> u32 {
        let key = strings.intern(text);
        if let Some(&id) = self.seen.get(&key) { return id; }
        let id = self.anonymous(span);
        self.seen.insert(key, id);
        self.name(id, text);
        id
    }
    fn name(&mut self, id: u32, text: &str) {
        self.fact("tsi.name", vec![Arg::Id(id), Arg::Text(text.to_owned())]);
    }
    fn anonymous(&mut self, span: Span) -> u32 {
        let id = self.bare_id();
        self.fact("tsi.type", vec![Arg::Id(id)]);
        self.fact("tsi.origin", vec![Arg::Id(id), Arg::Atom("rust".to_owned()), span_arg(span)]);
        id
    }
    fn bare_id(&mut self) -> u32 {
        let id = self.ids;
        self.ids += 1;
        id
    }
    fn fact(&mut self, relation: &'static str, args: Vec<Arg>) {
        self.facts.push(TsiFactRow { fact: self.facts.len() as u32, relation: relation.to_owned(), args });
    }
    fn edge(&mut self, owner: u32, label: &str, target: u32, position: i64) -> u32 {
        let id = self.bare_id();
        self.fact("tsi.edge", vec![Arg::Id(id), Arg::Id(owner), Arg::Text(label.to_owned()), Arg::Id(target), Arg::Int(position)]);
        id
    }
}

// ── TSI syntax rows: `rust.assoc`, `rust.lifetime` and `rust.ownership` are
// the semantic tier's, so an associated item and a reference's mode emit none.

/// Type-parameter names in scope, innermost declaration last.
type TsiScope = BTreeMap<String, u32>;

/// Per-file walk bookkeeping: the ids whose application rows are already
/// written, and the id each primitive class took.
#[derive(Default)]
struct TsiState {
    called: BTreeSet<u32>,
    classes: BTreeMap<&'static str, u32>,
}

/// The type names rust declares itself, which the v7 prelude also declares.
/// `unit` is the empty tuple's class and the one entry with no written name.
const PRIMITIVE_CLASSES: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128", "f32", "f64", "bool",
    "char", "str", "usize", "isize",
];

/// The rust twin of the ts pass, over the items `edge_candidates` walks.
pub fn tsi_syntax_rows(parsed: &syn::File, line_starts: &[u32]) -> TsiSyntaxRows {
    let mut strings = Strings::default();
    let mut names = TsiNames::default();
    let outer = TsiScope::new();
    let mut state = TsiState::default();
    for item in &parsed.items {
        tsi_item(item, &outer, line_starts, &mut strings, &mut names, &mut state);
    }
    TsiSyntaxRows { facts: names.facts, interned: strings.ordered }
}

fn tsi_item(
    item: &syn::Item,
    outer: &TsiScope,
    line_starts: &[u32],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    match item {
        syn::Item::Struct(declared) => {
            let owner = tsi_declaration(&declared.ident, line_starts, strings, names);
            names.fact("tsi.product", vec![Arg::Id(owner)]);
            let scope = tsi_generics(
                owner,
                &declared.generics,
                outer,
                line_starts,
                strings,
                names,
            );
            tsi_fields(
                owner,
                &declared.fields,
                &scope,
                line_starts,
                strings,
                names,
                state,
            );
        }
        syn::Item::Union(declared) => {
            let owner = tsi_declaration(&declared.ident, line_starts, strings, names);
            names.fact("tsi.product", vec![Arg::Id(owner)]);
            let scope = tsi_generics(
                owner,
                &declared.generics,
                outer,
                line_starts,
                strings,
                names,
            );
            let fields = Fields::Named(declared.fields.clone());
            tsi_fields(owner, &fields, &scope, line_starts, strings, names, state);
        }
        syn::Item::Enum(declared) => {
            let owner = tsi_declaration(&declared.ident, line_starts, strings, names);
            names.fact("tsi.sum", vec![Arg::Id(owner)]);
            let scope = tsi_generics(
                owner,
                &declared.generics,
                outer,
                line_starts,
                strings,
                names,
            );
            for (position, variant) in declared.variants.iter().enumerate() {
                let written = format!("{}::{}", declared.ident, variant.ident);
                let span = syn_span(line_starts, variant.ident.span());
                let target = names.named(strings, &written, span);
                names.edge(owner, &variant.ident.to_string(), target, position as i64);
                // A unit variant carries nothing, so it states no shape.
                if !matches!(variant.fields, Fields::Unit) {
                    names.fact("tsi.product", vec![Arg::Id(target)]);
                    tsi_fields(
                        target,
                        &variant.fields,
                        &scope,
                        line_starts,
                        strings,
                        names,
                        state,
                    );
                }
            }
        }
        syn::Item::Trait(declared) => {
            let owner = tsi_declaration(&declared.ident, line_starts, strings, names);
            names.fact("rust.trait", vec![Arg::Id(owner)]);
            let scope = tsi_generics(
                owner,
                &declared.generics,
                outer,
                line_starts,
                strings,
                names,
            );
            for (position, method) in declared
                .items
                .iter()
                .filter_map(|member| match member {
                    syn::TraitItem::Fn(method) => Some(method),
                    _ => None,
                })
                .enumerate()
            {
                let callable =
                    tsi_callable(&method.sig, &scope, line_starts, strings, names, state);
                let label = method.sig.ident.to_string();
                names.edge(owner, &label, callable, position as i64);
            }
        }
        syn::Item::Type(declared) => {
            let owner = tsi_declaration(&declared.ident, line_starts, strings, names);
            let scope = tsi_generics(
                owner,
                &declared.generics,
                outer,
                line_starts,
                strings,
                names,
            );
            tsi_application(
                owner,
                &declared.ty,
                &scope,
                line_starts,
                strings,
                names,
                state,
            );
        }
        syn::Item::Const(declared) => {
            let span = syn_span(line_starts, declared.ident.span());
            tsi_has_type(
                span,
                &declared.ty,
                outer,
                line_starts,
                strings,
                names,
                state,
            );
        }
        syn::Item::Static(declared) => {
            let span = syn_span(line_starts, declared.ident.span());
            tsi_has_type(
                span,
                &declared.ty,
                outer,
                line_starts,
                strings,
                names,
                state,
            );
        }
        syn::Item::Impl(block) => tsi_impl(block, outer, line_starts, strings, names, state),
        syn::Item::Fn(declared) => {
            tsi_callable(&declared.sig, outer, line_starts, strings, names, state);
        }
        syn::Item::Mod(module) => {
            if let Some((_, inner)) = &module.content {
                for nested in inner {
                    tsi_item(nested, outer, line_starts, strings, names, state);
                }
            }
        }
        _ => {}
    }
}

/// A value occurrence and the type written at it. The occurrence is a range,
/// never an id: naming the value's own symbol is the checker's row.
fn tsi_has_type(
    occurrence: Span,
    ty: &Type,
    scope: &TsiScope,
    line_starts: &[u32],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    let target = tsi_type_id(ty, scope, line_starts, strings, names, state);
    names.fact("tsi.has_type", vec![span_arg(occurrence), Arg::Id(target)]);
}

/// A declaration's own id, keyed on its bare name so a later written reference
/// to that name lands on the same id.
fn tsi_declaration(
    ident: &proc_macro2::Ident,
    line_starts: &[u32],
    strings: &mut Strings,
    names: &mut TsiNames,
) -> u32 {
    let span = syn_span(line_starts, ident.span());
    names.named(strings, &ident.to_string(), span)
}

/// `impl Trait for Type` is the one conformance a parse can state. A bare
/// `impl Type` block contributes its methods and no conformance.
fn tsi_impl(
    block: &syn::ItemImpl,
    outer: &TsiScope,
    line_starts: &[u32],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    let Some((self_span, self_name)) = self_ty_head(&block.self_ty, line_starts) else {
        return;
    };
    let owner = names.named(strings, &self_name, self_span);
    // The block's own type parameters are the block's, never the self type's.
    let block_id = names.anonymous(syn_span(line_starts, block.impl_token.span));
    let scope = tsi_generics(
        block_id,
        &block.generics,
        outer,
        line_starts,
        strings,
        names,
    );
    if let Some((_, path, _)) = &block.trait_ {
        if let (Some(name), Some(segment)) = (path_name(path), path.segments.last()) {
            let span = syn_span(line_starts, segment.ident.span());
            let contract = names.named(strings, &name, span);
            names.fact(
                "rust.impl",
                vec![Arg::Id(block_id), Arg::Id(owner), Arg::Id(contract)],
            );
            names.fact(
                "tsi.conforms",
                vec![
                    Arg::Id(owner),
                    Arg::Id(contract),
                    Arg::Atom("syntax".to_string()),
                ],
            );
        }
    }
    // The self type owns the method, never the block: two blocks over one type
    // reach it through one owner.
    for (position, method) in block
        .items
        .iter()
        .filter_map(|member| match member {
            syn::ImplItem::Fn(method) => Some(method),
            _ => None,
        })
        .enumerate()
    {
        let callable = tsi_callable(&method.sig, &scope, line_starts, strings, names, state);
        let label = method.sig.ident.to_string();
        names.edge(owner, &label, callable, position as i64);
    }
}

/// One `tsi.parameter` per declared type parameter plus a `bound`-labelled
/// edge per trait bound. Hands back the scope the declaration's members read.
fn tsi_generics(
    owner: u32,
    generics: &syn::Generics,
    outer: &TsiScope,
    line_starts: &[u32],
    strings: &mut Strings,
    names: &mut TsiNames,
) -> TsiScope {
    let mut scope = outer.clone();
    for (position, param) in generics.params.iter().enumerate() {
        let GenericParam::Type(declared) = param else {
            continue;
        };
        let id = names.anonymous(syn_span(line_starts, declared.ident.span()));
        names.name(id, &declared.ident.to_string());
        names.fact(
            "tsi.parameter",
            vec![
                Arg::Id(id),
                Arg::Id(owner),
                Arg::Int(position as i64),
                Arg::Atom("unspecified".to_string()),
            ],
        );
        for (at, bound) in declared.bounds.iter().enumerate() {
            let TypeParamBound::Trait(traited) = bound else {
                continue;
            };
            let (Some(name), Some(segment)) =
                (path_name(&traited.path), traited.path.segments.last())
            else {
                continue;
            };
            let span = syn_span(line_starts, segment.ident.span());
            let target = names.named(strings, &name, span);
            names.edge(id, "bound", target, at as i64);
        }
        scope.insert(declared.ident.to_string(), id);
    }
    scope
}

/// A tuple field's label is its ordinal, which is the name rust itself gives it.
fn tsi_fields(
    owner: u32,
    fields: &Fields,
    scope: &TsiScope,
    line_starts: &[u32],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    for (position, field) in fields.iter().enumerate() {
        let label = match &field.ident {
            Some(ident) => ident.to_string(),
            None => position.to_string(),
        };
        let target = tsi_type_id(&field.ty, scope, line_starts, strings, names, state);
        names.edge(owner, &label, target, position as i64);
    }
}

/// Hands back the callable's id, which is what an owning type's member edge
/// names. A free fn is ownerless and the id reaches nothing else.
fn tsi_callable(
    signature: &syn::Signature,
    outer: &TsiScope,
    line_starts: &[u32],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) -> u32 {
    let callable = names.anonymous(syn_span(line_starts, signature.ident.span()));
    names.name(callable, &signature.ident.to_string());
    names.fact("tsi.callable", vec![Arg::Id(callable)]);
    let scope = tsi_generics(
        callable,
        &signature.generics,
        outer,
        line_starts,
        strings,
        names,
    );
    // `&self` takes no input slot: the mode it is written in is `rust.ownership`,
    // which only the checker states.
    let mut position = 0i64;
    for input in &signature.inputs {
        let syn::FnArg::Typed(typed) = input else {
            continue;
        };
        let target = tsi_type_id(&typed.ty, &scope, line_starts, strings, names, state);
        names.fact(
            "tsi.input",
            vec![Arg::Id(callable), Arg::Int(position), Arg::Id(target)],
        );
        position += 1;
    }
    if let ReturnType::Type(_, returned) = &signature.output {
        let target = tsi_type_id(returned, &scope, line_starts, strings, names, state);
        names.fact(
            "tsi.output",
            vec![Arg::Id(callable), Arg::Int(0), Arg::Id(target)],
        );
    }
    callable
}

/// The application a written `Name<Args>` states, wherever written: the callee
/// is that path with its arguments dropped. A lifetime argument takes no slot.
fn tsi_application(
    result: u32,
    ty: &Type,
    scope: &TsiScope,
    line_starts: &[u32],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    let Type::Path(path) = strip_type(ty) else {
        return;
    };
    let Some(segment) = path.path.segments.last() else {
        return;
    };
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return;
    };
    if !state.called.insert(result) {
        return;
    }
    let span = syn_span(line_starts, segment.ident.span());
    let callee = names.named(strings, &path_head_text(&path.path), span);
    let list = names.bare_id();
    names.fact(
        "tsi.called",
        vec![Arg::Id(result), Arg::Id(callee), Arg::Id(list)],
    );
    let mut position = 0i64;
    for argument in &arguments.args {
        let GenericArgument::Type(written) = argument else {
            continue;
        };
        let target = tsi_type_id(written, scope, line_starts, strings, names, state);
        names.fact(
            "tsi.argument",
            vec![Arg::Id(list), Arg::Int(position), Arg::Id(target)],
        );
        position += 1;
    }
}

/// One id per written text, except a scoped type parameter (rule 4) and a
/// tuple (rule 2). An array, slice, bare fn, `impl` or `dyn` states no shape.
fn tsi_type_id(
    ty: &Type,
    scope: &TsiScope,
    line_starts: &[u32],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) -> u32 {
    let text = type_text(ty);
    if let Some(&id) = scope.get(&text) {
        return id;
    }
    if let Some(class) = PRIMITIVE_CLASSES.iter().find(|class| **class == text) {
        return tsi_primitive_id(class, names, state);
    }
    if let Type::Tuple(tuple) = strip_type(ty) {
        if tuple.elems.is_empty() {
            return tsi_primitive_id("unit", names, state);
        }
        return tsi_tuple_id(tuple, scope, line_starts, strings, names, state);
    }
    let id = names.named(strings, &text, tsi_type_span(ty, line_starts));
    tsi_application(id, ty, scope, line_starts, strings, names, state);
    id
}

/// A primitive is declared by the language, so it carries a class rather than
/// an origin: no range in this file declares it.
fn tsi_primitive_id(class: &'static str, names: &mut TsiNames, state: &mut TsiState) -> u32 {
    if let Some(&id) = state.classes.get(class) {
        return id;
    }
    let id = names.bare_id();
    names.fact("tsi.type", vec![Arg::Id(id)]);
    names.fact(
        "tsi.primitive",
        vec![Arg::Id(id), Arg::Atom(class.to_string())],
    );
    names.name(id, if class == "unit" { "()" } else { class });
    state.classes.insert(class, id);
    id
}

/// A tuple is structural, so its identity is its ordered edges (rule 2) rather
/// than its text, and every occurrence takes a fresh id.
fn tsi_tuple_id(
    tuple: &syn::TypeTuple,
    scope: &TsiScope,
    line_starts: &[u32],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) -> u32 {
    let id = names.anonymous(syn_span(line_starts, tuple.paren_token.span.join()));
    names.fact("tsi.product", vec![Arg::Id(id)]);
    for (position, element) in tuple.elems.iter().enumerate() {
        let target = tsi_type_id(element, scope, line_starts, strings, names, state);
        names.edge(id, &position.to_string(), target, position as i64);
    }
    id
}

/// The LAST path segment names a written type; the rest qualifies it. `syn` is
/// parsed without printing, so no token stream spans the whole thing.
fn tsi_type_span(ty: &Type, line_starts: &[u32]) -> Span {
    match strip_type(ty) {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| syn_span(line_starts, segment.ident.span()))
            .unwrap_or_else(Span::empty),
        Type::Array(inner) => tsi_type_span(&inner.elem, line_starts),
        Type::Slice(inner) => tsi_type_span(&inner.elem, line_starts),
        _ => Span::empty(),
    }
}

fn path_head_text(path: &Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

/// The written form of a type, rebuilt from the tree. An array length that is
/// not an integer literal renders `_`: the tokens are not reachable.
fn type_text(ty: &Type) -> String {
    match ty {
        Type::Array(inner) => format!(
            "[{}; {}]",
            type_text(&inner.elem),
            array_len_text(&inner.len)
        ),
        Type::BareFn(inner) => {
            let inputs: Vec<String> = inner.inputs.iter().map(|arg| type_text(&arg.ty)).collect();
            match &inner.output {
                ReturnType::Type(_, returned) => {
                    format!("fn({}) -> {}", inputs.join(", "), type_text(returned))
                }
                ReturnType::Default => format!("fn({})", inputs.join(", ")),
            }
        }
        Type::Group(inner) => type_text(&inner.elem),
        Type::ImplTrait(inner) => format!("impl {}", bounds_text(&inner.bounds)),
        Type::Infer(_) => "_".to_string(),
        Type::Never(_) => "!".to_string(),
        Type::Paren(inner) => format!("({})", type_text(&inner.elem)),
        Type::Path(inner) => path_text(&inner.path),
        Type::Ptr(inner) => {
            let mode = if inner.mutability.is_some() {
                "mut "
            } else {
                "const "
            };
            format!("*{mode}{}", type_text(&inner.elem))
        }
        Type::Reference(inner) => {
            let lifetime = inner
                .lifetime
                .as_ref()
                .map_or(String::new(), |name| format!("'{} ", name.ident));
            let mode = if inner.mutability.is_some() {
                "mut "
            } else {
                ""
            };
            format!("&{lifetime}{mode}{}", type_text(&inner.elem))
        }
        Type::Slice(inner) => format!("[{}]", type_text(&inner.elem)),
        Type::TraitObject(inner) => format!("dyn {}", bounds_text(&inner.bounds)),
        Type::Tuple(inner) => {
            let parts: Vec<String> = inner.elems.iter().map(type_text).collect();
            format!("({})", parts.join(", "))
        }
        _ => "_".to_string(),
    }
}

fn array_len_text(len: &syn::Expr) -> String {
    match len {
        syn::Expr::Lit(literal) => match &literal.lit {
            syn::Lit::Int(count) => count.base10_digits().to_string(),
            _ => "_".to_string(),
        },
        _ => "_".to_string(),
    }
}

fn path_text(path: &Path) -> String {
    let mut out = String::new();
    if path.leading_colon.is_some() {
        out.push_str("::");
    }
    for (at, segment) in path.segments.iter().enumerate() {
        if at > 0 {
            out.push_str("::");
        }
        out.push_str(&segment.ident.to_string());
        match &segment.arguments {
            PathArguments::None => {}
            PathArguments::AngleBracketed(arguments) => {
                let rendered: Vec<String> =
                    arguments.args.iter().filter_map(argument_text).collect();
                if !rendered.is_empty() {
                    out.push('<');
                    out.push_str(&rendered.join(", "));
                    out.push('>');
                }
            }
            PathArguments::Parenthesized(arguments) => {
                let inputs: Vec<String> = arguments.inputs.iter().map(type_text).collect();
                out.push('(');
                out.push_str(&inputs.join(", "));
                out.push(')');
                if let ReturnType::Type(_, returned) = &arguments.output {
                    out.push_str(" -> ");
                    out.push_str(&type_text(returned));
                }
            }
        }
    }
    out
}

fn argument_text(argument: &GenericArgument) -> Option<String> {
    match argument {
        GenericArgument::Type(written) => Some(type_text(written)),
        GenericArgument::Lifetime(name) => Some(format!("'{}", name.ident)),
        GenericArgument::AssocType(bound) => {
            Some(format!("{} = {}", bound.ident, type_text(&bound.ty)))
        }
        _ => None,
    }
}

fn bounds_text(bounds: &Punctuated<TypeParamBound, syn::Token![+]>) -> String {
    bounds
        .iter()
        .filter_map(|bound| match bound {
            TypeParamBound::Trait(traited) => Some(path_text(&traited.path)),
            TypeParamBound::Lifetime(name) => Some(format!("'{}", name.ident)),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" + ")
}
