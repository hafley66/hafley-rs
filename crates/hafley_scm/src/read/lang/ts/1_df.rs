use super::*;

#[path = "0_df_rows.rs"]
mod rows;
use rows::*;
#[path = "2_df_expr.rs"]
mod expr;
use expr::df_flow_expr;

/// The DfF projector: lifts each callable's body to its value-flow graph; the
/// `closure` node's NAME is v5's `lam_sym`, and `content` resolves raw-source
/// `df_lit` rows at the end of the walk.
pub struct DfProjector<'a> {
    pub file: &'a str,
    pub content: &'a str,
}

impl Project<DfF> for DfProjector<'_> {
    type Parsed<'a> = Program<'a>;

    fn project(&self, program: &Program<'_>, strings: &mut Strings, sink: &mut FamilyBundle<DfF>) {
        for stmt in with_module_bodies(&program.body) {
            df_flow_stmt(stmt, self.file, strings, sink);
        }
        // Resolve the pending template/concat spans into raw source-slice text.
        for (node, start, end, kind) in sink.aux.lit_spans.drain(..) {
            let text = self
                .content
                .get(start as usize..end as usize)
                .unwrap_or_default()
                .to_string();
            sink.aux.lits.push(DfLit { node, kind, text });
        }
        for (index, start, end) in std::mem::take(&mut sink.aux.loop_collection_spans) {
            sink.aux.loops[index].collection = self
                .content
                .get(start as usize..end as usize)
                .map(str::to_string);
        }
        sink.aux.nests = crate::read::types::compute_nests(&sink.nodes, &sink.aux.loops);
    }
}

type Scope = std::collections::HashMap<String, NodeRef>;

fn df_flow_stmt(
    stmt: &ts::Statement,
    file: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    let top_mark = sink.nodes.len();
    use ts::Statement as S;
    match stmt {
        S::FunctionDeclaration(func) => {
            if let Some(body) = func.body.as_deref() {
                let name = func
                    .id
                    .as_ref()
                    .map(|id| id.name.to_string())
                    .unwrap_or_default();
                let fn_sym = format!("{file}::function::{name}");
                let mark = sink.nodes.len();
                let mut scope = Scope::new();
                df_seed_params(&func.params, strings, &mut scope, sink);
                df_flow_body(body, file, &fn_sym, strings, &mut scope, sink);
                df_owner(sink, strings, mark, file, &fn_sym);
            }
        }
        S::ExportDeclaration(export) => df_flow_decl(&export.declaration, file, strings, sink),
        S::ExportDefaultDeclaration(export) => match &export.declaration {
            ts::ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                if let Some(body) = func.body.as_deref() {
                    let name = func
                        .id
                        .as_ref()
                        .map(|id| id.name.to_string())
                        .unwrap_or_default();
                    let fn_sym = format!("{file}::function::{name}");
                    let mark = sink.nodes.len();
                    let mut scope = Scope::new();
                    df_seed_params(&func.params, strings, &mut scope, sink);
                    df_flow_body(body, file, &fn_sym, strings, &mut scope, sink);
                    df_owner(sink, strings, mark, file, &fn_sym);
                }
            }
            ts::ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                df_flow_class(class, file, strings, sink);
            }
            _ => {}
        },
        S::ClassDeclaration(class) => df_flow_class(class, file, strings, sink),
        // Top-level var/expr/return statements have no enclosing callable; walk
        // them under a fresh empty scope (v5 keys them `{file}::function::<top>`).
        S::VariableDeclaration(_) | S::ExpressionStatement(_) | S::ReturnStatement(_) => {
            let fn_sym = format!("{file}::function::<top>");
            let mut scope = Scope::new();
            df_flow_body_stmt(stmt, file, &fn_sym, strings, &mut scope, sink);
        }
        _ => {}
    }
    df_owner(
        sink,
        strings,
        top_mark,
        file,
        &format!("{file}::function::<top>"),
    );
}

