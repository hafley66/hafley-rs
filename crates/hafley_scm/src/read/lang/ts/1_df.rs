use super::*;

#[path = "0_df_rows.rs"]
mod rows;
use rows::*;
#[path = "2_df_expr.rs"]
mod expr;
use expr::df_flow_expr;
#[path = "5_df_defaults.rs"]
mod defaults;
use defaults::DefaultValues;

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

struct DfOwner {
    kind: &'static str,
    name: String,
    is_async: bool,
    owner_kind: &'static str,
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
                let fn_sym = DfOwner {
                    kind: "function",
                    name,
                    is_async: func.r#async,
                    owner_kind: "function",
                };
                let mark = sink.nodes.len();
                let mut scope = Scope::new();
                df_seed_params(&func.params, strings, &mut scope, sink);
                df_flow_body(body, file, &fn_sym, strings, &mut scope, sink);
                df_owner(sink, strings, mark, &fn_sym);
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
                    let fn_sym = DfOwner {
                        kind: "function",
                        name,
                        is_async: func.r#async,
                        owner_kind: "function",
                    };
                    let mark = sink.nodes.len();
                    let mut scope = Scope::new();
                    df_seed_params(&func.params, strings, &mut scope, sink);
                    df_flow_body(body, file, &fn_sym, strings, &mut scope, sink);
                    df_owner(sink, strings, mark, &fn_sym);
                }
            }
            ts::ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                df_flow_class(class, file, strings, sink);
            }
            other => {
                if let Some(expression) = other.as_expression() {
                    let owner = DfOwner {
                        kind: "function",
                        name: "<top>".into(),
                        is_async: false,
                        owner_kind: "top_level",
                    };
                    df_flow_expr(expression, file, &owner, strings, &mut Scope::new(), sink);
                }
            }
        },
        S::ClassDeclaration(class) => df_flow_class(class, file, strings, sink),
        S::VariableDeclaration(_) | S::ExpressionStatement(_) | S::ReturnStatement(_) => {
            let fn_sym = DfOwner {
                kind: "function",
                name: "<top>".into(),
                is_async: false,
                owner_kind: "top_level",
            };
            let mut scope = Scope::new();
            df_flow_body_stmt(stmt, file, &fn_sym, strings, &mut scope, sink);
        }
        _ => {}
    }
    df_owner(
        sink,
        strings,
        top_mark,
        &DfOwner {
            kind: "function",
            name: "<top>".into(),
            is_async: false,
            owner_kind: "top_level",
        },
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
                let fn_sym = DfOwner {
                    kind: "function",
                    name,
                    is_async: func.r#async,
                    owner_kind: "function",
                };
                let mark = sink.nodes.len();
                let mut scope = Scope::new();
                df_seed_params(&func.params, strings, &mut scope, sink);
                df_flow_body(body, file, &fn_sym, strings, &mut scope, sink);
                df_owner(sink, strings, mark, &fn_sym);
            }
        }
        D::ClassDeclaration(class) => df_flow_class(class, file, strings, sink),
        D::VariableDeclaration(var) => {
            let owner = DfOwner {
                kind: "function",
                name: "<top>".into(),
                is_async: false,
                owner_kind: "top_level",
            };
            df_flow_var(var, file, &owner, strings, &mut Scope::new(), sink);
        }
        _ => {}
    }
}

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
        let (key, params, body, value, is_async) = match element {
            ts::ClassElement::MethodDefinition(method) => (
                &method.key,
                Some(&method.value.params),
                method.value.body.as_deref(),
                None,
                method.value.r#async,
            ),
            ts::ClassElement::PropertyDefinition(property) => {
                (&property.key, None, None, property.value.as_ref(), false)
            }
            _ => continue,
        };
        let name = match key {
            ts::PropertyKey::StaticIdentifier(key) => key.name.to_string(),
            _ => String::new(),
        };
        let fn_sym = DfOwner {
            kind: "method",
            name: format!("{owner}.{name}"),
            is_async,
            owner_kind: if params.is_some() {
                "class_method"
            } else {
                "class_field"
            },
        };
        let mark = sink.nodes.len();
        let mut scope = Scope::new();
        if let (Some(params), Some(body)) = (params, body) {
            df_lift_fn(params, body, file, &fn_sym, strings, sink, &scope);
        }
        if let Some(value) = value {
            df_flow_expr(value, file, &fn_sym, strings, &mut scope, sink);
        }
        df_owner(sink, strings, mark, &fn_sym);
    }
}

