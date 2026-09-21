//! The TypeScript arm of `ryi cleave`: the only file in the verb that names a
//! TypeScript node kind, a `./` specifier, an `export` keyword or a `.ts`
//! extension. The planner in `0_cleave.rs` asks through `Cleave` and never
//! reads source syntax itself.
//! @comment-ok: module header, the seam list every lang arm opens with
//!
//! The file read is a projection of the crate's OWN cst fact plane: `dispatch`
//! + `flatten_each` over the `cst` and `call` families, which is the uniform
//! surface every other arm already produces. There is no hand-written oxc walk
//! here to replace with a `.scm` query, and a query would have to re-parse the
//! file the fact plane already parsed once.

use sprefa_extract::types::{Cleave, CleaveDecl, CleaveImport, CleaveView};
use sprefa_extract::{
    dispatch, flatten_each, join_rel, relative_between, FamilyMask, FamilyTag, FlatFact, Span,
};

/// The roster. `cleave_for` answers None for every path no arm owns, which the
/// verb turns into the out-of-scope message.
pub const CLEAVES: [&dyn Cleave; 1] = [&TsCleave];

/// Whether any arm owns `rel`, and which.
pub fn cleave_for(rel: &str) -> Option<&'static dyn Cleave> {
    CLEAVES.iter().copied().find(|arm| arm.owns(rel))
}

pub struct TsCleave;

/// The extensions this arm owns.
const EXTENSIONS: [&str; 6] = ["ts", "tsx", "mts", "cts", "js", "mjs"];

/// cst kinds that carry a top-level declaration's name.
const DECL_KINDS: [&str; 8] = [
    "function_declaration",
    "generator_function_declaration",
    "class_declaration",
    "abstract_class_declaration",
    "interface_declaration",
    "type_alias_declaration",
    "enum_declaration",
    "variable_declarator",
];

/// cst kinds that bind a name inside a declaration.
const BINDER_KINDS: [&str; 3] = [
    "required_parameter",
    "optional_parameter",
    "variable_declarator",
];

/// cst kinds that are an identifier occurrence. `property_identifier` is not
/// one: `a.join` names a member, never the `join` an import bound.
const USE_KINDS: [&str; 3] = [
    "identifier",
    "type_identifier",
    "shorthand_property_identifier",
];

/// Globals no import carries. A free name among them is graded, not missing.
const BUILTINS: [&str; 30] = [
    "Array", "BigInt", "Boolean", "Date", "Error", "Infinity", "JSON", "Map", "Math", "NaN",
    "Number", "Object", "Promise", "Proxy", "Reflect", "RegExp", "Set", "String", "Symbol",
    "WeakMap", "WeakSet", "console", "globalThis", "module", "process", "require", "undefined",
    "this", "super", "arguments",
];

impl Cleave for TsCleave {
    fn name(&self) -> &'static str {
        "ts"
    }

    fn owns(&self, rel: &str) -> bool {
        rel.rsplit_once('.')
            .is_some_and(|(_, extension)| EXTENSIONS.contains(&extension))
    }

    fn view(&self, rel: &str, text: &str) -> Option<CleaveView> {
        let mask = FamilyMask {
            cst: true,
            call: true,
            ..FamilyMask::NONE
        };
        let out = dispatch(rel, text.as_bytes(), mask)?;
        let mut facts = Vec::new();
        flatten_each(&out, None, &mut |fact: FlatFact| -> Result<(), ()> {
            facts.push(fact);
            Ok(())
        })
        .ok()?;
        Some(project(text, &facts))
    }

    fn export_prefix(&self) -> &'static str {
        "export "
    }

    fn import_line(&self, names: &[String], module: &str, quote: char) -> String {
        format!(
            "import {{ {} }} from {quote}{module}{quote};\n",
            names.join(", ")
        )
    }

    fn is_relative(&self, module: &str) -> bool {
        module.starts_with('.')
    }

    fn spell_module(&self, from_dir: &str, target: &str) -> String {
        let target = drop_extension(target);
        let relative = relative_between(from_dir, &target);
        match relative.is_empty() {
            true => ".".to_string(),
            false if relative.starts_with("..") => relative,
            false => format!("./{relative}"),
        }
    }

    fn aims_at(&self, from_dir: &str, module: &str, target: &str) -> bool {
        if !module.starts_with('.') {
            return false;
        }
        let written = join_rel(from_dir, module);
        let target = drop_extension(target);
        written == target || written == format!("{target}/index")
    }

    fn specifier_cut(&self, import: &CleaveImport, index: usize) -> Span {
        let (_, span) = import.names[index];
        if let Some((_, next)) = import.names.get(index + 1) {
            return Span {
                start: span.start,
                len: next.start - span.start,
            };
        }
        match index.checked_sub(1).and_then(|at| import.names.get(at)) {
            Some((_, previous)) => Span {
                start: previous.end(),
                len: span.end() - previous.end(),
            },
            None => span,
        }
    }
}