fn df_flow_decl(
    decl: &ts::Declaration,
    file: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    use ts::Declaration as D;
    match decl {
        D::FunctionDeclaration(func) => {
            if let Some(body) = func.body.as_deref() {
                let name = func
                    .id
                    .as_ref()
                    .map(|id| id.name.to_string())
                    .unwrap_or_default();
                let fn_sym = format!("{file}::function::{name}");
                let mark = sink.nodes.len();
                let mut scope = Scope::new();
                df_seed_params(&func.params, strings, &mut scope, sink);
                df_flow_body(body, file, &fn_sym, strings, &mut scope, sink);
                df_owner(sink, strings, mark, file, &fn_sym);
            }
        }
        D::ClassDeclaration(class) => df_flow_class(class, file, strings, sink),
        _ => {}
    }
}

/// Each method body flows like a free function's, scoped under v5's
/// `{file}::method::{Owner}.{method}` sym. Field initializers are not covered
/// (no natural enclosing callable scope). Port of v5 `ts_flow_class`.
fn df_flow_class(
    class: &ts::Class,
    file: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    let owner = class
        .id
        .as_ref()
        .map(|id| id.name.to_string())
        .unwrap_or_default();
    for element in &class.body.body {
        let ts::ClassElement::MethodDefinition(method) = element else {
            continue;
        };
        let Some(body) = method.value.body.as_deref() else {
            continue;
        };
        let method_name = match &method.key {
            ts::PropertyKey::StaticIdentifier(key) => key.name.to_string(),
            _ => String::new(),
        };
        let fn_sym = format!("{file}::method::{owner}.{method_name}");
        let mark = sink.nodes.len();
        let mut scope = Scope::new();
        df_seed_params(&method.value.params, strings, &mut scope, sink);
        df_flow_body(body, file, &fn_sym, strings, &mut scope, sink);
        df_owner(sink, strings, mark, file, &fn_sym);
    }
}

fn df_flow_body(
    body: &ts::FunctionBody,
    file: &str,
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    for stmt in &body.statements {
        df_flow_body_stmt(stmt, file, fn_sym, strings, scope, sink);
    }
}

/// Lift a function value (arrow or function expression) as its own scope: seed
/// params, then walk the body. An expression-body arrow (`(x) => expr`) wraps
/// the expr as an implicit return into a `ret` node. Port of v5 `ts_lift_fn`.
/// `fn_sym` is the lambda's own sym (the binding-name sym for a const-bound
/// arrow, the `lam_sym` for an inline one): the body's nodes walk under it.
fn df_lift_fn(
    params: &ts::FormalParameters,
    body: &ts::FunctionBody,
    expression: bool,
    file: &str,
    fn_sym: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
    outer_scope: &Scope,
) {
    let mark = sink.nodes.len();
    let mut scope = outer_scope.clone();
    df_seed_params(params, strings, &mut scope, sink);
    if expression {
        if let Some(ts::Statement::ExpressionStatement(expr_stmt)) = body.statements.first() {
            let value = df_flow_expr(
                &expr_stmt.expression,
                file,
                fn_sym,
                strings,
                &mut scope,
                sink,
            );
            let ret = df_push(sink, strings, expr_stmt.span, DfNodeKind::Ret, None);
            df_edge(sink, value, ret);
        }
    } else {
        for stmt in &body.statements {
            df_flow_body_stmt(stmt, file, fn_sym, strings, &mut scope, sink);
        }
    }
    df_owner(sink, strings, mark, file, fn_sym);
}

fn df_lift_arrow(
    params: &ts::FormalParameters,
    body: &ts::ArrowFunctionBody,
    file: &str,
    fn_sym: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
    outer_scope: &Scope,
) {
    if let ts::ArrowFunctionBody::FunctionBody(body) = body {
        df_lift_fn(params, body, false, file, fn_sym, strings, sink, outer_scope);
        return;
    }
    let expression = body.to_expression();
    let mark = sink.nodes.len();
    let mut scope = outer_scope.clone();
    df_seed_params(params, strings, &mut scope, sink);
    let value = df_flow_expr(expression, file, fn_sym, strings, &mut scope, sink);
    let ret = df_push(sink, strings, expression.span(), DfNodeKind::Ret, None);
    df_edge(sink, value, ret);
    df_owner(sink, strings, mark, file, fn_sym);
}

