//! Rust syntax-flow rows over the caller's existing tree-sitter parse.

use std::collections::HashMap;

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

type Scope = HashMap<String, NodeRef>;
type LoopBreaks = Vec<(Option<String>, Vec<NodeRef>)>;

fn df_edge(sink: &mut DfSyntaxRows, src: NodeRef, dst: NodeRef) {
    sink.edges.push(Edge::new(src, dst, DfEdgeKind::Direct));
}

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
        tree_expression_span_with_attributes(node)
    }
}

fn tree_children(node: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn tree_text<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> &'a str {
    std::str::from_utf8(&source[node.byte_range()]).expect("Rust source is UTF-8")
}

fn tree_expression_span_with_attributes(node: tree_sitter::Node<'_>) -> Span {
    let span = Span {
        start: node.start_byte() as u32,
        len: (node.end_byte() - node.start_byte()) as u32,
    };
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
        .map(|child| child.start_byte() as u32)
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
    use super::df_syntax_rows_from_tree;
    use std::fmt::Write as _;
    use std::path::{Path, PathBuf};

    #[test]
    fn tree_df_rows_match_snapshot_across_pinned_rust_fixtures() {
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

        let mut actual = String::new();
        for path in files {
            let source = std::fs::read(&path).expect("fixture reads");
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&tree_sitter::Language::new(tree_sitter_rust::LANGUAGE))
                .expect("Rust grammar");
            let tree = parser.parse(&source, None).expect("tree-sitter parse");
            let rows = df_syntax_rows_from_tree(&tree, "fixture.rs", &source);
            let relative = path.strip_prefix(&fixtures).expect("fixture-relative path");
            let rendered = format!("{rows:#?}");
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hasher::write(&mut hasher, rendered.as_bytes());
            writeln!(
                &mut actual,
                "{} nodes={} edges={} params={} args={} fields={} literals={} loops={} allocations={} loop_spans={} allocator_hits={} debug_hash={:016x}",
                relative.display(),
                rows.nodes.len(),
                rows.edges.len(),
                rows.aux.params.len(),
                rows.aux.args.len(),
                rows.aux.fields.len(),
                rows.aux.lits.len(),
                rows.aux.loops.len(),
                rows.aux.allocates.len(),
                rows.aux.loop_collection_spans.len(),
                rows.aux.allocator_hits.len(),
                std::hash::Hasher::finish(&hasher),
            )
            .unwrap();
        }

        let snapshot = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/lang/rust/11_df_syntax_rows.tree.snap");
        if std::env::var_os("UPDATE_TREE_DF_SNAPSHOT").is_some() {
            std::fs::write(snapshot, &actual).expect("write tree DF snapshot");
            return;
        }
        assert_eq!(actual, include_str!("11_df_syntax_rows.tree.snap"));
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