fn df_flow_body(
    body: &ts::FunctionBody,
    file: &str,
    fn_sym: &DfOwner,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    for stmt in &body.statements {
        df_flow_body_stmt(stmt, file, fn_sym, strings, scope, sink);
    }
}

fn df_lift_fn(
    params: &ts::FormalParameters,
    body: &ts::FunctionBody,
    file: &str,
    fn_sym: &DfOwner,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
    outer_scope: &Scope,
) {
    let mark = sink.nodes.len();
    let mut scope = outer_scope.clone();
    df_seed_params(params, strings, &mut scope, sink);
    df_flow_body(body, file, fn_sym, strings, &mut scope, sink);
    df_owner(sink, strings, mark, fn_sym);
}

fn df_lift_arrow(
    params: &ts::FormalParameters,
    body: &ts::ArrowFunctionBody,
    file: &str,
    fn_sym: &DfOwner,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
    outer_scope: &Scope,
) {
    if let ts::ArrowFunctionBody::FunctionBody(body) = body {
        df_lift_fn(params, body, file, fn_sym, strings, sink, outer_scope);
        return;
    }
    let expression = body.to_expression();
    let mark = sink.nodes.len();
    let mut scope = outer_scope.clone();
    df_seed_params(params, strings, &mut scope, sink);
    let value = df_flow_expr(expression, file, fn_sym, strings, &mut scope, sink);
    let ret = df_push(sink, strings, expression.span(), DfNodeKind::Ret, None);
    df_edge(sink, value, ret);
    df_owner(sink, strings, mark, fn_sym);
}

fn df_flow_body_stmt(
    stmt: &ts::Statement,
    file: &str,
    fn_sym: &DfOwner,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    use ts::Statement as S;
    match stmt {
        S::VariableDeclaration(var) => df_flow_var(var, file, fn_sym, strings, scope, sink),
        S::FunctionDeclaration(func) => {
            if let Some(body) = func.body.as_deref() {
                let owner = DfOwner {
                    kind: "function",
                    is_async: func.r#async,
                    owner_kind: "function",
                    name: func
                        .id
                        .as_ref()
                        .map(|id| id.name.to_string())
                        .unwrap_or_default(),
                };
                df_lift_fn(&func.params, body, file, &owner, strings, sink, scope);
            }
        }
        S::LabeledStatement(label) => {
            df_flow_body_stmt(&label.body, file, fn_sym, strings, scope, sink)
        }
        S::TryStatement(statement) => {
            for inner in &statement.block.body {
                df_flow_body_stmt(inner, file, fn_sym, strings, scope, sink);
            }
            if let Some(handler) = &statement.handler {
                for inner in &handler.body.body {
                    df_flow_body_stmt(inner, file, fn_sym, strings, scope, sink);
                }
            }
            if let Some(finalizer) = &statement.finalizer {
                for inner in &finalizer.body {
                    df_flow_body_stmt(inner, file, fn_sym, strings, scope, sink);
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
    fn_sym: &DfOwner,
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

fn df_flow_call(
    call: &ts::CallExpression,
    span: oxc_span::Span,
    file: &str,
    fn_sym: &DfOwner,
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

fn df_flow_member(
    object: &ts::Expression,
    property: Option<&str>,
    span: oxc_span::Span,
    file: &str,
    fn_sym: &DfOwner,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> NodeRef {
    let object_id = df_flow_expr(object, file, fn_sym, strings, scope, sink);
    let member = df_push(sink, strings, span, DfNodeKind::Member, property);
    df_edge(sink, object_id, member);
    member
}

fn df_flow_var(
    var: &ts::VariableDeclaration,
    file: &str,
    fn_sym: &DfOwner,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    for declarator in &var.declarations {
        DefaultValues {
            file,
            fn_sym,
            strings,
            scope,
            sink,
        }
        .visit_binding_pattern(&declarator.id);
        if let ts::BindingPattern::BindingIdentifier(binding) = &declarator.id {
            match &declarator.init {
                Some(ts::Expression::ArrowFunctionExpression(arrow)) => {
                    let sym = DfOwner {
                        kind: "function",
                        name: binding.name.to_string(),
                        is_async: arrow.r#async,
                        owner_kind: "function",
                    };
                    df_lift_arrow(&arrow.params, &arrow.body, file, &sym, strings, sink, scope);
                    continue;
                }
                Some(ts::Expression::FunctionExpression(func)) => {
                    if let Some(body) = func.body.as_deref() {
                        let sym = DfOwner {
                            kind: "function",
                            name: binding.name.to_string(),
                            is_async: func.r#async,
                            owner_kind: "function",
                        };
                        df_lift_fn(&func.params, body, file, &sym, strings, sink, scope);
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