fn df_flow_body_stmt(
    stmt: &ts::Statement,
    file: &str,
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    use ts::Statement as S;
    match stmt {
        S::VariableDeclaration(var) => {
            for declarator in &var.declarations {
                // A const-bound arrow / function expression is a callable, not a
                // value: lift its body as its own scope keyed by the binding name
                // (v5 mints `{file}::function::{binding}` and NO closure node).
                if let ts::BindingPattern::BindingIdentifier(binding) = &declarator.id {
                    match &declarator.init {
                        Some(ts::Expression::ArrowFunctionExpression(arrow)) => {
                            let sym = format!("{file}::function::{}", binding.name);
                            df_lift_arrow(&arrow.params, &arrow.body, file, &sym, strings, sink, scope);
                            continue;
                        }
                        Some(ts::Expression::FunctionExpression(func)) => {
                            if let Some(body) = func.body.as_deref() {
                                let sym = format!("{file}::function::{}", binding.name);
                                df_lift_fn(&func.params, body, false, file, &sym, strings, sink, scope);
                            }
                            continue;
                        }
                        _ => {}
                    }
                }
                let rhs = declarator
                    .init
                    .as_ref()
                    .map(|init| df_flow_expr(init, file, fn_sym, strings, scope, sink));
                if let Some(name) = binding_name(&declarator.id) {
                    let bind = df_push(
                        sink,
                        strings,
                        declarator.span,
                        DfNodeKind::LetBind,
                        Some(&name),
                    );
                    if let Some(rhs) = rhs {
                        df_edge(sink, rhs, bind);
                    }
                    scope.insert(name, bind);
                }
            }
        }
        S::ExpressionStatement(expr_stmt) => {
            let _ = df_flow_expr(&expr_stmt.expression, file, fn_sym, strings, scope, sink);
        }
        // `return EXPR`: the returned value flows into the fn's `ret` node (the
        // sink the interprocedural backward hop reads).
        S::ReturnStatement(ret_stmt) => {
            let ret = df_push(sink, strings, ret_stmt.span, DfNodeKind::Ret, None);
            if let Some(arg) = &ret_stmt.argument {
                let value = df_flow_expr(arg, file, fn_sym, strings, scope, sink);
                df_edge(sink, value, ret);
            }
        }
        S::BlockStatement(block) => {
            for inner in &block.body {
                df_flow_body_stmt(inner, file, fn_sym, strings, scope, sink);
            }
        }
        S::IfStatement(if_stmt) => {
            let _ = df_flow_expr(&if_stmt.test, file, fn_sym, strings, scope, sink);
            df_flow_body_stmt(&if_stmt.consequent, file, fn_sym, strings, scope, sink);
            if let Some(alternate) = &if_stmt.alternate {
                df_flow_body_stmt(alternate, file, fn_sym, strings, scope, sink);
            }
        }
        S::ForStatement(for_stmt) => {
            if let Some(ts::ForStatementInit::VariableDeclaration(var)) = &for_stmt.init {
                for declarator in &var.declarations {
                    let rhs = declarator
                        .init
                        .as_ref()
                        .map(|init| df_flow_expr(init, file, fn_sym, strings, scope, sink));
                    if let Some(name) = binding_name(&declarator.id) {
                        let bind = df_push(
                            sink,
                            strings,
                            declarator.span,
                            DfNodeKind::LetBind,
                            Some(&name),
                        );
                        if let Some(rhs) = rhs {
                            df_edge(sink, rhs, bind);
                        }
                        scope.insert(name, bind);
                    }
                }
            }
            if let Some(test) = &for_stmt.test {
                let _ = df_flow_expr(test, file, fn_sym, strings, scope, sink);
            }
            if let Some(update) = &for_stmt.update {
                let _ = df_flow_expr(update, file, fn_sym, strings, scope, sink);
            }
            // v5 records no var for a classic `for` (ts/flow.rs:346).
            df_loop_row(sink, for_stmt.span, None, None);
            df_flow_body_stmt(&for_stmt.body, file, fn_sym, strings, scope, sink);
        }
        S::ForOfStatement(for_stmt) => df_for_in_of(
            &for_stmt.left,
            &for_stmt.right,
            &for_stmt.body,
            for_stmt.span,
            file,
            fn_sym,
            strings,
            scope,
            sink,
        ),
        S::ForInStatement(for_stmt) => df_for_in_of(
            &for_stmt.left,
            &for_stmt.right,
            &for_stmt.body,
            for_stmt.span,
            file,
            fn_sym,
            strings,
            scope,
            sink,
        ),
        S::WhileStatement(while_stmt) => {
            let _ = df_flow_expr(&while_stmt.test, file, fn_sym, strings, scope, sink);
            df_loop_row(sink, while_stmt.span, None, None);
            df_flow_body_stmt(&while_stmt.body, file, fn_sym, strings, scope, sink);
        }
        S::DoWhileStatement(do_stmt) => {
            let _ = df_flow_expr(&do_stmt.test, file, fn_sym, strings, scope, sink);
            df_loop_row(sink, do_stmt.span, None, None);
            df_flow_body_stmt(&do_stmt.body, file, fn_sym, strings, scope, sink);
        }
        _ => {}
    }
}

