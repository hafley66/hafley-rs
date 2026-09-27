//! Rust TSI syntax facts from the caller's syn parse.
use super::call_metadata_rows::{line_col_to_byte, path_name};
use super::type_candidate_rows::bare_self_head;
use super::type_entity_rows::strip_type;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use syn::punctuated::Punctuated;
use syn::{
    Fields, GenericArgument, GenericParam, Path, PathArguments, ReturnType, Type, TypeParamBound,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub len: u32,
}
impl Span {
    fn end(self) -> u32 {
        self.start + self.len
    }
    fn empty() -> Self {
        Self { start: 0, len: 0 }
    }
}
fn syn_span(line_starts: &[u32], span: proc_macro2::Span) -> Span {
    let start = span.start();
    let end = span.end();
    let start = line_col_to_byte(line_starts, start.line as u32, start.column as u32);
    let end = line_col_to_byte(line_starts, end.line as u32, end.column as u32);
    Span {
        start,
        len: end.saturating_sub(start),
    }
}
fn span_arg(span: Span) -> Arg {
    Arg::Span(String::new(), span.start, span.end())
}
fn self_ty_head(ty: &Type, line_starts: &[u32]) -> Option<(Span, String)> {
    bare_self_head(ty, line_starts).map(|(range, name)| {
        (
            Span {
                start: range.start,
                len: range.end - range.start,
            },
            name,
        )
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arg {
    Id(u32),
    Span(String, u32, u32),
    Text(String),
    Int(i64),
    Atom(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TsiFactRow {
    pub fact: u32,
    pub relation: String,
    pub args: Vec<Arg>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TsiSyntaxRows {
    pub facts: Vec<TsiFactRow>,
    pub interned: Vec<String>,
}

#[derive(Default)]
struct Strings {
    ordered: Vec<String>,
}
impl Strings {
    fn intern(&mut self, text: &str) -> String {
        self.ordered.push(text.to_owned());
        text.to_owned()
    }
}
#[derive(Default)]
struct TsiNames {
    facts: Vec<TsiFactRow>,
    seen: HashMap<String, u32>,
    ids: u32,
}
impl TsiNames {
    fn named(&mut self, strings: &mut Strings, text: &str, span: Span) -> u32 {
        let key = strings.intern(text);
        if let Some(&id) = self.seen.get(&key) {
            return id;
        }
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
        self.fact(
            "tsi.origin",
            vec![Arg::Id(id), Arg::Atom("rust".to_owned()), span_arg(span)],
        );
        id
    }
    fn bare_id(&mut self) -> u32 {
        let id = self.ids;
        self.ids += 1;
        id
    }
    fn fact(&mut self, relation: &'static str, args: Vec<Arg>) {
        self.facts.push(TsiFactRow {
            fact: self.facts.len() as u32,
            relation: relation.to_owned(),
            args,
        });
    }
    fn edge(&mut self, owner: u32, label: &str, target: u32, position: i64) -> u32 {
        let id = self.bare_id();
        self.fact(
            "tsi.edge",
            vec![
                Arg::Id(id),
                Arg::Id(owner),
                Arg::Text(label.to_owned()),
                Arg::Id(target),
                Arg::Int(position),
            ],
        );
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
        tsi_item(
            item,
            &outer,
            line_starts,
            &mut strings,
            &mut names,
            &mut state,
        );
    }
    TsiSyntaxRows {
        facts: names.facts,
        interned: strings.ordered,
    }
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
    if let Some((path, _)) = &block.trait_ {
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
        Type::FnPtr(inner) => {
            let inputs: Vec<String> = inner.inputs.iter().map(|arg| type_text(&arg.ty)).collect();
            match &inner.output {
                ReturnType::Type(_, returned) => {
                    format!(
                        "fn({}) -> {}",
                        inputs.join(", "),
                        type_text(returned.as_ref())
                    )
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
            let mode = match inner.mutability {
                syn::PointerMutability::Mut(_) => "mut ",
                syn::PointerMutability::Const(_) => "const ",
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
                let inputs: Vec<String> = arguments
                    .inputs
                    .iter()
                    .map(|input| type_text(&input.ty))
                    .collect();
                out.push('(');
                out.push_str(&inputs.join(", "));
                out.push(')');
                if let ReturnType::Type(_, returned) = &arguments.output {
                    out.push_str(" -> ");
                    out.push_str(&type_text(returned.as_ref()));
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

/// Rust TSI syntax facts projected from the caller's tree-sitter parse.
pub fn tsi_syntax_rows_from_tree(tree: &tree_sitter::Tree, source: &[u8]) -> TsiSyntaxRows {
    let mut writer = TreeTsi {
        source,
        strings: Strings::default(),
        names: TsiNames::default(),
        state: TsiState::default(),
    };
    writer.items(tree.root_node(), &TsiScope::new());
    TsiSyntaxRows {
        facts: writer.names.facts,
        interned: writer.strings.ordered,
    }
}

struct TreeTsi<'a> {
    source: &'a [u8],
    strings: Strings,
    names: TsiNames,
    state: TsiState,
}

impl TreeTsi<'_> {
    fn declaration(&mut self, name: tree_sitter::Node<'_>) -> u32 {
        self.names.named(
            &mut self.strings,
            tree_text(name, self.source),
            tree_span(name),
        )
    }

    fn items(&mut self, node: tree_sitter::Node<'_>, outer: &TsiScope) {
        for item in tree_children(node) {
            match item.kind() {
                "struct_item" | "union_item" => {
                    let Some(name) = item.child_by_field_name("name") else {
                        continue;
                    };
                    let owner = self.declaration(name);
                    self.names.fact("tsi.product", vec![Arg::Id(owner)]);
                    let scope = self.generics(owner, item, outer);
                    if let Some(fields) = item.child_by_field_name("body") {
                        self.fields(owner, fields, &scope);
                    }
                }
                "enum_item" => {
                    let (Some(name), Some(body)) = (
                        item.child_by_field_name("name"),
                        item.child_by_field_name("body"),
                    ) else {
                        continue;
                    };
                    let owner = self.declaration(name);
                    self.names.fact("tsi.sum", vec![Arg::Id(owner)]);
                    let scope = self.generics(owner, item, outer);
                    for (position, variant) in tree_children(body)
                        .into_iter()
                        .filter(|child| child.kind() == "enum_variant")
                        .enumerate()
                    {
                        let Some(variant_name) = variant.child_by_field_name("name") else {
                            continue;
                        };
                        let written = format!(
                            "{}::{}",
                            tree_text(name, self.source),
                            tree_text(variant_name, self.source)
                        );
                        let target =
                            self.names
                                .named(&mut self.strings, &written, tree_span(variant_name));
                        self.names.edge(
                            owner,
                            tree_text(variant_name, self.source),
                            target,
                            position as i64,
                        );
                        if let Some(fields) = variant
                            .child_by_field_name("body")
                            .or_else(|| variant.child_by_field_name("fields"))
                        {
                            self.names.fact("tsi.product", vec![Arg::Id(target)]);
                            self.fields(target, fields, &scope);
                        }
                    }
                }
                "trait_item" => {
                    let Some(name) = item.child_by_field_name("name") else {
                        continue;
                    };
                    let owner = self.declaration(name);
                    self.names.fact("rust.trait", vec![Arg::Id(owner)]);
                    let scope = self.generics(owner, item, outer);
                    if let Some(body) = item.child_by_field_name("body") {
                        for (position, method) in tree_children(body)
                            .into_iter()
                            .filter(|child| {
                                matches!(child.kind(), "function_item" | "function_signature_item")
                            })
                            .enumerate()
                        {
                            let Some(method_name) = method.child_by_field_name("name") else {
                                continue;
                            };
                            let callable = self.callable(&method, &scope);
                            self.names.edge(
                                owner,
                                tree_text(method_name, self.source),
                                callable,
                                position as i64,
                            );
                        }
                    }
                }
                "type_item" => {
                    let (Some(name), Some(ty)) = (
                        item.child_by_field_name("name"),
                        item.child_by_field_name("type"),
                    ) else {
                        continue;
                    };
                    let owner = self.declaration(name);
                    let scope = self.generics(owner, item, outer);
                    self.application(owner, ty, &scope);
                }
                "const_item" | "static_item" => {
                    if let (Some(name), Some(ty)) = (
                        item.child_by_field_name("name"),
                        item.child_by_field_name("type"),
                    ) {
                        self.has_type(tree_span(name), ty, outer);
                    }
                }
                "impl_item" => self.impl_item(item, outer),
                "function_item" => {
                    self.callable(&item, outer);
                }
                "mod_item" => {
                    if let Some(body) = item.child_by_field_name("body") {
                        self.items(body, outer);
                    }
                }
                _ => {}
            }
        }
    }

    fn impl_item(&mut self, item: tree_sitter::Node<'_>, outer: &TsiScope) {
        let (Some(self_type), Some(body)) = (
            item.child_by_field_name("type"),
            item.child_by_field_name("body"),
        ) else {
            return;
        };
        let Some((self_span, self_name)) = tree_bare_self_head(self_type, self.source) else {
            return;
        };
        let owner = self.names.named(&mut self.strings, &self_name, self_span);
        let block_id = self.names.anonymous(Span {
            start: item.start_byte() as u32,
            len: 4,
        });
        let scope = self.generics(block_id, item, outer);
        if let Some(trait_ty) = item.child_by_field_name("trait") {
            if let Some((contract_name, contract_span)) = tree_path_name(trait_ty, self.source) {
                let contract = self
                    .names
                    .named(&mut self.strings, &contract_name, contract_span);
                self.names.fact(
                    "rust.impl",
                    vec![Arg::Id(block_id), Arg::Id(owner), Arg::Id(contract)],
                );
                self.names.fact(
                    "tsi.conforms",
                    vec![
                        Arg::Id(owner),
                        Arg::Id(contract),
                        Arg::Atom("syntax".to_owned()),
                    ],
                );
            }
        }
        for (position, method) in tree_children(body)
            .into_iter()
            .filter(|child| child.kind() == "function_item")
            .enumerate()
        {
            let Some(name) = method.child_by_field_name("name") else {
                continue;
            };
            let callable = self.callable(&method, &scope);
            self.names.edge(
                owner,
                tree_text(name, self.source),
                callable,
                position as i64,
            );
        }
    }

    fn generics(
        &mut self,
        owner: u32,
        declaration: tree_sitter::Node<'_>,
        outer: &TsiScope,
    ) -> TsiScope {
        let mut scope = outer.clone();
        let Some(params) = declaration.child_by_field_name("type_parameters") else {
            return scope;
        };
        for (position, param) in tree_children(params).into_iter().enumerate() {
            if param.kind() != "type_parameter" {
                continue;
            }
            let Some(name) = param.child_by_field_name("name") else {
                continue;
            };
            let id = self.names.anonymous(tree_span(name));
            let text = tree_text(name, self.source).to_owned();
            self.names.name(id, &text);
            self.names.fact(
                "tsi.parameter",
                vec![
                    Arg::Id(id),
                    Arg::Id(owner),
                    Arg::Int(position as i64),
                    Arg::Atom("unspecified".to_owned()),
                ],
            );
            if let Some(bounds) = param.child_by_field_name("bounds") {
                for (at, bound) in tree_children(bounds).into_iter().enumerate() {
                    let Some((bound_name, span)) = tree_path_name(bound, self.source) else {
                        continue;
                    };
                    let target = self.names.named(&mut self.strings, &bound_name, span);
                    self.names.edge(id, "bound", target, at as i64);
                }
            }
            scope.insert(text, id);
        }
        scope
    }

    fn fields(&mut self, owner: u32, fields: tree_sitter::Node<'_>, scope: &TsiScope) {
        let mut position = 0usize;
        for field in tree_children(fields) {
            let (ty, label) = if matches!(field.kind(), "field_declaration" | "tuple_field") {
                let Some(ty) = field.child_by_field_name("type") else {
                    continue;
                };
                let label = field
                    .child_by_field_name("name")
                    .map(|name| tree_text(name, self.source).to_owned())
                    .unwrap_or_else(|| position.to_string());
                (ty, label)
            } else if is_tree_type(field.kind()) {
                (field, position.to_string())
            } else {
                continue;
            };
            let target = self.type_id(ty, scope);
            self.names.edge(owner, &label, target, position as i64);
            position += 1;
        }
    }

    fn callable(&mut self, function: &tree_sitter::Node<'_>, outer: &TsiScope) -> u32 {
        let Some(name) = function.child_by_field_name("name") else {
            return self.names.anonymous(tree_span(*function));
        };
        let callable = self.names.anonymous(tree_span(name));
        let text = tree_text(name, self.source).to_owned();
        self.names.name(callable, &text);
        self.names.fact("tsi.callable", vec![Arg::Id(callable)]);
        let scope = self.generics(callable, *function, outer);
        if let Some(parameters) = function.child_by_field_name("parameters") {
            let mut position = 0i64;
            for parameter in tree_children(parameters) {
                if parameter.kind() != "parameter" {
                    continue;
                }
                if parameter
                    .child_by_field_name("pattern")
                    .is_some_and(|pattern| tree_text(pattern, self.source).trim() == "self")
                {
                    continue;
                }
                let Some(ty) = parameter.child_by_field_name("type") else {
                    continue;
                };
                let target = self.type_id(ty, &scope);
                self.names.fact(
                    "tsi.input",
                    vec![Arg::Id(callable), Arg::Int(position), Arg::Id(target)],
                );
                position += 1;
            }
        }
        if let Some(returned) = function.child_by_field_name("return_type") {
            let target = self.type_id(returned, &scope);
            self.names.fact(
                "tsi.output",
                vec![Arg::Id(callable), Arg::Int(0), Arg::Id(target)],
            );
        }
        callable
    }

    fn has_type(&mut self, occurrence: Span, ty: tree_sitter::Node<'_>, scope: &TsiScope) {
        let target = self.type_id(ty, scope);
        self.names
            .fact("tsi.has_type", vec![span_arg(occurrence), Arg::Id(target)]);
    }

    fn application(&mut self, result: u32, ty: tree_sitter::Node<'_>, scope: &TsiScope) {
        let Some((callee_name, callee_span, args)) = tree_generic_path(ty, self.source) else {
            return;
        };
        if !self.state.called.insert(result) {
            return;
        }
        let callee = self
            .names
            .named(&mut self.strings, &callee_name, callee_span);
        let list = self.names.bare_id();
        self.names.fact(
            "tsi.called",
            vec![Arg::Id(result), Arg::Id(callee), Arg::Id(list)],
        );
        let mut position = 0i64;
        for argument in tree_children(args) {
            if !is_tree_type(argument.kind()) {
                continue;
            }
            let target = self.type_id(argument, scope);
            self.names.fact(
                "tsi.argument",
                vec![Arg::Id(list), Arg::Int(position), Arg::Id(target)],
            );
            position += 1;
        }
    }

    fn type_id(&mut self, ty: tree_sitter::Node<'_>, scope: &TsiScope) -> u32 {
        let text = tree_type_text(ty, self.source);
        if let Some(&id) = scope.get(&text) {
            return id;
        }
        if let Some(class) = PRIMITIVE_CLASSES.iter().find(|class| **class == text) {
            return tsi_primitive_id(class, &mut self.names, &mut self.state);
        }
        if ty.kind() == "unit_type" {
            return tsi_primitive_id("unit", &mut self.names, &mut self.state);
        }
        if ty.kind() == "tuple_type" {
            let elements = tree_children(ty)
                .into_iter()
                .filter(|child| is_tree_type(child.kind()))
                .collect::<Vec<_>>();
            if elements.is_empty() {
                return tsi_primitive_id("unit", &mut self.names, &mut self.state);
            }
            let id = self.names.anonymous(tree_span(ty));
            self.names.fact("tsi.product", vec![Arg::Id(id)]);
            for (position, element) in elements.into_iter().enumerate() {
                let target = self.type_id(element, scope);
                self.names
                    .edge(id, &position.to_string(), target, position as i64);
            }
            return id;
        }
        let span = tree_type_span(ty, self.source).unwrap_or_else(Span::empty);
        let id = self.names.named(&mut self.strings, &text, span);
        self.application(id, ty, scope);
        id
    }
}

fn tree_children(node: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn tree_text<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> &'a str {
    std::str::from_utf8(&source[node.byte_range()]).expect("Rust source is UTF-8")
}

fn tree_span(node: tree_sitter::Node<'_>) -> Span {
    Span {
        start: node.start_byte() as u32,
        len: (node.end_byte() - node.start_byte()) as u32,
    }
}

fn is_tree_type(kind: &str) -> bool {
    matches!(
        kind,
        "type_identifier"
            | "primitive_type"
            | "scoped_type_identifier"
            | "generic_type"
            | "reference_type"
            | "pointer_type"
            | "parenthesized_type"
            | "tuple_type"
            | "array_type"
            | "slice_type"
            | "function_type"
            | "trait_object"
            | "dynamic_type"
            | "abstract_type"
            | "unit_type"
            | "never_type"
            | "generic_type_with_turbofish"
    )
}

fn tree_path_name(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<(String, Span)> {
    if !matches!(
        node.kind(),
        "type_identifier" | "primitive_type" | "scoped_type_identifier" | "generic_type"
    ) {
        return None;
    }
    let head = if node.kind() == "generic_type" {
        node.child_by_field_name("type")?
    } else {
        node
    };
    let name_node = if matches!(head.kind(), "scoped_type_identifier") {
        head.child_by_field_name("name").unwrap_or(head)
    } else {
        head
    };
    let name = tree_type_path_text(head, source);
    Some((name, tree_span(name_node)))
}

fn tree_type_path_text(node: tree_sitter::Node<'_>, source: &[u8]) -> String {
    let written = tree_text(node, source);
    if let Some((qualified, tail)) = written.rsplit_once(">::") {
        let qualified = qualified.strip_prefix('<').unwrap_or(qualified);
        let principal = qualified
            .split_once(" as ")
            .map(|(_, principal)| principal)
            .unwrap_or("");
        if !principal.is_empty() {
            return format!("{principal}::{tail}");
        }
        return tail.to_owned();
    }
    written
        .split("::")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("::")
}

fn tree_bare_self_head(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<(Span, String)> {
    let head = if node.kind() == "generic_type" {
        node.child_by_field_name("type")?
    } else {
        node
    };
    (head.kind() == "type_identifier")
        .then(|| (tree_span(head), tree_text(head, source).to_owned()))
}

fn tree_generic_path<'tree>(
    node: tree_sitter::Node<'tree>,
    source: &[u8],
) -> Option<(String, Span, tree_sitter::Node<'tree>)> {
    let mut generic = node;
    while matches!(
        generic.kind(),
        "reference_type" | "pointer_type" | "parenthesized_type"
    ) {
        generic = tree_children(generic)
            .into_iter()
            .find(|child| is_tree_type(child.kind()))?;
    }
    if generic.kind() == "generic_type_with_turbofish" {
        generic = generic.child_by_field_name("type")?;
    }
    if generic.kind() != "generic_type" {
        return None;
    }
    let head = generic.child_by_field_name("type")?;
    let (name, span) = tree_path_name(head, source)?;
    Some((name, span, generic.child_by_field_name("type_arguments")?))
}

fn tree_type_span(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<Span> {
    match node.kind() {
        "type_identifier" | "primitive_type" | "scoped_type_identifier" => {
            tree_path_name(node, source).map(|(_, span)| span)
        }
        "generic_type" => node
            .child_by_field_name("type")
            .and_then(|head| tree_type_span(head, source)),
        "array_type" | "slice_type" | "reference_type" | "pointer_type" | "parenthesized_type" => {
            tree_children(node)
                .into_iter()
                .find(|child| is_tree_type(child.kind()))
                .and_then(|child| tree_type_span(child, source))
        }
        _ => None,
    }
}

fn tree_type_text(node: tree_sitter::Node<'_>, source: &[u8]) -> String {
    let children = tree_children(node);
    match node.kind() {
        "type_identifier" | "primitive_type" | "scoped_type_identifier" => {
            tree_type_path_text(node, source)
        }
        "generic_type" | "generic_type_with_turbofish" => {
            let generic = if node.kind() == "generic_type_with_turbofish" {
                node.child_by_field_name("type").unwrap_or(node)
            } else {
                node
            };
            let Some(head) = generic.child_by_field_name("type") else {
                return "_".to_owned();
            };
            let mut path = tree_type_text(head, source);
            if let Some(args) = generic.child_by_field_name("type_arguments") {
                let rendered = tree_children(args)
                    .into_iter()
                    .filter_map(|arg| tree_argument_text(arg, source))
                    .collect::<Vec<_>>();
                if !rendered.is_empty() {
                    path.push('<');
                    path.push_str(&rendered.join(", "));
                    path.push('>');
                }
            }
            path
        }
        "reference_type" => {
            let lifetime = children
                .iter()
                .find(|child| child.kind() == "lifetime")
                .map_or(String::new(), |child| {
                    format!("{} ", tree_text(*child, source))
                });
            let mode = children
                .iter()
                .any(|child| child.kind() == "mutable_specifier");
            let ty = children
                .iter()
                .find(|child| is_tree_type(child.kind()))
                .map_or_else(|| "_".to_owned(), |child| tree_type_text(*child, source));
            format!("&{lifetime}{}{ty}", if mode { "mut " } else { "" })
        }
        "pointer_type" => {
            let mode = children
                .iter()
                .any(|child| child.kind() == "mutable_specifier");
            let ty = children
                .iter()
                .find(|child| is_tree_type(child.kind()))
                .map_or_else(|| "_".to_owned(), |child| tree_type_text(*child, source));
            format!("*{}{ty}", if mode { "mut " } else { "const " })
        }
        "parenthesized_type" => children
            .iter()
            .find(|child| is_tree_type(child.kind()))
            .map_or_else(
                || "(_)".to_owned(),
                |child| format!("({})", tree_type_text(*child, source)),
            ),
        "tuple_type" => {
            let parts = children
                .iter()
                .filter(|child| is_tree_type(child.kind()))
                .map(|child| tree_type_text(*child, source))
                .collect::<Vec<_>>();
            if parts.len() == 1 && tree_text(node, source).trim_end().ends_with(",)") {
                format!("({},)", parts[0])
            } else {
                format!("({})", parts.join(", "))
            }
        }
        "unit_type" => "()".to_owned(),
        "never_type" => "!".to_owned(),
        "array_type" => {
            let ty = children
                .iter()
                .find(|child| is_tree_type(child.kind()))
                .map_or_else(|| "_".to_owned(), |child| tree_type_text(*child, source));
            match node.child_by_field_name("length") {
                Some(length) => {
                    let value = tree_text(length, source).trim();
                    let length = if value.chars().all(|c| c.is_ascii_digit()) {
                        value
                    } else {
                        "_"
                    };
                    format!("[{ty}; {length}]")
                }
                None => format!("[{ty}]"),
            }
        }
        "slice_type" => {
            let ty = children
                .iter()
                .find(|child| is_tree_type(child.kind()))
                .map_or_else(|| "_".to_owned(), |child| tree_type_text(*child, source));
            format!("[{ty}]")
        }
        "function_type" => {
            let trait_name = node
                .child_by_field_name("trait")
                .map(|trait_name| tree_type_text(trait_name, source))
                .unwrap_or_else(|| "fn".to_owned());
            let inputs = node
                .child_by_field_name("parameters")
                .into_iter()
                .flat_map(tree_children)
                .filter(|parameter| is_tree_type(parameter.kind()))
                .map(|ty| tree_type_text(ty, source))
                .collect::<Vec<_>>();
            let ret = node
                .child_by_field_name("return_type")
                .map(|ret| format!(" -> {}", tree_type_text(ret, source)))
                .unwrap_or_default();
            format!("{trait_name}({}){ret}", inputs.join(", "))
        }
        "abstract_type" => node.child_by_field_name("trait").map_or_else(
            || "impl _".to_owned(),
            |bound| format!("impl {}", tree_type_text(bound, source)),
        ),
        "dynamic_type" | "trait_object" => {
            let bounds = children
                .iter()
                .filter(|child| child.kind() == "trait_bounds")
                .flat_map(|bounds| tree_children(*bounds))
                .map(|bound| tree_type_text(bound, source))
                .collect::<Vec<_>>();
            let trait_path = node
                .child_by_field_name("trait")
                .map(|path| tree_type_text(path, source));
            let bounds = trait_path.into_iter().chain(bounds).collect::<Vec<_>>();
            format!("dyn {}", bounds.join(" + "))
        }
        "impl_trait" => {
            let bounds = children
                .iter()
                .filter(|child| child.kind() == "trait_bounds")
                .flat_map(|bounds| tree_children(*bounds))
                .map(|bound| tree_type_text(bound, source))
                .collect::<Vec<_>>();
            format!("impl {}", bounds.join(" + "))
        }
        "trait_bounds" => children
            .iter()
            .map(|bound| tree_type_text(*bound, source))
            .collect::<Vec<_>>()
            .join(" + "),
        "lifetime" => tree_text(node, source).to_owned(),
        _ => "_".to_owned(),
    }
}

fn tree_argument_text(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    match node.kind() {
        "lifetime" => Some(tree_type_text(node, source)),
        "associated_type" | "type_binding" => {
            let (Some(name), Some(ty)) = (
                node.child_by_field_name("name"),
                node.child_by_field_name("type"),
            ) else {
                return None;
            };
            Some(format!(
                "{} = {}",
                tree_text(name, source),
                tree_type_text(ty, source)
            ))
        }
        kind if is_tree_type(kind) => Some(tree_type_text(node, source)),
        _ => None,
    }
}

#[cfg(test)]
mod tree_projection_tests {
    use super::{tsi_syntax_rows, tsi_syntax_rows_from_tree};
    use std::path::{Path, PathBuf};

    #[test]
    fn tree_tsi_rows_match_syn_on_rust_shapes() {
        let source = r#"
            struct Pair<T: Clone, U>(T, Option<&'static U>);
            struct Named { left: i32, right: Vec<String> }
            enum Choice<T> { Empty, One(T), Pair(i32, bool), Named { value: T } }
            trait Convert<T> { fn convert(&self, value: T) -> Result<Self, Error> where Self: Sized; }
            type Alias<T> = Option<Vec<T>>;
            const LIMIT: usize = 12;
            static LABEL: &'static str = "x";
            impl<T> Convert<T> for Pair<T, String> {
                fn convert(&self, value: T) -> Result<Self, Error> { todo!() }
            }
            fn choose<'a, T: Clone>(input: &'a [T], callback: fn(T) -> bool) -> &'a T { &input[0] }
            mod nested { pub struct Inner(pub (u8,)); }
        "#;
        let parsed = syn::parse_file(source).expect("valid Rust fixture");
        let line_starts = source
            .split_inclusive('\n')
            .scan(0u32, |at, line| {
                let start = *at;
                *at += line.len() as u32;
                Some(start)
            })
            .collect::<Vec<_>>();
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter::Language::new(tree_sitter_rust::LANGUAGE))
            .expect("Rust grammar");
        let tree = parser.parse(source, None).expect("tree-sitter parse");

        let syn_rows = tsi_syntax_rows(&parsed, &line_starts);
        let tree_rows = tsi_syntax_rows_from_tree(&tree, source.as_bytes());
        let interned_mismatch = tree_rows
            .interned
            .iter()
            .zip(&syn_rows.interned)
            .position(|(tree, syn)| tree != syn);
        assert!(
            interned_mismatch.is_none() && tree_rows.interned.len() == syn_rows.interned.len(),
            "shape fixture interned mismatch at {:?}: tree={:?}, syn={:?}; lengths {} vs {}",
            interned_mismatch,
            interned_mismatch.and_then(|at| tree_rows.interned.get(at)),
            interned_mismatch.and_then(|at| syn_rows.interned.get(at)),
            tree_rows.interned.len(),
            syn_rows.interned.len()
        );
        let fact_mismatch = tree_rows
            .facts
            .iter()
            .zip(&syn_rows.facts)
            .position(|(tree, syn)| tree != syn);
        assert!(
            fact_mismatch.is_none() && tree_rows.facts.len() == syn_rows.facts.len(),
            "shape fixture fact mismatch at {:?}: tree={:?}, syn={:?}; lengths {} vs {}",
            fact_mismatch,
            fact_mismatch.and_then(|at| tree_rows.facts.get(at)),
            fact_mismatch.and_then(|at| syn_rows.facts.get(at)),
            tree_rows.facts.len(),
            syn_rows.facts.len()
        );
    }

    #[test]
    fn tree_tsi_rows_match_syn_on_pinned_rust_fixtures() {
        let fixtures =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sprefa-extract/tests/fixtures");
        let roots = ["type_ladder", "type_ladder_scope", "ratchet_soopy"];
        let mut files = Vec::new();
        for root in roots {
            rust_files(&fixtures.join(root), &mut files);
        }
        files.sort();
        assert!(!files.is_empty());
        for path in files {
            let source = std::fs::read(&path).expect("fixture source");
            let source_text = std::str::from_utf8(&source).expect("UTF-8 fixture");
            let parsed = syn::parse_file(source_text)
                .unwrap_or_else(|error| panic!("Syn parse failed for {}: {error}", path.display()));
            let line_starts = source_text
                .split_inclusive('\n')
                .scan(0u32, |at, line| {
                    let start = *at;
                    *at += line.len() as u32;
                    Some(start)
                })
                .collect::<Vec<_>>();
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&tree_sitter::Language::new(tree_sitter_rust::LANGUAGE))
                .expect("Rust grammar");
            let tree = parser.parse(&source, None).expect("tree-sitter parse");
            let tree_rows = tsi_syntax_rows_from_tree(&tree, &source);
            let syn_rows = tsi_syntax_rows(&parsed, &line_starts);
            let interned_mismatch = tree_rows
                .interned
                .iter()
                .zip(&syn_rows.interned)
                .position(|(tree, syn)| tree != syn);
            if let Some(at) = interned_mismatch {
                let start = at.saturating_sub(3);
                let tree_end = (at + 4).min(tree_rows.interned.len());
                let syn_end = (at + 4).min(syn_rows.interned.len());
                eprintln!(
                    "interned window {} tree={:?} syn={:?}",
                    start,
                    &tree_rows.interned[start..tree_end],
                    &syn_rows.interned[start..syn_end]
                );
            }
            assert!(
                interned_mismatch.is_none() && tree_rows.interned.len() == syn_rows.interned.len(),
                "interned names differ for {} at {:?}: tree={:?}, syn={:?}; lengths {} vs {}",
                path.display(),
                interned_mismatch,
                interned_mismatch.and_then(|at| tree_rows.interned.get(at)),
                interned_mismatch.and_then(|at| syn_rows.interned.get(at)),
                tree_rows.interned.len(),
                syn_rows.interned.len()
            );
            let fact_mismatch = tree_rows
                .facts
                .iter()
                .zip(&syn_rows.facts)
                .position(|(tree, syn)| tree != syn);
            assert_eq!(
                tree_rows.facts.len(),
                syn_rows.facts.len(),
                "fact count differs for {}",
                path.display()
            );
            assert_eq!(
                fact_mismatch,
                None,
                "first fact mismatch for {}: tree={:?}, syn={:?}",
                path.display(),
                fact_mismatch.and_then(|at| tree_rows.facts.get(at)),
                fact_mismatch.and_then(|at| syn_rows.facts.get(at))
            );
        }
    }

    fn rust_files(path: &Path, files: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                rust_files(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }
}
