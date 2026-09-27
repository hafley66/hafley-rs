//! Rust syntax-flow rows over the caller's existing syn parse.

use std::collections::HashMap;
use syn::spanned::Spanned;

use super::call_metadata_rows::{line_col_to_byte, primary_type};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeRef(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub len: u32,
}
impl Span {
    pub fn end(self) -> u32 {
        self.start + self.len
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DfNodeKind {
    Param,
    LetBind,
    VarRead,
    VarWrite,
    Lit,
    CallRes,
    New,
    Member,
    Ret,
    Binop,
    Unop,
    Loop,
    If,
    Closure,
    Expr,
    Borrow,
    Break,
    Match,
    Block,
}
const BORROW: DfNodeKind = DfNodeKind::Borrow;
const BREAK: DfNodeKind = DfNodeKind::Break;
const MATCH: DfNodeKind = DfNodeKind::Match;
const BLOCK: DfNodeKind = DfNodeKind::Block;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub span: Span,
    pub kind: DfNodeKind,
    pub name: Option<String>,
}
impl Node {
    fn new(span: Span, kind: DfNodeKind) -> Self {
        Self {
            span,
            kind,
            name: None,
        }
    }
    fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DfEdgeKind {
    Direct,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub src: NodeRef,
    pub dst: NodeRef,
}
impl Edge {
    fn new(src: NodeRef, dst: NodeRef, _: DfEdgeKind) -> Self {
        Self { src, dst }
    }
}

#[derive(Default)]
struct Strings;
impl Strings {
    fn intern(&mut self, text: &str) -> String {
        text.to_owned()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfParam {
    pub node: NodeRef,
    pub pos: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfArg {
    pub call: NodeRef,
    pub pos: i64,
    pub arg: NodeRef,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfField {
    pub owner: NodeRef,
    pub name: String,
    pub value: NodeRef,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfLit {
    pub node: NodeRef,
    pub kind: &'static str,
    pub text: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfLoop {
    pub span: Span,
    pub var: Option<String>,
    pub collection: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DfAllocates {
    pub owner: Span,
}
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct DfAux {
    pub params: Vec<DfParam>,
    pub args: Vec<DfArg>,
    pub fields: Vec<DfField>,
    pub lits: Vec<DfLit>,
    pub loops: Vec<DfLoop>,
    pub allocates: Vec<DfAllocates>,
    pub loop_collection_spans: Vec<(usize, u32, u32)>,
    pub allocator_hits: Vec<Span>,
}
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct DfSyntaxRows {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub aux: DfAux,
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
fn def_span(line_starts: &[u32], start: proc_macro2::Span, end: proc_macro2::Span) -> Span {
    let first = syn_span(line_starts, start).start;
    let last = syn_span(line_starts, end).end();
    Span {
        start: first,
        len: last.saturating_sub(first),
    }
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
                if let syn::Pat::Guard(pattern) = &arm.pat {
                    let _ = flow_expr(
                        &pattern.guard,
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
        syn::Pat::Guard(guard) => bind_pat_rec(&guard.pat, line_starts, strings, scope, sink, acc),
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

/// Rust syntax-flow rows projected from the caller's tree-sitter parse.
pub fn df_syntax_rows_from_tree(
    tree: &tree_sitter::Tree,
    file: &str,
    source: &[u8],
) -> DfSyntaxRows {
    let mut rows = DfSyntaxRows::default();
    let mut writer = TreeDf {
        source,
        file,
        rows: &mut rows,
    };
    writer.items(tree.root_node(), "");
    rows
}

struct TreeDf<'a, 'rows> {
    source: &'a [u8],
    file: &'a str,
    rows: &'rows mut DfSyntaxRows,
}

impl TreeDf<'_, '_> {
    fn items(&mut self, node: tree_sitter::Node<'_>, mod_path: &str) {
        for item in tree_children(node) {
            match item.kind() {
                "function_item" => self.function(item, mod_path, None),
                "impl_item" => {
                    let owner = item
                        .child_by_field_name("type")
                        .and_then(|ty| tree_primary_type(ty, self.source));
                    if let Some(body) = item.child_by_field_name("body") {
                        for method in tree_children(body)
                            .into_iter()
                            .filter(|child| child.kind() == "function_item")
                        {
                            self.function(method, mod_path, owner.as_deref());
                        }
                    }
                }
                "mod_item" => {
                    if let (Some(name), Some(body)) = (
                        item.child_by_field_name("name"),
                        item.child_by_field_name("body"),
                    ) {
                        let nested = format!("{mod_path}{}::", tree_text(name, self.source));
                        self.items(body, &nested);
                    }
                }
                _ => {}
            }
        }
    }

    fn function(&mut self, function: tree_sitter::Node<'_>, mod_path: &str, owner: Option<&str>) {
        let (Some(name), Some(body)) = (
            function.child_by_field_name("name"),
            function.child_by_field_name("body"),
        ) else {
            return;
        };
        let name_text = tree_text(name, self.source);
        let fn_sym = match owner {
            Some(owner) => format!("{}::method::{mod_path}{owner}.{name_text}", self.file),
            None => format!("{}::function::{mod_path}{name_text}", self.file),
        };
        let mut scope = Scope::new();
        let mut loop_breaks = LoopBreaks::new();
        self.flow_fn_body(function, body, &fn_sym, &mut scope, &mut loop_breaks);
        self.claim_allocator_hits(
            0,
            Span {
                start: name.start_byte() as u32,
                len: body.end_byte() as u32 - name.start_byte() as u32,
            },
        );
    }

    fn claim_allocator_hits(&mut self, mark: usize, owner: Span) {
        if self.rows.aux.allocator_hits.len() > mark {
            self.rows.aux.allocator_hits.truncate(mark);
            self.rows.aux.allocates.push(DfAllocates { owner });
        }
    }

    fn flow_fn_body(
        &mut self,
        function: tree_sitter::Node<'_>,
        block: tree_sitter::Node<'_>,
        fn_sym: &str,
        scope: &mut Scope,
        loop_breaks: &mut LoopBreaks,
    ) {
        let mut pos = 0u32;
        if let Some(parameters) = function.child_by_field_name("parameters") {
            for parameter in tree_children(parameters) {
                if parameter.kind() != "parameter" {
                    continue;
                }
                let (Some(pattern), Some(ty)) = (
                    parameter.child_by_field_name("pattern"),
                    parameter.child_by_field_name("type"),
                ) else {
                    continue;
                };
                if tree_text(pattern, self.source).trim() == "self" {
                    continue;
                }
                if let Some(binding) = tree_simple_binding(pattern) {
                    let name = tree_text(binding, self.source);
                    let node = self.push(self.span(binding), DfNodeKind::Param, Some(name));
                    self.rows.aux.params.push(DfParam { node, pos });
                    scope.insert(name.to_owned(), node);
                }
                let _ = ty;
                pos += 1;
            }
        }
        if let Some((tail, tail_span)) = self.flow_block(block, fn_sym, scope, loop_breaks) {
            let ret = self.push(tail_span, DfNodeKind::Ret, None);
            df_edge(self.rows, tail, ret);
        }
    }

    fn flow_block(
        &mut self,
        block: tree_sitter::Node<'_>,
        fn_sym: &str,
        scope: &mut Scope,
        loop_breaks: &mut LoopBreaks,
    ) -> Option<(NodeRef, Span)> {
        let statements = tree_children(block);
        let last = statements.len().checked_sub(1);
        let mut tail = None;
        for (index, statement) in statements.iter().copied().enumerate() {
            match statement.kind() {
                "let_declaration" => {
                    if let (Some(pattern), Some(value)) = (
                        statement.child_by_field_name("pattern"),
                        statement.child_by_field_name("value"),
                    ) {
                        let rhs = self.flow_expr(value, fn_sym, scope, loop_breaks);
                        if statement.child_by_field_name("type").is_none() {
                            for (_, binding) in self.bind_pattern(pattern, scope) {
                                df_edge(self.rows, rhs, binding);
                            }
                        }
                    }
                }
                "expression_statement" => {
                    let Some(expr) = tree_children(statement).first().copied() else {
                        continue;
                    };
                    if expr.kind() == "macro_invocation"
                        && has_semicolon(statement, expr, self.source)
                    {
                        continue;
                    }
                    let node = self.flow_expr(expr, fn_sym, scope, loop_breaks);
                    if Some(index) == last && !has_semicolon(statement, expr, self.source) {
                        tail = Some((node, self.span(expr)));
                    }
                }
                "macro_invocation" => {
                    let has_semicolon = self.source[statement.end_byte()..block.end_byte()]
                        .iter()
                        .copied()
                        .find(|byte| !byte.is_ascii_whitespace())
                        == Some(b';');
                    if !has_semicolon {
                        let value = self.flow_expr(statement, fn_sym, scope, loop_breaks);
                        if Some(index) == last {
                            tail = Some((value, self.span(statement)));
                        }
                    }
                }
                kind if is_tree_expression(kind) => {
                    let node = self.flow_expr(statement, fn_sym, scope, loop_breaks);
                    if Some(index) == last {
                        tail = Some((node, self.span(statement)));
                    }
                }
                _ => {}
            }
        }
        tail
    }

    fn flow_expr(
        &mut self,
        expr: tree_sitter::Node<'_>,
        fn_sym: &str,
        scope: &mut Scope,
        loop_breaks: &mut LoopBreaks,
    ) -> NodeRef {
        let node_span = self.span(expr);
        let start = expr.start_position();
        let line_col = (start.row as u32 + 1, start.column as u32);
        match expr.kind() {
            "self" => self.path_read("self".to_owned(), node_span, scope),
            "identifier" | "scoped_identifier" | "generic_function" => {
                let name = tree_last_identifier(expr, self.source).unwrap_or_default();
                let node = self.push(node_span, DfNodeKind::VarRead, Some(&name));
                if let Some(binding) = scope.get(&name) {
                    df_edge(self.rows, *binding, node);
                }
                node
            }
            kind if is_tree_literal(kind) => {
                let node = self.push(node_span, DfNodeKind::Lit, None);
                if matches!(expr.kind(), "string_literal" | "raw_string_literal") {
                    let raw = &self.source[expr.byte_range()];
                    let is_rust_string = !raw.starts_with(b"b\"")
                        && !raw.starts_with(b"br")
                        && !raw.starts_with(b"c\"")
                        && !raw.starts_with(b"cr");
                    if let Some(literal) = is_rust_string
                        .then(|| std::str::from_utf8(raw).ok())
                        .flatten()
                        .and_then(super::const_string_rows::decode_string_literal)
                    {
                        self.rows.aux.lits.push(DfLit {
                            node,
                            kind: "lit",
                            text: literal,
                        });
                    }
                }
                node
            }
            "call_expression" => self.call_expr(expr, fn_sym, scope, loop_breaks),
            "struct_expression" => self.struct_expr(expr, fn_sym, scope, loop_breaks),
            "field_expression" => {
                let base = expr
                    .child_by_field_name("value")
                    .map(|value| self.flow_expr(value, fn_sym, scope, loop_breaks));
                let name = expr
                    .child_by_field_name("field")
                    .map(|field| tree_text(field, self.source).to_owned())
                    .unwrap_or_default();
                let node = self.push(node_span, DfNodeKind::Member, Some(&name));
                if let Some(base) = base {
                    df_edge(self.rows, base, node);
                }
                node
            }
            "parenthesized_expression" => self.pass_through(expr, fn_sym, scope, loop_breaks),
            "reference_expression" => {
                let value = tree_children(expr)
                    .into_iter()
                    .find(|child| is_tree_expression(child.kind()))
                    .map(|inner| self.flow_expr(inner, fn_sym, scope, loop_breaks));
                let node = self.push(node_span, DfNodeKind::Borrow, None);
                if let Some(value) = value {
                    df_edge(self.rows, value, node);
                }
                node
            }
            "binary_expression" | "compound_assignment_expr" => {
                let left = expr
                    .child_by_field_name("left")
                    .map(|inner| self.flow_expr(inner, fn_sym, scope, loop_breaks));
                let right = expr
                    .child_by_field_name("right")
                    .map(|inner| self.flow_expr(inner, fn_sym, scope, loop_breaks));
                let node = self.push(node_span, DfNodeKind::Binop, None);
                if let Some(left) = left {
                    df_edge(self.rows, left, node);
                }
                if let Some(right) = right {
                    df_edge(self.rows, right, node);
                }
                node
            }
            "unary_expression" => {
                let inner = expr
                    .child_by_field_name("argument")
                    .or_else(|| {
                        tree_children(expr)
                            .into_iter()
                            .find(|child| is_tree_expression(child.kind()))
                    })
                    .map(|inner| self.flow_expr(inner, fn_sym, scope, loop_breaks));
                let node = self.push(node_span, DfNodeKind::Unop, None);
                if let Some(inner) = inner {
                    df_edge(self.rows, inner, node);
                }
                node
            }
            "try_expression" => self.pass_through(expr, fn_sym, scope, loop_breaks),
            "return_expression" => self.return_expr(expr, fn_sym, scope, loop_breaks),
            "break_expression" => self.break_expr(expr, fn_sym, scope, loop_breaks),
            "for_expression" => self.for_expr(expr, fn_sym, scope, loop_breaks),
            "while_expression" => self.while_expr(expr, fn_sym, scope, loop_breaks),
            "loop_expression" => self.loop_expr(expr, fn_sym, scope, loop_breaks),
            "if_expression" => self.if_expr(expr, fn_sym, scope, loop_breaks),
            "match_expression" => self.match_expr(expr, fn_sym, scope, loop_breaks),
            "block" => self.block_expr(expr, fn_sym, scope, loop_breaks),
            "closure_expression" => self.closure_expr(expr, fn_sym, line_col, scope),
            "assignment_expression" => self.assign_expr(expr, fn_sym, scope, loop_breaks),
            _ => self.push(node_span, DfNodeKind::Expr, None),
        }
    }

    fn pass_through(
        &mut self,
        expr: tree_sitter::Node<'_>,
        fn_sym: &str,
        scope: &mut Scope,
        loops: &mut LoopBreaks,
    ) -> NodeRef {
        match tree_children(expr)
            .into_iter()
            .find(|child| is_tree_expression(child.kind()))
        {
            Some(inner) => self.flow_expr(inner, fn_sym, scope, loops),
            None => self.push(self.span(expr), DfNodeKind::Expr, None),
        }
    }

    fn call_expr(
        &mut self,
        expr: tree_sitter::Node<'_>,
        fn_sym: &str,
        scope: &mut Scope,
        loop_breaks: &mut LoopBreaks,
    ) -> NodeRef {
        let Some(function) = expr.child_by_field_name("function") else {
            return self.push(self.span(expr), DfNodeKind::Expr, None);
        };
        let function = if function.kind() == "generic_function" {
            function.child_by_field_name("function").unwrap_or(function)
        } else {
            function
        };
        if function.kind() == "field_expression" {
            let name = function
                .child_by_field_name("field")
                .map(|field| tree_text(field, self.source).to_owned())
                .unwrap_or_default();
            if matches!(
                name.as_str(),
                "collect" | "to_vec" | "to_string" | "to_owned" | "clone" | "format"
            ) {
                self.rows.aux.allocator_hits.push(self.span(expr));
            }
            let receiver = function
                .child_by_field_name("value")
                .map(|value| self.flow_expr(value, fn_sym, scope, loop_breaks));
            let args = expr
                .child_by_field_name("arguments")
                .map(tree_children)
                .unwrap_or_default()
                .into_iter()
                .filter(|arg| is_tree_expression(arg.kind()))
                .map(|arg| self.flow_expr(arg, fn_sym, scope, loop_breaks))
                .collect::<Vec<_>>();
            let node = self.push(
                self.span(function.child_by_field_name("field").unwrap_or(function)),
                DfNodeKind::CallRes,
                None,
            );
            if let Some(receiver) = receiver {
                df_edge(self.rows, receiver, node);
                self.rows.aux.args.push(DfArg {
                    call: node,
                    pos: -1,
                    arg: receiver,
                });
            }
            for (position, arg) in args.into_iter().enumerate() {
                df_edge(self.rows, arg, node);
                self.rows.aux.args.push(DfArg {
                    call: node,
                    pos: position as i64,
                    arg,
                });
            }
            return node;
        }

        let allocator = is_tree_allocator_call(function, self.source);
        if allocator {
            self.rows.aux.allocator_hits.push(self.span(expr));
        }
        let constructor = tree_path_last(function, self.source)
            .filter(|name| name.chars().next().is_some_and(char::is_uppercase));
        let args = expr
            .child_by_field_name("arguments")
            .map(tree_children)
            .unwrap_or_default()
            .into_iter()
            .filter(|arg| is_tree_expression(arg.kind()))
            .map(|arg| self.flow_expr(arg, fn_sym, scope, loop_breaks))
            .collect::<Vec<_>>();
        let node = self.push(
            self.span(expr),
            if constructor.is_some() {
                DfNodeKind::New
            } else {
                DfNodeKind::CallRes
            },
            constructor.as_deref(),
        );
        for (position, child) in args.into_iter().enumerate() {
            df_edge(self.rows, child, node);
            self.rows.aux.args.push(DfArg {
                call: node,
                pos: position as i64,
                arg: child,
            });
        }
        node
    }

    fn struct_expr(
        &mut self,
        expr: tree_sitter::Node<'_>,
        fn_sym: &str,
        scope: &mut Scope,
        loop_breaks: &mut LoopBreaks,
    ) -> NodeRef {
        let type_name = expr
            .child_by_field_name("name")
            .or_else(|| expr.child_by_field_name("type"))
            .and_then(|name| tree_path_last(name, self.source))
            .unwrap_or_default();
        let body = expr.child_by_field_name("body");
        let mut fields = Vec::new();
        let mut base = None;
        if let Some(body) = body {
            for field in tree_children(body) {
                match field.kind() {
                    "field_initializer" => {
                        if let Some(value) = field.child_by_field_name("value") {
                            let name = field
                                .child_by_field_name("field")
                                .map(|field| tree_text(field, self.source).to_owned())
                                .unwrap_or_default();
                            fields.push((name, self.flow_expr(value, fn_sym, scope, loop_breaks)));
                        }
                    }
                    "shorthand_field_initializer" => {
                        let Some(name) = tree_children(field)
                            .into_iter()
                            .find(|child| child.kind() == "identifier")
                        else {
                            continue;
                        };
                        let span = self.span(name);
                        let name = tree_text(name, self.source).to_owned();
                        let value = self.path_read(name.clone(), span, scope);
                        fields.push((name, value));
                    }
                    "base_field_initializer" => {
                        if let Some(value) = tree_children(field)
                            .into_iter()
                            .find(|child| is_tree_expression(child.kind()))
                        {
                            base = Some(self.flow_expr(value, fn_sym, scope, loop_breaks));
                        }
                    }
                    _ => {}
                }
            }
        }
        let node = self.push(self.span(expr), DfNodeKind::New, Some(&type_name));
        for (name, value) in fields {
            df_edge(self.rows, value, node);
            self.rows.aux.fields.push(DfField {
                owner: node,
                name,
                value,
            });
        }
        if let Some(value) = base {
            df_edge(self.rows, value, node);
            self.rows.aux.fields.push(DfField {
                owner: node,
                name: "..".to_owned(),
                value,
            });
        }
        node
    }

    fn path_read(&mut self, name: String, span: Span, scope: &mut Scope) -> NodeRef {
        let node = self.push(span, DfNodeKind::VarRead, Some(&name));
        if let Some(binding) = scope.get(&name) {
            df_edge(self.rows, *binding, node);
        }
        node
    }

    fn return_expr(
        &mut self,
        expr: tree_sitter::Node<'_>,
        fn_sym: &str,
        scope: &mut Scope,
        loops: &mut LoopBreaks,
    ) -> NodeRef {
        let node = self.push(self.span(expr), DfNodeKind::Ret, None);
        if let Some(value) = tree_children(expr)
            .into_iter()
            .find(|child| is_tree_expression(child.kind()))
        {
            let value = self.flow_expr(value, fn_sym, scope, loops);
            df_edge(self.rows, value, node);
        }
        node
    }

    fn break_expr(
        &mut self,
        expr: tree_sitter::Node<'_>,
        fn_sym: &str,
        scope: &mut Scope,
        loops: &mut LoopBreaks,
    ) -> NodeRef {
        let node = self.push(self.span(expr), BREAK, None);
        let children = tree_children(expr);
        if let Some(value) = children
            .iter()
            .rev()
            .copied()
            .find(|child| is_tree_expression(child.kind()))
        {
            let value = self.flow_expr(value, fn_sym, scope, loops);
            df_edge(self.rows, value, node);
            let label = children
                .iter()
                .find(|child| child.kind() == "label")
                .map(|child| {
                    tree_text(*child, self.source)
                        .trim_start_matches('\'')
                        .to_owned()
                });
            let frame = match label.as_deref() {
                Some(label) => loops
                    .iter_mut()
                    .rev()
                    .find(|(frame, _)| frame.as_deref() == Some(label)),
                None => loops.last_mut(),
            };
            if let Some((_, tails)) = frame {
                tails.push(node);
            }
        }
        node
    }

    fn for_expr(
        &mut self,
        expr: tree_sitter::Node<'_>,
        fn_sym: &str,
        scope: &mut Scope,
        loops: &mut LoopBreaks,
    ) -> NodeRef {
        let collection = expr
            .child_by_field_name("value")
            .or_else(|| expr.child_by_field_name("collection"));
        let collection_node =
            collection.map(|collection| self.flow_expr(collection, fn_sym, scope, loops));
        let binds = expr
            .child_by_field_name("pattern")
            .map(|pattern| self.bind_pattern(pattern, scope))
            .unwrap_or_default();
        if let Some(collection) = collection_node {
            for (_, binding) in &binds {
                df_edge(self.rows, collection, *binding);
            }
        }
        let var = binds
            .first()
            .map(|(name, _)| name.clone())
            .unwrap_or_default();
        let body = expr.child_by_field_name("body");
        self.loop_row(expr, (!var.is_empty()).then(|| var.clone()), collection);
        if let Some(body) = body {
            let _ = self.flow_block(body, fn_sym, scope, loops);
        }
        self.push(self.span(expr), DfNodeKind::Loop, Some(&var))
    }

    fn while_expr(
        &mut self,
        expr: tree_sitter::Node<'_>,
        fn_sym: &str,
        scope: &mut Scope,
        loops: &mut LoopBreaks,
    ) -> NodeRef {
        if let Some(condition) = expr.child_by_field_name("condition") {
            let _ = self.flow_expr(condition, fn_sym, scope, loops);
            if condition.kind() == "let_condition" {
                if let Some(pattern) = condition.child_by_field_name("pattern") {
                    let _ = self.bind_pattern(pattern, scope);
                }
            }
        }
        self.loop_row(expr, None, None);
        if let Some(body) = expr.child_by_field_name("body") {
            let _ = self.flow_block(body, fn_sym, scope, loops);
        }
        self.push(self.span(expr), DfNodeKind::Loop, None)
    }

    fn loop_expr(
        &mut self,
        expr: tree_sitter::Node<'_>,
        fn_sym: &str,
        scope: &mut Scope,
        loops: &mut LoopBreaks,
    ) -> NodeRef {
        self.loop_row(expr, None, None);
        let label = expr.child_by_field_name("label").map(|label| {
            tree_text(label, self.source)
                .trim_start_matches('\'')
                .trim_end_matches(':')
                .to_owned()
        });
        loops.push((label, Vec::new()));
        if let Some(body) = expr.child_by_field_name("body") {
            let _ = self.flow_block(body, fn_sym, scope, loops);
        }
        let (_, tails) = loops.pop().expect("loop frame was pushed");
        let node = self.push(self.span(expr), DfNodeKind::Loop, None);
        for tail in tails {
            df_edge(self.rows, tail, node);
        }
        node
    }

    fn loop_row(
        &mut self,
        expr: tree_sitter::Node<'_>,
        var: Option<String>,
        collection: Option<tree_sitter::Node<'_>>,
    ) {
        let index = self.rows.aux.loops.len();
        self.rows.aux.loops.push(DfLoop {
            span: self.span(expr),
            var,
            collection: None,
        });
        if let Some(collection) = collection {
            self.rows.aux.loop_collection_spans.push((
                index,
                collection.start_byte() as u32,
                collection.end_byte() as u32,
            ));
        }
    }

    fn if_expr(
        &mut self,
        expr: tree_sitter::Node<'_>,
        fn_sym: &str,
        scope: &mut Scope,
        loops: &mut LoopBreaks,
    ) -> NodeRef {
        if let Some(condition) = expr.child_by_field_name("condition") {
            let _ = self.flow_expr(condition, fn_sym, scope, loops);
        }
        let then_tail = expr
            .child_by_field_name("consequence")
            .and_then(|body| self.flow_block(body, fn_sym, scope, loops));
        let alternative = expr.child_by_field_name("alternative").and_then(|alt| {
            tree_children(alt)
                .into_iter()
                .find(|child| is_tree_expression(child.kind()))
                .or(Some(alt))
        });
        let else_tail = alternative.map(|alt| self.flow_expr(alt, fn_sym, scope, loops));
        let node = self.push(self.span(expr), DfNodeKind::If, None);
        if let Some((tail, _)) = then_tail {
            df_edge(self.rows, tail, node);
        }
        if let Some(tail) = else_tail {
            df_edge(self.rows, tail, node);
        }
        node
    }

    fn match_expr(
        &mut self,
        expr: tree_sitter::Node<'_>,
        fn_sym: &str,
        scope: &mut Scope,
        loops: &mut LoopBreaks,
    ) -> NodeRef {
        let scrut = expr
            .child_by_field_name("value")
            .map(|value| self.flow_expr(value, fn_sym, scope, loops));
        let mut tails = Vec::new();
        if let Some(body) = expr.child_by_field_name("body") {
            for arm in tree_children(body)
                .into_iter()
                .filter(|child| child.kind() == "match_arm")
            {
                if let Some(pattern) = arm.child_by_field_name("pattern") {
                    let guard = pattern.child_by_field_name("condition");
                    let binding_pattern = tree_children(pattern)
                        .into_iter()
                        .find(|child| guard.is_none_or(|guard| child.id() != guard.id()));
                    let binds = binding_pattern
                        .map(|pattern| self.bind_pattern(pattern, scope))
                        .unwrap_or_default();
                    if let Some(scrut) = scrut {
                        for (_, binding) in binds {
                            df_edge(self.rows, scrut, binding);
                        }
                    }
                    if let Some(guard) = guard {
                        let _ = self.flow_expr(guard, fn_sym, scope, loops);
                    }
                }
                if let Some(value) = arm.child_by_field_name("value") {
                    tails.push(self.flow_expr(value, fn_sym, scope, loops));
                }
            }
        }
        let node = self.push(self.span(expr), MATCH, None);
        for tail in tails {
            df_edge(self.rows, tail, node);
        }
        node
    }

    fn block_expr(
        &mut self,
        expr: tree_sitter::Node<'_>,
        fn_sym: &str,
        scope: &mut Scope,
        loops: &mut LoopBreaks,
    ) -> NodeRef {
        let tail = self.flow_block(expr, fn_sym, scope, loops);
        let node = self.push(self.span(expr), BLOCK, None);
        if let Some((tail, _)) = tail {
            df_edge(self.rows, tail, node);
        }
        node
    }

    fn closure_expr(
        &mut self,
        expr: tree_sitter::Node<'_>,
        fn_sym: &str,
        position: (u32, u32),
        scope: &mut Scope,
    ) -> NodeRef {
        let mark = self.rows.aux.allocator_hits.len();
        let lam_sym = format!("{fn_sym}::closure::{}_{}", position.0, position.1);
        if let Some(parameters) = expr.child_by_field_name("parameters") {
            for (pos, pattern) in tree_children(parameters).into_iter().enumerate() {
                let binding_pattern = if pattern.kind() == "parameter" {
                    pattern.child_by_field_name("pattern").unwrap_or(pattern)
                } else {
                    pattern
                };
                if let Some(binding) = tree_simple_binding(binding_pattern) {
                    let name = tree_text(binding, self.source);
                    let node = self.push(self.span(binding), DfNodeKind::Param, Some(name));
                    self.rows.aux.params.push(DfParam {
                        node,
                        pos: pos as u32,
                    });
                    scope.insert(name.to_owned(), node);
                } else {
                    let _ = self.bind_pattern(binding_pattern, scope);
                }
            }
        }
        let mut closure_loops = LoopBreaks::new();
        if let Some(body) = expr.child_by_field_name("body") {
            let body_value = if body.kind() == "block" {
                self.flow_block(body, &lam_sym, scope, &mut closure_loops)
            } else {
                Some((
                    self.flow_expr(body, &lam_sym, scope, &mut closure_loops),
                    self.span(body),
                ))
            };
            if let Some((value, span)) = body_value {
                let ret = self.push(span, DfNodeKind::Ret, None);
                df_edge(self.rows, value, ret);
            }
        }
        self.claim_allocator_hits(mark, self.span(expr));
        self.push(self.span(expr), DfNodeKind::Closure, Some(&lam_sym))
    }

    fn assign_expr(
        &mut self,
        expr: tree_sitter::Node<'_>,
        fn_sym: &str,
        scope: &mut Scope,
        loops: &mut LoopBreaks,
    ) -> NodeRef {
        let rhs = expr
            .child_by_field_name("right")
            .map(|right| self.flow_expr(right, fn_sym, scope, loops));
        if let (Some(left), Some(rhs)) = (expr.child_by_field_name("left"), rhs) {
            if matches!(left.kind(), "identifier" | "scoped_identifier") {
                let name = tree_last_identifier(left, self.source).unwrap_or_default();
                let node = self.push(self.span(expr), DfNodeKind::VarWrite, Some(&name));
                df_edge(self.rows, rhs, node);
                scope.insert(name, node);
                return node;
            }
            rhs
        } else {
            rhs.unwrap_or_else(|| self.push(self.span(expr), DfNodeKind::Expr, None))
        }
    }

    fn bind_pattern(
        &mut self,
        pattern: tree_sitter::Node<'_>,
        scope: &mut Scope,
    ) -> Vec<(String, NodeRef)> {
        let mut bindings = Vec::new();
        self.bind_pattern_inner(pattern, scope, &mut bindings);
        bindings
    }

    fn bind_pattern_inner(
        &mut self,
        pattern: tree_sitter::Node<'_>,
        scope: &mut Scope,
        bindings: &mut Vec<(String, NodeRef)>,
    ) {
        match pattern.kind() {
            "type_pattern" => {}
            "match_pattern" => {
                for child in tree_children(pattern) {
                    self.bind_pattern_inner(child, scope, bindings);
                }
            }
            "identifier" | "shorthand_field_identifier" => {
                let name = tree_text(pattern, self.source);
                if name == "_" {
                    return;
                }
                let node = self.push(self.span(pattern), DfNodeKind::LetBind, Some(name));
                scope.insert(name.to_owned(), node);
                bindings.push((name.to_owned(), node));
            }
            "tuple_pattern"
            | "slice_pattern"
            | "reference_pattern"
            | "parenthesized_pattern"
            | "mut_pattern"
            | "ref_pattern" => {
                for child in tree_children(pattern) {
                    self.bind_pattern_inner(child, scope, bindings);
                }
            }
            "tuple_struct_pattern" | "struct_pattern" => {
                for child in tree_children(pattern) {
                    if pattern
                        .child_by_field_name("type")
                        .is_some_and(|ty| ty.id() == child.id())
                    {
                        continue;
                    }
                    self.bind_pattern_inner(child, scope, bindings);
                }
            }
            "field_pattern" => {
                if let Some(pattern) = pattern.child_by_field_name("pattern") {
                    self.bind_pattern_inner(pattern, scope, bindings);
                } else if let Some(name) = pattern.child_by_field_name("name") {
                    self.bind_pattern_inner(name, scope, bindings);
                }
            }
            _ => {}
        }
    }

    fn push(&mut self, span: Span, kind: DfNodeKind, name: Option<&str>) -> NodeRef {
        let node = NodeRef(self.rows.nodes.len() as u32);
        let mut value = Node::new(span, kind);
        if let Some(name) = name.filter(|name| !name.is_empty()) {
            value = value.with_name(name.to_owned());
        }
        self.rows.nodes.push(value);
        node
    }

    fn span(&self, node: tree_sitter::Node<'_>) -> Span {
        tree_expression_span_with_attributes(node, self.source)
    }
}

fn tree_children(node: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn tree_text<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> &'a str {
    std::str::from_utf8(&source[node.byte_range()]).expect("Rust source is UTF-8")
}

fn tree_span_for_syn_columns(node: tree_sitter::Node<'_>, source: &[u8]) -> Span {
    let position = |offset: usize| {
        let prefix = &source[..offset];
        let line_start = prefix
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1);
        let column = std::str::from_utf8(&source[line_start..offset])
            .expect("tree-sitter span boundaries are UTF-8 boundaries")
            .chars()
            .count();
        line_start as u32 + column as u32
    };
    let start = position(node.start_byte());
    let end = position(node.end_byte());
    Span {
        start,
        len: end.saturating_sub(start),
    }
}

fn tree_expression_span_with_attributes(node: tree_sitter::Node<'_>, source: &[u8]) -> Span {
    let span = tree_span_for_syn_columns(node, source);
    let Some(statement) = node.parent() else {
        return span;
    };
    if statement.kind() != "expression_statement" {
        return span;
    }
    let Some(container) = statement.parent() else {
        return span;
    };
    let attribute_start = tree_children(container)
        .into_iter()
        .take_while(|child| child.end_byte() <= statement.start_byte())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .take_while(|child| {
            matches!(
                child.kind(),
                "attribute_item" | "line_comment" | "block_comment"
            )
        })
        .filter(|child| child.kind() == "attribute_item")
        .map(|child| tree_span_for_syn_columns(child, source).start)
        .min();
    let Some(start) = attribute_start else {
        return span;
    };
    Span {
        start,
        len: span.end().saturating_sub(start),
    }
}

fn tree_last_identifier(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .filter(|name| name.kind() == "identifier")
        .map(|name| tree_text(name, source).to_owned())
        .or_else(|| {
            node.child_by_field_name("field")
                .map(|name| tree_text(name, source).to_owned())
        })
        .or_else(|| {
            tree_children(node)
                .into_iter()
                .rev()
                .find_map(|child| tree_last_identifier(child, source))
        })
        .or_else(|| {
            matches!(node.kind(), "identifier" | "type_identifier")
                .then(|| tree_text(node, source).to_owned())
        })
}

fn tree_path_last(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    tree_last_identifier(node, source)
}

fn tree_simple_binding(pattern: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    match pattern.kind() {
        "identifier" => Some(pattern),
        "mut_pattern" => tree_children(pattern)
            .into_iter()
            .find(|child| child.kind() == "identifier"),
        _ => None,
    }
}

fn tree_primary_type(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let head = if node.kind() == "generic_type" {
        node.child_by_field_name("type")?
    } else {
        node
    };
    tree_path_last(head, source)
}

fn is_tree_literal(kind: &str) -> bool {
    matches!(
        kind,
        "string_literal"
            | "raw_string_literal"
            | "integer_literal"
            | "float_literal"
            | "boolean_literal"
            | "char_literal"
            | "byte_literal"
            | "byte_string_literal"
            | "raw_byte_string_literal"
    )
}

fn is_tree_expression(kind: &str) -> bool {
    is_tree_literal(kind)
        || matches!(
            kind,
            "identifier"
                | "self"
                | "scoped_identifier"
                | "generic_function"
                | "call_expression"
                | "struct_expression"
                | "field_expression"
                | "parenthesized_expression"
                | "reference_expression"
                | "binary_expression"
                | "compound_assignment_expr"
                | "unary_expression"
                | "try_expression"
                | "return_expression"
                | "break_expression"
                | "for_expression"
                | "while_expression"
                | "loop_expression"
                | "if_expression"
                | "match_expression"
                | "block"
                | "unit_expression"
                | "closure_expression"
                | "assignment_expression"
                | "index_expression"
                | "tuple_expression"
                | "array_expression"
                | "await_expression"
                | "range_expression"
                | "type_cast_expression"
                | "macro_invocation"
                | "unsafe_block"
                | "async_block"
                | "yield_expression"
                | "continue_expression"
        )
}

fn has_semicolon(
    statement: tree_sitter::Node<'_>,
    expression: tree_sitter::Node<'_>,
    source: &[u8],
) -> bool {
    source[expression.end_byte()..statement.end_byte()].contains(&b';')
}

fn is_tree_allocator_call(function: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let text = tree_text(function, source);
    let segments = text.split("::").map(str::trim).collect::<Vec<_>>();
    let Some(last) = segments.last().copied() else {
        return false;
    };
    if last != "new" {
        return matches!(
            text,
            "Vec::with_capacity" | "HashMap::with_capacity" | "String::with_capacity"
        );
    }
    segments.iter().any(|segment| {
        matches!(
            *segment,
            "Vec"
                | "HashMap"
                | "BTreeMap"
                | "HashSet"
                | "BTreeSet"
                | "VecDeque"
                | "String"
                | "LinkedList"
        )
    })
}

#[cfg(test)]
mod tree_df_tests {
    use super::{df_syntax_rows, df_syntax_rows_from_tree, DfSyntaxRows};
    use std::path::{Path, PathBuf};

    #[test]
    fn tree_df_rows_match_syn_across_pinned_rust_fixtures() {
        let fixtures =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sprefa-extract/tests/fixtures");
        let roots = [
            "type_ladder",
            "type_ladder_scope",
            "ratchet_soopy",
            "call_ladder",
        ];
        let mut files = Vec::new();
        for root in roots {
            rust_files(&fixtures.join(root), &mut files);
        }
        files.sort();
        assert!(!files.is_empty());
        for path in files {
            let source = std::fs::read(&path).expect("fixture reads");
            let source_text = std::str::from_utf8(&source).expect("fixture is UTF-8");
            let parsed = syn::parse_file(source_text)
                .unwrap_or_else(|error| panic!("Syn parse failed for {}: {error}", path.display()));
            let syn_rows = df_syntax_rows(
                &parsed,
                "fixture.rs",
                &super::super::call_metadata_rows::build_line_starts(source_text),
            );
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&tree_sitter::Language::new(tree_sitter_rust::LANGUAGE))
                .expect("Rust grammar");
            let tree = parser.parse(&source, None).expect("tree-sitter parse");
            let tree_rows = df_syntax_rows_from_tree(&tree, "fixture.rs", &source);
            assert_df_rows_equal(&tree_rows, &syn_rows, &path);
        }
    }

    fn assert_df_rows_equal(tree: &DfSyntaxRows, syn: &DfSyntaxRows, path: &Path) {
        if let Some((index, (tree_node, syn_node))) = tree
            .nodes
            .iter()
            .zip(&syn.nodes)
            .enumerate()
            .find(|(_, (tree_node, syn_node))| tree_node != syn_node)
        {
            let from = index.saturating_sub(4);
            let tree_to = (index + 4).min(tree.nodes.len());
            let syn_to = (index + 4).min(syn.nodes.len());
            panic!(
                "node {index} differs for {}: tree={tree_node:?}, syn={syn_node:?}; tree near={:?}; syn near={:?}",
                path.display()
                , &tree.nodes[from..tree_to], &syn.nodes[from..syn_to]
            );
        }
        assert_eq!(
            tree.nodes.len(),
            syn.nodes.len(),
            "node count differs for {}: tree={}, syn={}",
            path.display(),
            tree.nodes.len(),
            syn.nodes.len()
        );
        assert_eq!(
            tree.edges.len(),
            syn.edges.len(),
            "edge count differs for {}",
            path.display()
        );
        if let Some((index, (tree_edge, syn_edge))) = tree
            .edges
            .iter()
            .zip(&syn.edges)
            .enumerate()
            .find(|(_, (tree_edge, syn_edge))| tree_edge != syn_edge)
        {
            panic!(
                "edge {index} differs for {}: tree={tree_edge:?}, syn={syn_edge:?}",
                path.display()
            );
        }
        assert_eq!(
            tree.aux.params,
            syn.aux.params,
            "params differ for {}",
            path.display()
        );
        assert_eq!(
            tree.aux.args,
            syn.aux.args,
            "args differ for {}",
            path.display()
        );
        assert_eq!(
            tree.aux.fields,
            syn.aux.fields,
            "fields differ for {}",
            path.display()
        );
        assert_eq!(
            tree.aux.lits,
            syn.aux.lits,
            "lits differ for {}",
            path.display()
        );
        assert_eq!(
            tree.aux.loops,
            syn.aux.loops,
            "loops differ for {}",
            path.display()
        );
        assert_eq!(
            tree.aux.allocates,
            syn.aux.allocates,
            "allocates differ for {}",
            path.display()
        );
        assert_eq!(
            tree.aux.loop_collection_spans,
            syn.aux.loop_collection_spans,
            "loop spans differ for {}",
            path.display()
        );
        assert_eq!(
            tree.aux.allocator_hits,
            syn.aux.allocator_hits,
            "allocator hits differ for {}",
            path.display()
        );
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
