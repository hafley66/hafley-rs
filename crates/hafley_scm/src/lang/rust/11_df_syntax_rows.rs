//! Rust syntax-flow rows over the caller's existing syn parse.

use std::collections::HashMap;
use syn::spanned::Spanned;

use super::call_metadata_rows::{line_col_to_byte, primary_type};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeRef(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span { pub start: u32, pub len: u32 }
impl Span { pub fn end(self) -> u32 { self.start + self.len } }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DfNodeKind {
    Param, LetBind, VarRead, VarWrite, Lit, CallRes, New, Member,
    Ret, Binop, Unop, Loop, If, Closure, Expr, Borrow, Break, Match, Block,
}
const BORROW: DfNodeKind = DfNodeKind::Borrow;
const BREAK: DfNodeKind = DfNodeKind::Break;
const MATCH: DfNodeKind = DfNodeKind::Match;
const BLOCK: DfNodeKind = DfNodeKind::Block;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node { pub span: Span, pub kind: DfNodeKind, pub name: Option<String> }
impl Node {
    fn new(span: Span, kind: DfNodeKind) -> Self { Self { span, kind, name: None } }
    fn with_name(mut self, name: String) -> Self { self.name = Some(name); self }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DfEdgeKind { Direct }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge { pub src: NodeRef, pub dst: NodeRef }
impl Edge { fn new(src: NodeRef, dst: NodeRef, _: DfEdgeKind) -> Self { Self { src, dst } } }

#[derive(Default)]
struct Strings;
impl Strings { fn intern(&mut self, text: &str) -> String { text.to_owned() } }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfParam { pub node: NodeRef, pub pos: u32 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfArg { pub call: NodeRef, pub pos: i64, pub arg: NodeRef }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfField { pub owner: NodeRef, pub name: String, pub value: NodeRef }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfLit { pub node: NodeRef, pub kind: &'static str, pub text: String }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfLoop { pub span: Span, pub var: Option<String>, pub collection: Option<String> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfAllocates { pub owner: Span }
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct DfAux {
    pub params: Vec<DfParam>, pub args: Vec<DfArg>, pub fields: Vec<DfField>,
    pub lits: Vec<DfLit>, pub loops: Vec<DfLoop>, pub allocates: Vec<DfAllocates>,
    pub loop_collection_spans: Vec<(usize, u32, u32)>, pub allocator_hits: Vec<Span>,
}
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct DfSyntaxRows { pub nodes: Vec<Node>, pub edges: Vec<Edge>, pub aux: DfAux }

fn syn_span(line_starts: &[u32], span: proc_macro2::Span) -> Span {
    let start = span.start();
    let end = span.end();
    let start = line_col_to_byte(line_starts, start.line as u32, start.column as u32);
    let end = line_col_to_byte(line_starts, end.line as u32, end.column as u32);
    Span { start, len: end.saturating_sub(start) }
}
fn def_span(line_starts: &[u32], start: proc_macro2::Span, end: proc_macro2::Span) -> Span {
    let first = syn_span(line_starts, start).start;
    let last = syn_span(line_starts, end).end();
    Span { start: first, len: last.saturating_sub(first) }
}

pub fn df_syntax_rows(parsed: &syn::File, file: &str, line_starts: &[u32]) -> DfSyntaxRows {
    let mut strings = Strings;
    let mut rows = DfSyntaxRows::default();
    project_df(parsed, file, line_starts, &mut strings, &mut rows);
    rows
}


// ════════════════════════════════════════════════════════════════════════════
// DfF: intra-procedural value flow (nodes + Direct edges).
//
// Ports v5 `rust_dataflow_from` (src/graph/typegraph/rust/mod.rs:746-1332). Every
// value-bearing position in a callable's body becomes a NODE; local value flow
// becomes a Direct EDGE. The two are the dataflow graph the engine's `df_reaches`
// closure walks.
//
// What is DROPPED vs v5 (each deliberate, matching the TS DfF port):
//  - `fn_sym` ON NODES: the enclosing callable is not stored on every df node;
//    it is threaded through the walk (v5's own mechanism) purely so the
//    `closure` VALUE node carries v5's exact `lam_sym` name
//    (`{file}::function::{fn}::closure::{line}_{col}`, syn's 1-based line /
//    0-based col; methods root at `{file}::method::{Owner}.{m}`). No sym store:
//    the name derives from the walk's containment path + the closure's span.
//  - `line`/`col`: a node is a byte Span (start via line_col_to_byte), never a
//    line/col pair.
//  - the enrichment aux: `args`, `fields`, `lits`, `param_pos`. The EDGES
//    already carry every value flow.
// ════════════════════════════════════════════════════════════════════════════

/// Transient scope: a variable name -> its binding node (param or `let`).
type Scope = HashMap<String, NodeRef>;

/// Live enclosing `loop` frames for break-value routing: each entry is
/// (label, collected break-value tails). Threaded through the recursive walk so
/// `Expr::Break` finds its target loop and `Expr::Loop` drains the tails.
type LoopBreaks = Vec<(Option<String>, Vec<NodeRef>)>;

/// Project the DfF family: each callable's body lifted to its value-flow graph.
/// Port of v5 `rust_dataflow_from` (the driver half). `file` roots each fn_sym:
/// `{file}::function::{name}` for free fns, `{file}::method::{Owner}.{name}` for
/// impl methods (v5 `mint_sym`; the closure value node's name derives from it).
fn project_df(
    parsed: &syn::File,
    file: &str,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut DfSyntaxRows,
) {
    df_items(&parsed.items, "", file, line_starts, strings, sink);
}

/// `mod_path` is the enclosing inline-`mod` chain (`""` at the file root,
/// `inner::deeper::` two mods down), so sibling mods mint distinct fn syms.
fn df_items(
    items: &[syn::Item],
    mod_path: &str,
    file: &str,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut DfSyntaxRows,
) {
    for item in items {
        match item {
            syn::Item::Fn(f) => {
                let fn_sym = format!("{file}::function::{mod_path}{}", f.sig.ident);
                let mut scope = Scope::new();
                let mut loop_breaks = LoopBreaks::new();
                flow_fn_body(
                    &f.sig,
                    &f.block,
                    &fn_sym,
                    line_starts,
                    strings,
                    &mut scope,
                    sink,
                    &mut loop_breaks,
                );
                claim_allocator_hits(
                    sink,
                    0,
                    def_span(line_starts, f.sig.ident.span(), f.block.span()),
                );
            }
            syn::Item::Impl(i) => {
                let owner = primary_type(&i.self_ty);
                for ii in &i.items {
                    if let syn::ImplItem::Fn(m) = ii {
                        let fn_sym = match &owner {
                            Some(o) => format!("{file}::method::{mod_path}{o}.{}", m.sig.ident),
                            None => format!("{file}::function::{mod_path}{}", m.sig.ident),
                        };
                        let mut scope = Scope::new();
                        let mut loop_breaks = LoopBreaks::new();
                        flow_fn_body(
                            &m.sig,
                            &m.block,
                            &fn_sym,
                            line_starts,
                            strings,
                            &mut scope,
                            sink,
                            &mut loop_breaks,
                        );
                        claim_allocator_hits(
                            sink,
                            0,
                            def_span(line_starts, m.sig.ident.span(), m.block.span()),
                        );
                    }
                }
            }
            syn::Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    let nested = format!("{mod_path}{}::", m.ident);
                    df_items(inner, &nested, file, line_starts, strings, sink);
                }
            }
            _ => {}
        }
    }
}

/// v5 keys `allocators` on `fn_sym`, which a closure rebinds to its `lam_sym`
/// (rust/mod.rs:1149,1176): the INNERMOST callable claims the hit and truncates.
fn claim_allocator_hits(sink: &mut DfSyntaxRows, mark: usize, owner: Span) {
    if sink.aux.allocator_hits.len() > mark {
        sink.aux.allocator_hits.truncate(mark);
        sink.aux.allocates.push(DfAllocates { owner });
    }
}

/// Port of v5 `is_allocator_call` (rust/mod.rs:1056): a collection constructor
/// callee marks its enclosing callable as allocating.
fn is_allocator_call(expr: &syn::Expr) -> bool {
    let syn::Expr::Path(path) = expr else {
        return false;
    };
    let segments: Vec<String> = path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect();
    let full = segments.join("::");
    if full.ends_with("::new") {
        return segments.iter().any(|segment| {
            matches!(
                segment.as_str(),
                "Vec"
                    | "HashMap"
                    | "BTreeMap"
                    | "HashSet"
                    | "BTreeSet"
                    | "VecDeque"
                    | "String"
                    | "LinkedList"
            )
        });
    }
    matches!(
        full.as_str(),
        "Vec::with_capacity" | "HashMap::with_capacity" | "String::with_capacity"
    )
}

/// A collecting method call marks its enclosing callable as allocating. Port of
/// v5 `is_allocator_method` (src/graph/typegraph/rust/mod.rs:1094).
fn is_allocator_method(ident: &syn::Ident) -> bool {
    matches!(
        ident.to_string().as_str(),
        "collect" | "to_vec" | "to_string" | "to_owned" | "clone" | "format"
    )
}

/// One loop row, with the iterated expression's byte range parked on
/// `loop_collection_spans` for `project_df` to slice once the source is in hand.
fn df_loop_row(
    sink: &mut DfSyntaxRows,
    line_starts: &[u32],
    loop_span: proc_macro2::Span,
    var: Option<String>,
    collection: Option<proc_macro2::Span>,
) {
    let index = sink.aux.loops.len();
    sink.aux.loops.push(DfLoop {
        span: syn_span(line_starts, loop_span),
        var,
        collection: None,
    });
    if let Some(collection) = collection {
        let span = syn_span(line_starts, collection);
        sink.aux
            .loop_collection_spans
            .push((index, span.start, span.end()));
    }
}

/// Seed the scope with param nodes, then walk the body. The block's tail
/// expression (last stmt, no semicolon) is the fn's implicit return: mint a
/// `ret` node and flow the tail into it. Port of v5 `flow_fn_body`. `fn_sym`
/// is the callable's v5 sym (only a closure node's name derives from it).
#[allow(clippy::too_many_arguments)]
fn flow_fn_body(
    sig: &syn::Signature,
    block: &syn::Block,
    fn_sym: &str,
    line_starts: &[u32],
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut DfSyntaxRows,
    loop_breaks: &mut LoopBreaks,
) {
    // Position counts only typed params (the receiver `self` is skipped).
    let mut pos = 0u32;
    for arg in &sig.inputs {
        if let syn::FnArg::Typed(pt) = arg {
            if let syn::Pat::Ident(pi) = &*pt.pat {
                let node = df_push(
                    sink,
                    strings,
                    line_starts,
                    pi.ident.span(),
                    DfNodeKind::Param,
                    Some(&pi.ident.to_string()),
                );
                sink.aux.params.push(DfParam { node, pos });
                scope.insert(pi.ident.to_string(), node);
            }
            pos += 1;
        }
    }
    if let Some((tail, tail_span)) = flow_block(
        block,
        fn_sym,
        line_starts,
        strings,
        scope,
        sink,
        loop_breaks,
    ) {
        let ret = df_push(sink, strings, line_starts, tail_span, DfNodeKind::Ret, None);
        df_edge(sink, tail, ret);
    }
}

/// Walk a block. Returns the (node, span) of the tail value (a last statement
/// with no semicolon) so a caller can treat it as an implicit return.
#[allow(clippy::too_many_arguments)]
fn flow_block(
    block: &syn::Block,
    fn_sym: &str,
    line_starts: &[u32],
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut DfSyntaxRows,
    loop_breaks: &mut LoopBreaks,
) -> Option<(NodeRef, proc_macro2::Span)> {
    let mut tail = None;
    let statement_count = block.stmts.len();
    for (idx, stmt) in block.stmts.iter().enumerate() {
        match stmt {
            syn::Stmt::Local(local) => {
                if let Some(init) = local.init.as_ref() {
                    let rhs = flow_expr(
                        &init.expr,
                        fn_sym,
                        line_starts,
                        strings,
                        scope,
                        sink,
                        loop_breaks,
                    );
                    // Bind every ident in the pattern (`let (a, b) = pair`), each
                    // tainted by the rhs conservatively.
                    for (_, binding) in bind_pat(&local.pat, line_starts, strings, scope, sink) {
                        df_edge(sink, rhs, binding);
                    }
                }
            }
            syn::Stmt::Expr(expr, semi) => {
                // A bare `;` is Expr::Verbatim(<empty>); its span is (0,0), so
                // skip it. Non-empty Verbatim keeps its node in the `_ =>` arm.
                if matches!(expr, syn::Expr::Verbatim(tokens) if tokens.is_empty()) {
                    continue;
                }
                let stmt_span = expr.span();
                let node = flow_expr(expr, fn_sym, line_starts, strings, scope, sink, loop_breaks);
                if idx + 1 == statement_count && semi.is_none() {
                    tail = Some((node, stmt_span));
                }
            }
            syn::Stmt::Item(_) | syn::Stmt::Macro(_) => {}
        }
    }
    tail
}

/// A call whose callee is a bare path with a capitalized last segment is a
/// tuple-struct or enum-variant constructor (`Foo(x)`, `Some(x)`). Returns the
/// constructed type/variant name, or None for an ordinary call. Port of v5
/// `ctor_name`.
fn ctor_name(expr: &syn::Expr) -> Option<String> {
    if let syn::Expr::Path(path) = expr {
        let last = path.path.segments.last()?.ident.to_string();
        if last.chars().next().is_some_and(char::is_uppercase) {
            return Some(last);
        }
    }
    None
}

/// Post-order value flow for one expression. Returns the node carrying its value
/// and emits every internal edge as a side effect. `loop_breaks` is the live
/// stack of enclosing `loop` frames for break-value routing. Port of v5
/// `flow_expr`. `fn_sym` is the enclosing callable's v5 sym (a closure node's
/// name = `{fn_sym}::closure::{line}_{col}`).
#[allow(clippy::too_many_arguments)]
fn flow_expr(
    expr: &syn::Expr,
    fn_sym: &str,
    line_starts: &[u32],
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut DfSyntaxRows,
    loop_breaks: &mut LoopBreaks,
) -> NodeRef {
    let node_span = expr.span();
    let start = node_span.start();
    let (line, col) = (start.line as u32, start.column as u32);
    match expr {
        // A read of a variable: flow from its binding slot to this read.
        syn::Expr::Path(path) => {
            let name = path
                .path
                .segments
                .last()
                .map(|segment| segment.ident.to_string())
                .unwrap_or_default();
            let node = df_push(
                sink,
                strings,
                line_starts,
                node_span,
                DfNodeKind::VarRead,
                Some(&name),
            );
            if let Some(binding) = scope.get(&name) {
                df_edge(sink, *binding, node);
            }
            node
        }
        syn::Expr::Lit(lit_expr) => {
            let node = df_push(sink, strings, line_starts, node_span, DfNodeKind::Lit, None);
            // A string literal carries its cooked value into `df_lit` (v5
            // `rust` mints `lit` rows for `syn::Lit::Str` only).
            if let syn::Lit::Str(string) = &lit_expr.lit {
                sink.aux.lits.push(DfLit {
                    node,
                    kind: "lit",
                    text: string.value(),
                });
            }
            node
        }
        // f(args): each argument flows into the call result. A capitalized last
        // path segment is a tuple-struct / enum-variant constructor -> a `new`
        // node carrying the type name.
        syn::Expr::Call(call) => {
            if is_allocator_call(&call.func) {
                sink.aux
                    .allocator_hits
                    .push(syn_span(line_starts, node_span));
            }
            let constructor = ctor_name(&call.func);
            let mut children = Vec::new();
            for arg in &call.args {
                children.push(flow_expr(
                    arg,
                    fn_sym,
                    line_starts,
                    strings,
                    scope,
                    sink,
                    loop_breaks,
                ));
            }
            let kind = if constructor.is_some() {
                DfNodeKind::New
            } else {
                DfNodeKind::CallRes
            };
            let node = df_push(
                sink,
                strings,
                line_starts,
                node_span,
                kind,
                constructor.as_deref(),
            );
            for (pos, child) in children.into_iter().enumerate() {
                df_edge(sink, child, node);
                sink.aux.args.push(DfArg {
                    call: node,
                    pos: pos as i64,
                    arg: child,
                });
            }
            node
        }
        // recv.m(args): receiver + args flow into the result. The node sits at
        // the METHOD ident (the same line the call-site extractor records).
        syn::Expr::MethodCall(call) => {
            if is_allocator_method(&call.method) {
                sink.aux
                    .allocator_hits
                    .push(syn_span(line_starts, node_span));
            }
            let receiver = flow_expr(
                &call.receiver,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let mut children = Vec::new();
            for arg in &call.args {
                children.push(flow_expr(
                    arg,
                    fn_sym,
                    line_starts,
                    strings,
                    scope,
                    sink,
                    loop_breaks,
                ));
            }
            let node = df_push(
                sink,
                strings,
                line_starts,
                call.method.span(),
                DfNodeKind::CallRes,
                None,
            );
            df_edge(sink, receiver, node);
            sink.aux.args.push(DfArg {
                call: node,
                pos: -1,
                arg: receiver,
            });
            for (pos, child) in children.into_iter().enumerate() {
                df_edge(sink, child, node);
                sink.aux.args.push(DfArg {
                    call: node,
                    pos: pos as i64,
                    arg: child,
                });
            }
            node
        }
        // `Foo { a: x, ..base }`: an instantiation; each field value flows into
        // the `new` node and records a `df_field` row (the functional-update
        // base under "..").
        syn::Expr::Struct(struct_expr) => {
            let type_name = struct_expr
                .path
                .segments
                .last()
                .map(|segment| segment.ident.to_string())
                .unwrap_or_default();
            let mut filled: Vec<(String, NodeRef)> = Vec::new();
            for field in &struct_expr.fields {
                let value = flow_expr(
                    &field.expr,
                    fn_sym,
                    line_starts,
                    strings,
                    scope,
                    sink,
                    loop_breaks,
                );
                let name = match &field.member {
                    syn::Member::Named(ident) => ident.to_string(),
                    syn::Member::Unnamed(index) => index.index.to_string(),
                };
                filled.push((name, value));
            }
            let base = struct_expr.rest.as_ref().map(|rest| {
                flow_expr(rest, fn_sym, line_starts, strings, scope, sink, loop_breaks)
            });
            let node = df_push(
                sink,
                strings,
                line_starts,
                node_span,
                DfNodeKind::New,
                Some(type_name.as_str()),
            );
            for (name, value) in filled {
                df_edge(sink, value, node);
                sink.aux.fields.push(DfField {
                    owner: node,
                    name,
                    value,
                });
            }
            if let Some(base) = base {
                df_edge(sink, base, node);
                sink.aux.fields.push(DfField {
                    owner: node,
                    name: "..".to_string(),
                    value: base,
                });
            }
            node
        }
        // `base.f` / `tuple.0`: a field read. The base flows into a `member` node
        // whose name is the field name (field-sensitive flow).
        syn::Expr::Field(field) => {
            let base = flow_expr(
                &field.base,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let name = match &field.member {
                syn::Member::Named(ident) => ident.to_string(),
                syn::Member::Unnamed(index) => index.index.to_string(),
            };
            let node = df_push(
                sink,
                strings,
                line_starts,
                node_span,
                DfNodeKind::Member,
                Some(&name),
            );
            df_edge(sink, base, node);
            node
        }
        syn::Expr::Paren(paren) => flow_expr(
            &paren.expr,
            fn_sym,
            line_starts,
            strings,
            scope,
            sink,
            loop_breaks,
        ),
        syn::Expr::Reference(reference) => {
            let inner = flow_expr(
                &reference.expr,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let node = df_push(sink, strings, line_starts, node_span, BORROW, None);
            df_edge(sink, inner, node);
            node
        }
        syn::Expr::Binary(binary) => {
            let left = flow_expr(
                &binary.left,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let right = flow_expr(
                &binary.right,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let node = df_push(
                sink,
                strings,
                line_starts,
                node_span,
                DfNodeKind::Binop,
                None,
            );
            df_edge(sink, left, node);
            df_edge(sink, right, node);
            node
        }
        syn::Expr::Unary(unary) => {
            let inner = flow_expr(
                &unary.expr,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let node = df_push(
                sink,
                strings,
                line_starts,
                node_span,
                DfNodeKind::Unop,
                None,
            );
            df_edge(sink, inner, node);
            node
        }
        // Transparent pass-through: the `?` operator does not alter value flow.
        syn::Expr::Try(try_expr) => flow_expr(
            &try_expr.expr,
            fn_sym,
            line_starts,
            strings,
            scope,
            sink,
            loop_breaks,
        ),
        // `return EXPR`: the returned value flows into the fn's `ret` node.
        syn::Expr::Return(return_expr) => {
            let node = df_push(sink, strings, line_starts, node_span, DfNodeKind::Ret, None);
            if let Some(inner) = &return_expr.expr {
                let value = flow_expr(
                    inner,
                    fn_sym,
                    line_starts,
                    strings,
                    scope,
                    sink,
                    loop_breaks,
                );
                df_edge(sink, value, node);
            }
            node
        }
        // `break EXPR;`: the value's tail is recorded into the `loop_breaks`
        // frame it targets; `Expr::Loop` drains its frame's tails into edges on
        // its own node.
        syn::Expr::Break(break_expr) => {
            let node = df_push(sink, strings, line_starts, node_span, BREAK, None);
            if let Some(value_expr) = &break_expr.expr {
                let value = flow_expr(
                    value_expr,
                    fn_sym,
                    line_starts,
                    strings,
                    scope,
                    sink,
                    loop_breaks,
                );
                df_edge(sink, value, node);
                let target_label = break_expr
                    .label
                    .as_ref()
                    .map(|lifetime| lifetime.ident.to_string());
                let frame = match &target_label {
                    Some(label) => loop_breaks
                        .iter_mut()
                        .rev()
                        .find(|(frame_label, _)| frame_label.as_deref() == Some(label.as_str())),
                    None => loop_breaks.last_mut(),
                };
                if let Some((_, tails)) = frame {
                    tails.push(node);
                }
            }
            node
        }
        // `for pat in coll { body }`: bind the loop variable from the collection
        // (each element tainted conservatively), then walk the body.
        syn::Expr::ForLoop(for_loop) => {
            let collection = flow_expr(
                &for_loop.expr,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let binds = bind_pat(&for_loop.pat, line_starts, strings, scope, sink);
            for (_, binding) in &binds {
                df_edge(sink, collection, *binding);
            }
            let loop_var = binds
                .first()
                .map(|(name, _)| name.clone())
                .unwrap_or_default();
            df_loop_row(
                sink,
                line_starts,
                node_span,
                Some(loop_var.clone()).filter(|name| !name.is_empty()),
                Some(for_loop.expr.span()),
            );
            flow_block(
                &for_loop.body,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            df_push(
                sink,
                strings,
                line_starts,
                node_span,
                DfNodeKind::Loop,
                Some(&loop_var),
            )
        }
        // `while cond { body }`: no collection; walk cond + body.
        syn::Expr::While(while_expr) => {
            let _ = flow_expr(
                &while_expr.cond,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            if let syn::Expr::Let(let_expr) = &*while_expr.cond {
                let _ = bind_pat(&let_expr.pat, line_starts, strings, scope, sink);
            }
            df_loop_row(sink, line_starts, node_span, None, None);
            flow_block(
                &while_expr.body,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            df_push(
                sink,
                strings,
                line_starts,
                node_span,
                DfNodeKind::Loop,
                None,
            )
        }
        // `loop { body }`: Rust's value-yielding loop. Push a fresh `loop_breaks`
        // frame before walking the body, pop it after, and edge every collected
        // break tail into this loop's node.
        syn::Expr::Loop(loop_expr) => {
            df_loop_row(sink, line_starts, node_span, None, None);
            loop_breaks.push((
                loop_expr
                    .label
                    .as_ref()
                    .map(|label| label.name.ident.to_string()),
                Vec::new(),
            ));
            flow_block(
                &loop_expr.body,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let (_, break_tails) = loop_breaks
                .pop()
                .expect("Expr::Loop popping the frame it pushed");
            let node = df_push(
                sink,
                strings,
                line_starts,
                node_span,
                DfNodeKind::Loop,
                None,
            );
            for tail in break_tails {
                df_edge(sink, tail, node);
            }
            node
        }
        // `if cond { then } else { els }`: branch TAILS flow into the `if` node,
        // so a value-position if carries both branches through to the binding.
        syn::Expr::If(if_expr) => {
            let _ = flow_expr(
                &if_expr.cond,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let then_tail = flow_block(
                &if_expr.then_branch,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let else_tail = if_expr.else_branch.as_ref().map(|(_, els)| {
                flow_expr(els, fn_sym, line_starts, strings, scope, sink, loop_breaks)
            });
            let node = df_push(sink, strings, line_starts, node_span, DfNodeKind::If, None);
            if let Some((tail, _)) = then_tail {
                df_edge(sink, tail, node);
            }
            if let Some(else_tail) = else_tail {
                df_edge(sink, else_tail, node);
            }
            node
        }
        // `match scrut { arms }`: scrut + each arm body; arm-bound patterns derive
        // from the scrutinee. Arm tails flow into the `match` node.
        syn::Expr::Match(match_expr) => {
            let scrut = flow_expr(
                &match_expr.expr,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let mut arm_tails = Vec::new();
            for arm in &match_expr.arms {
                for (_, binding) in bind_pat(&arm.pat, line_starts, strings, scope, sink) {
                    df_edge(sink, scrut, binding);
                }
                if let Some((_, guard)) = &arm.guard {
                    let _ = flow_expr(
                        guard,
                        fn_sym,
                        line_starts,
                        strings,
                        scope,
                        sink,
                        loop_breaks,
                    );
                }
                arm_tails.push(flow_expr(
                    &arm.body,
                    fn_sym,
                    line_starts,
                    strings,
                    scope,
                    sink,
                    loop_breaks,
                ));
            }
            let node = df_push(sink, strings, line_starts, node_span, MATCH, None);
            for tail in arm_tails {
                df_edge(sink, tail, node);
            }
            node
        }
        // `{ stmts }` as an expression: the tail statement's value flows through.
        syn::Expr::Block(block_expr) => {
            let tail = flow_block(
                &block_expr.block,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            let node = df_push(sink, strings, line_starts, node_span, BLOCK, None);
            if let Some((tail, _)) = tail {
                df_edge(sink, tail, node);
            }
            node
        }
        // `|params| body`: lift the lambda as its own scope (seed params, walk the
        // body under v5's `lam_sym`, mint a ret for the body value), then mint the
        // `closure` VALUE node in the enclosing fn carrying that exact sym as its
        // name (`{fn_sym}::closure::{line}_{col}`, syn's 1-based line / 0-based
        // col; chains when nested). The enclosing scope is shared so captures
        // resolve.
        syn::Expr::Closure(closure) => {
            let allocator_mark = sink.aux.allocator_hits.len();
            let lam_sym = format!("{fn_sym}::closure::{line}_{col}");
            for (pos, input) in closure.inputs.iter().enumerate() {
                let ident_pat = match input {
                    syn::Pat::Type(pat_type) => pat_type.pat.as_ref(),
                    other => other,
                };
                if let syn::Pat::Ident(ident) = ident_pat {
                    let node = df_push(
                        sink,
                        strings,
                        line_starts,
                        ident.ident.span(),
                        DfNodeKind::Param,
                        Some(&ident.ident.to_string()),
                    );
                    sink.aux.params.push(DfParam {
                        node,
                        pos: pos as u32,
                    });
                    scope.insert(ident.ident.to_string(), node);
                } else {
                    let _ = bind_pat(input, line_starts, strings, scope, sink);
                }
            }
            // A `break` cannot cross a closure boundary, so the body gets a fresh
            // loop_breaks stack.
            let mut closure_loop_breaks = LoopBreaks::new();
            let body_val = match closure.body.as_ref() {
                syn::Expr::Block(block) => flow_block(
                    &block.block,
                    &lam_sym,
                    line_starts,
                    strings,
                    scope,
                    sink,
                    &mut closure_loop_breaks,
                ),
                other => {
                    let other_span = other.span();
                    let value = flow_expr(
                        other,
                        &lam_sym,
                        line_starts,
                        strings,
                        scope,
                        sink,
                        &mut closure_loop_breaks,
                    );
                    Some((value, other_span))
                }
            };
            if let Some((value, ret_span)) = body_val {
                let ret = df_push(sink, strings, line_starts, ret_span, DfNodeKind::Ret, None);
                df_edge(sink, value, ret);
            }
            claim_allocator_hits(sink, allocator_mark, syn_span(line_starts, node_span));
            df_push(
                sink,
                strings,
                line_starts,
                node_span,
                DfNodeKind::Closure,
                Some(&lam_sym),
            )
        }
        // `lhs = rhs`: flow rhs; rebind a write slot so later reads see the new
        // value (taint-correct for reassignment).
        syn::Expr::Assign(assign) => {
            let rhs = flow_expr(
                &assign.right,
                fn_sym,
                line_starts,
                strings,
                scope,
                sink,
                loop_breaks,
            );
            if let syn::Expr::Path(path) = assign.left.as_ref() {
                if let Some(name) = path
                    .path
                    .segments
                    .last()
                    .map(|segment| segment.ident.to_string())
                {
                    let node = df_push(
                        sink,
                        strings,
                        line_starts,
                        node_span,
                        DfNodeKind::VarWrite,
                        Some(&name),
                    );
                    df_edge(sink, rhs, node);
                    scope.insert(name, node);
                    return node;
                }
            }
            rhs
        }
        // Macros (format!/println!), verbatim, and remaining variants: mint a
        // node but don't chase. Conservative: may miss flows, never invents.
        _ => df_push(
            sink,
            strings,
            line_starts,
            node_span,
            DfNodeKind::Expr,
            None,
        ),
    }
}

/// Bind every identifier in a pattern into scope, returning `(name, binding)`
/// for each. Handles single-ident + tuple / tuple-struct / struct / reference /
/// paren / slice destructuring. Port of v5 `bind_pat`/`bind_pat_rec`.
fn bind_pat(
    pattern: &syn::Pat,
    line_starts: &[u32],
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut DfSyntaxRows,
) -> Vec<(String, NodeRef)> {
    let mut acc = Vec::new();
    bind_pat_rec(pattern, line_starts, strings, scope, sink, &mut acc);
    acc
}

#[allow(clippy::too_many_arguments)]
fn bind_pat_rec(
    pattern: &syn::Pat,
    line_starts: &[u32],
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut DfSyntaxRows,
    acc: &mut Vec<(String, NodeRef)>,
) {
    match pattern {
        syn::Pat::Ident(ident) => {
            let binding = df_push(
                sink,
                strings,
                line_starts,
                ident.ident.span(),
                DfNodeKind::LetBind,
                Some(&ident.ident.to_string()),
            );
            scope.insert(ident.ident.to_string(), binding);
            acc.push((ident.ident.to_string(), binding));
        }
        syn::Pat::Tuple(tuple) => {
            for elem in &tuple.elems {
                bind_pat_rec(elem, line_starts, strings, scope, sink, acc);
            }
        }
        syn::Pat::TupleStruct(tuple_struct) => {
            for elem in &tuple_struct.elems {
                bind_pat_rec(elem, line_starts, strings, scope, sink, acc);
            }
        }
        syn::Pat::Struct(struct_pat) => {
            for field in &struct_pat.fields {
                bind_pat_rec(&field.pat, line_starts, strings, scope, sink, acc);
            }
        }
        syn::Pat::Reference(reference) => {
            bind_pat_rec(&reference.pat, line_starts, strings, scope, sink, acc)
        }
        syn::Pat::Paren(paren) => bind_pat_rec(&paren.pat, line_starts, strings, scope, sink, acc),
        syn::Pat::Slice(slice) => {
            for elem in &slice.elems {
                bind_pat_rec(elem, line_starts, strings, scope, sink, acc);
            }
        }
        _ => {}
    }
}

/// Push one df node at its FULL syntactic extent: `FlatFact::Edge` carries
/// endpoint spans only, so a start-only anchor merges distinct value nodes.
fn df_push(
    sink: &mut DfSyntaxRows,
    strings: &mut Strings,
    line_starts: &[u32],
    node_span: proc_macro2::Span,
    kind: DfNodeKind,
    name: Option<&str>,
) -> NodeRef {
    let node_ref = NodeRef(sink.nodes.len() as u32);
    let mut node = Node::new(syn_span(line_starts, node_span), kind);
    if let Some(name) = name.filter(|candidate| !candidate.is_empty()) {
        node = node.with_name(strings.intern(name));
    }
    sink.nodes.push(node);
    node_ref
}

/// One Direct value edge: `dst` receives the value of `src`.
fn df_edge(sink: &mut DfSyntaxRows, src: NodeRef, dst: NodeRef) {
    sink.edges.push(Edge::new(src, dst, DfEdgeKind::Direct));
}