/// The cst plane folded into the planner's view. Top-level statements come
/// from the `program` node's child edges, so nesting is never guessed.
fn project(text: &str, facts: &[FlatFact]) -> CleaveView {
    let mut nodes: Vec<(&str, Option<&str>, Span)> = Vec::new();
    let mut program = Span::empty();
    let mut children: Vec<Span> = Vec::new();
    let mut specifiers: Vec<(String, Span)> = Vec::new();
    let mut calls: Vec<(String, Span, bool)> = Vec::new();
    for fact in facts {
        match fact {
            FlatFact::Node {
                family: FamilyTag::Cst,
                span,
                kind,
                name,
                ..
            } => {
                let span = span_of(span);
                if kind == "program" {
                    program = span;
                }
                nodes.push((kind.as_str(), name.as_deref(), span));
            }
            FlatFact::Edge {
                family: FamilyTag::Cst,
                kind,
                from,
                to,
                ..
            } if kind == "child" && from.start == program.start && from.end == program.end() => {
                children.push(span_of(to));
            }
            FlatFact::Specifier { span, name, .. } => {
                specifiers.push((name.clone(), span_of(span)));
            }
            FlatFact::Site {
                span,
                callee,
                callee_path,
                ..
            } => calls.push((callee.clone(), span_of(span), callee_path.is_some())),
            _ => {}
        }
    }
    children.sort_by_key(|span| span.start);

    let mut imports: Vec<CleaveImport> = Vec::new();
    let mut decls: Vec<CleaveDecl> = Vec::new();
    for child in &children {
        let child = line_span(text, *child);
        let held: Vec<&(&str, Option<&str>, Span)> = nodes
            .iter()
            .filter(|(_, _, span)| inside(*span, child))
            .collect();
        if held.iter().any(|(kind, _, _)| *kind == "import_statement") {
            let module_span = held
                .iter()
                .find(|(kind, _, _)| *kind == "string")
                .map(|(_, _, span)| *span)
                .unwrap_or(child);
            imports.push(CleaveImport {
                span: child,
                module: bare(slice(text, module_span)).to_string(),
                module_span,
                names: specifiers
                    .iter()
                    .filter(|(_, span)| inside(*span, child))
                    .cloned()
                    .collect(),
            });
            continue;
        }
        let Some((_, Some(name), _)) = held
            .iter()
            .find(|(kind, name, _)| DECL_KINDS.contains(kind) && name.is_some())
        else {
            continue;
        };
        decls.push(CleaveDecl {
            name: (*name).to_string(),
            span: child,
            exported: held
                .iter()
                .any(|(kind, _, span)| *kind == "export_statement" && span.start == child.start),
        });
    }

    let identifiers: Vec<(&str, Span)> = nodes
        .iter()
        .filter(|(kind, name, _)| USE_KINDS.contains(kind) && name.is_some())
        .map(|(_, name, span)| (name.unwrap(), *span))
        .collect();
    let leftmost = |scope: Span| -> Option<(&str, Span)> {
        identifiers
            .iter()
            .filter(|(_, span)| inside(*span, scope))
            .min_by_key(|(_, span)| span.start)
            .copied()
    };
    let mut bindings: Vec<(String, Span)> = Vec::new();
    for (kind, _, span) in &nodes {
        if !BINDER_KINDS.contains(kind) {
            continue;
        }
        if let Some((name, at)) = leftmost(*span) {
            bindings.push((name.to_string(), at));
        }
    }
    let declaring: Vec<u32> = decls
        .iter()
        .filter_map(|decl| leftmost(decl.span).filter(|(name, _)| *name == decl.name))
        .map(|(_, at)| at.start)
        .collect();
    let uses = identifiers
        .iter()
        .filter(|(_, span)| !imports.iter().any(|row| inside(*span, row.span)))
        .filter(|(_, span)| !declaring.contains(&span.start))
        .map(|(name, span)| ((*name).to_string(), *span))
        .collect();

    CleaveView {
        imports,
        decls,
        uses,
        bindings,
        calls,
        builtins: BUILTINS.to_vec(),
    }
}

fn span_of(span: &sprefa_extract::SpanOut) -> Span {
    Span {
        start: span.start,
        len: span.end - span.start,
    }
}

fn slice(text: &str, span: Span) -> &str {
    text.get(span.start as usize..span.end() as usize)
        .unwrap_or_default()
}

/// `span` widened to whole lines: back to the start of its first line, forward
/// through the newline closing its last.
fn line_span(text: &str, span: Span) -> Span {
    let bytes = text.as_bytes();
    let mut start = span.start as usize;
    while start > 0 && bytes[start - 1] != b'\n' {
        start -= 1;
    }
    let mut end = span.end() as usize;
    while end < bytes.len() && bytes[end - 1] != b'\n' {
        end += 1;
    }
    Span {
        start: start as u32,
        len: (end - start) as u32,
    }
}

fn inside(inner: Span, outer: Span) -> bool {
    inner.start >= outer.start && inner.end() <= outer.end()
}

/// A string literal without its quotes.
fn bare(literal: &str) -> &str {
    let bytes = literal.as_bytes();
    let quoted = bytes.len() >= 2
        && matches!(bytes[0], b'\'' | b'"' | b'`')
        && bytes[bytes.len() - 1] == bytes[0];
    match quoted {
        true => &literal[1..literal.len() - 1],
        false => literal,
    }
}

/// A TS path without its extension; anything else unchanged.
fn drop_extension(rel: &str) -> String {
    match rel.rsplit_once('.') {
        Some((head, extension)) if EXTENSIONS.contains(&extension) => head.to_string(),
        _ => rel.to_string(),
    }
}

/// A specifier a file in `src_dir` writes, as a file in `dest_dir` would. A
/// package path anchors to the root and travels as written.
pub fn reaim(arm: &dyn Cleave, src_dir: &str, dest_dir: &str, module: &str) -> String {
    match arm.is_relative(module) {
        true => arm.spell_module(dest_dir, &join_rel(src_dir, module)),
        false => module.to_string(),
    }
}