/// Shared handling for `for (x of/in coll) body`: bind the loop variable, flow
/// the collection into it, record the loop row, then walk the body.
#[allow(clippy::too_many_arguments)]
fn df_for_in_of(
    left: &ts::ForStatementLeft,
    right: &ts::Expression,
    body: &ts::Statement,
    loop_span: oxc_span::Span,
    file: &str,
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    let collection = df_flow_expr(right, file, fn_sym, strings, scope, sink);
    let mut loop_var = None;
    if let ts::ForStatementLeft::VariableDeclaration(var) = left {
        if let Some(declarator) = var.declarations.first() {
            if let Some(name) = binding_name(&declarator.id) {
                let bind = df_push(
                    sink,
                    strings,
                    declarator.span,
                    DfNodeKind::LetBind,
                    Some(&name),
                );
                df_edge(sink, collection, bind);
                scope.insert(name.clone(), bind);
                loop_var = Some(name);
            }
        }
    }
    df_loop_row(sink, loop_span, loop_var, Some(right.span()));
    df_flow_body_stmt(body, file, fn_sym, strings, scope, sink);
}

/// `f(args)` / `recv.m(args)`: each argument flows into the call result; a
/// member callee flows its receiver in too. (The positional `args` slots are
/// deferred aux; the edges already carry the flow.) Port of v5 `ts_flow_call`.
fn df_flow_call(
    call: &ts::CallExpression,
    span: oxc_span::Span,
    file: &str,
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> NodeRef {
    use ts::Expression as E;
    let receiver = match &call.callee {
        E::StaticMemberExpression(member) => Some(df_flow_expr(
            &member.object,
            file,
            fn_sym,
            strings,
            scope,
            sink,
        )),
        E::ComputedMemberExpression(member) => Some(df_flow_expr(
            &member.object,
            file,
            fn_sym,
            strings,
            scope,
            sink,
        )),
        _ => None,
    };
    let mut arg_ids = Vec::new();
    for arg in &call.arguments {
        if let Some(expr) = arg.as_expression() {
            arg_ids.push(df_flow_expr(expr, file, fn_sym, strings, scope, sink));
        }
    }
    let call_res = df_push(sink, strings, span, DfNodeKind::CallRes, None);
    if let Some(recv) = receiver {
        df_edge(sink, recv, call_res);
        sink.aux.args.push(DfArg {
            call: call_res,
            pos: -1,
            arg: recv,
        });
    }
    for (pos, arg_id) in arg_ids.into_iter().enumerate() {
        df_edge(sink, arg_id, call_res);
        sink.aux.args.push(DfArg {
            call: call_res,
            pos: pos as i64,
            arg: arg_id,
        });
    }
    call_res
}

/// `recv.prop` / `recv[prop]`: the receiver flows into a `member` node whose
/// name is the accessed property (empty for a computed access). Port of v5
/// `ts_flow_member`.
fn df_flow_member(
    object: &ts::Expression,
    property: Option<&str>,
    span: oxc_span::Span,
    file: &str,
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> NodeRef {
    let object_id = df_flow_expr(object, file, fn_sym, strings, scope, sink);
    let member = df_push(sink, strings, span, DfNodeKind::Member, property);
    df_edge(sink, object_id, member);
    member
}
