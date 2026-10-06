use super::*;

#[path = "3_df_jsx.rs"]
mod jsx;
use jsx::{df_jsx_element, df_jsx_fragment};

pub(super) fn df_flow_expr(
    expr: &ts::Expression,
    file: &str,
    fn_sym: &DfOwner,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> NodeRef {
    use ts::Expression as E;
    let span = expr.span();
    match expr {
        // A read of a variable: flow from its binding slot.
        E::Identifier(id) => {
            let name = id.name.to_string();
            let node = df_push(sink, strings, span, DfNodeKind::VarRead, Some(&name));
            if let Some(binding) = scope.get(&name) {
                df_edge(sink, *binding, node);
            }
            node
        }
        // A string literal carries its cooked value into `df_lit` (the only
        // literal kind that does; numbers/bools/regex stay textless `lit` nodes).
        E::StringLiteral(string) => {
            let node = df_push(sink, strings, span, DfNodeKind::Lit, None);
            sink.aux.lits.push(DfLit {
                node,
                kind: "lit",
                text: string.value.to_string(),
            });
            node
        }
        E::NumericLiteral(_)
        | E::BooleanLiteral(_)
        | E::NullLiteral(_)
        | E::BigIntLiteral(_)
        | E::RegExpLiteral(_) => df_push(sink, strings, span, DfNodeKind::Lit, None),
        E::ClassExpression(class) => {
            df_flow_class(class, file, strings, sink);
            df_push(sink, strings, span, DfNodeKind::Expr, None)
        }
        E::CallExpression(call) => df_flow_call(call, span, file, fn_sym, strings, scope, sink),
        // `new Foo(args)`: a `new` node carrying the class name; each arg flows in.
        E::NewExpression(new_expr) => {
            let type_name = match &new_expr.callee {
                E::Identifier(id) => Some(id.name.to_string()),
                E::StaticMemberExpression(member) => Some(member.property.name.to_string()),
                _ => None,
            };
            let mut arg_ids = Vec::new();
            for arg in &new_expr.arguments {
                if let Some(expr) = arg.as_expression() {
                    arg_ids.push(df_flow_expr(expr, file, fn_sym, strings, scope, sink));
                }
            }
            let new_node = df_push(sink, strings, span, DfNodeKind::New, type_name.as_deref());
            for (pos, arg_id) in arg_ids.into_iter().enumerate() {
                df_edge(sink, arg_id, new_node);
                sink.aux.args.push(DfArg {
                    call: new_node,
                    pos: pos as i64,
                    arg: arg_id,
                });
            }
            new_node
        }
        // `{ a: x, ...rest }`: a composite `new` node; each named property
        // records a `df_field` row (spread under "..").
        E::ObjectExpression(object) => {
            let mut filled: Vec<(String, NodeRef)> = Vec::new();
            for property in &object.properties {
                match property {
                    ts::ObjectPropertyKind::ObjectProperty(prop) => {
                        let value = df_flow_expr(&prop.value, file, fn_sym, strings, scope, sink);
                        let name = match &prop.key {
                            ts::PropertyKey::StaticIdentifier(ident) => ident.name.to_string(),
                            ts::PropertyKey::StringLiteral(string) => string.value.to_string(),
                            _ => String::new(),
                        };
                        filled.push((name, value));
                    }
                    ts::ObjectPropertyKind::SpreadProperty(spread) => {
                        let value =
                            df_flow_expr(&spread.argument, file, fn_sym, strings, scope, sink);
                        filled.push(("..".into(), value));
                    }
                }
            }
            let new_node = df_push(sink, strings, span, DfNodeKind::New, None);
            for (name, value) in filled {
                df_edge(sink, value, new_node);
                if !name.is_empty() {
                    sink.aux.fields.push(DfField {
                        owner: new_node,
                        name,
                        value,
                    });
                }
            }
            new_node
        }
        E::ArrayExpression(array) => {
            let mut filled: Vec<(String, NodeRef)> = Vec::new();
            for element in &array.elements {
                match element {
                    ts::ArrayExpressionElement::SpreadElement(spread) => {
                        let value =
                            df_flow_expr(&spread.argument, file, fn_sym, strings, scope, sink);
                        filled.push(("..".into(), value));
                    }
                    ts::ArrayExpressionElement::Elision(_) => {}
                    _ => {
                        if let Some(expr) = element.as_expression() {
                            let value = df_flow_expr(expr, file, fn_sym, strings, scope, sink);
                            filled.push((String::new(), value));
                        }
                    }
                }
            }
            let new_node = df_push(sink, strings, span, DfNodeKind::New, None);
            for (name, value) in filled {
                df_edge(sink, value, new_node);
                if !name.is_empty() {
                    sink.aux.fields.push(DfField {
                        owner: new_node,
                        name,
                        value,
                    });
                }
            }
            new_node
        }
        // recv.prop / recv[prop]: receiver flows into a `member` node.
        E::StaticMemberExpression(member) => df_flow_member(
            &member.object,
            Some(member.property.name.as_str()),
            span,
            file,
            fn_sym,
            strings,
            scope,
            sink,
        ),
        E::ComputedMemberExpression(member) => df_flow_member(
            &member.object,
            None,
            span,
            file,
            fn_sym,
            strings,
            scope,
            sink,
        ),
        // `a + b` is its own `concat` kind (so a string-construction query matches
        // `kind IN (template, concat)`); any other binary op is `binop`.
        E::BinaryExpression(binary) => {
            let left = df_flow_expr(&binary.left, file, fn_sym, strings, scope, sink);
            let right = df_flow_expr(&binary.right, file, fn_sym, strings, scope, sink);
            let kind = if binary.operator == ts::BinaryOperator::Addition {
                CONCAT
            } else {
                DfNodeKind::Binop
            };
            let node = df_push(sink, strings, span, kind, None);
            df_edge(sink, left, node);
            df_edge(sink, right, node);
            if binary.operator == ts::BinaryOperator::Addition {
                sink.aux
                    .lit_spans
                    .push((node, binary.span.start, binary.span.end, "concat"));
            }
            node
        }
        E::ArrowFunctionExpression(arrow) => {
            let lam_sym = DfOwner {
                kind: fn_sym.kind,
                name: format!("{}::closure::{}", fn_sym.name, span.start),
            };
            df_lift_arrow(
                &arrow.params,
                &arrow.body,
                file,
                &lam_sym,
                strings,
                sink,
                scope,
            );
            df_push(
                sink,
                strings,
                span,
                DfNodeKind::Closure,
                Some(&format!("{file}::{}::{}", lam_sym.kind, lam_sym.name)),
            )
        }
        E::FunctionExpression(func) => match func.body.as_deref() {
            Some(body) => {
                let lam_sym = DfOwner {
                    kind: fn_sym.kind,
                    name: format!("{}::closure::{}", fn_sym.name, span.start),
                };
                df_lift_fn(&func.params, body, file, &lam_sym, strings, sink, scope);
                df_push(
                    sink,
                    strings,
                    span,
                    DfNodeKind::Closure,
                    Some(&format!("{file}::{}::{}", lam_sym.kind, lam_sym.name)),
                )
            }
            None => df_push(sink, strings, span, DfNodeKind::Expr, None),
        },
        // Transparent wrappers: flow the inner expression straight through.
        E::ParenthesizedExpression(paren) => {
            df_flow_expr(&paren.expression, file, fn_sym, strings, scope, sink)
        }
        E::TSAsExpression(inner) => {
            df_flow_expr(&inner.expression, file, fn_sym, strings, scope, sink)
        }
        E::TSSatisfiesExpression(inner) => {
            df_flow_expr(&inner.expression, file, fn_sym, strings, scope, sink)
        }
        E::TSNonNullExpression(inner) => {
            df_flow_expr(&inner.expression, file, fn_sym, strings, scope, sink)
        }
        E::AwaitExpression(inner) => {
            df_flow_expr(&inner.argument, file, fn_sym, strings, scope, sink)
        }
        E::TSTypeAssertion(inner) => {
            df_flow_expr(&inner.expression, file, fn_sym, strings, scope, sink)
        }
        E::TSInstantiationExpression(inner) => {
            df_flow_expr(&inner.expression, file, fn_sym, strings, scope, sink)
        }
        E::ChainExpression(chain) => {
            use ts::ChainElement as Chain;
            use ts::MemberExpression as Member;
            match &chain.expression {
                Chain::CallExpression(call) => {
                    df_flow_call(call, span, file, fn_sym, strings, scope, sink)
                }
                other => match other.member_expression() {
                    Some(Member::StaticMemberExpression(member)) => df_flow_member(
                        &member.object,
                        Some(member.property.name.as_str()),
                        span,
                        file,
                        fn_sym,
                        strings,
                        scope,
                        sink,
                    ),
                    Some(Member::ComputedMemberExpression(member)) => df_flow_member(
                        &member.object,
                        None,
                        span,
                        file,
                        fn_sym,
                        strings,
                        scope,
                        sink,
                    ),
                    Some(Member::PrivateFieldExpression(member)) => df_flow_member(
                        &member.object,
                        None,
                        span,
                        file,
                        fn_sym,
                        strings,
                        scope,
                        sink,
                    ),
                    None => df_push(sink, strings, span, DfNodeKind::Expr, None),
                },
            }
        }
        // `x = y` as a value evaluates to the assigned value.
        E::AssignmentExpression(assignment) => {
            DefaultValues {
                file,
                fn_sym,
                strings,
                scope,
                sink,
            }
            .visit_assignment_target(&assignment.left);
            df_flow_expr(&assignment.right, file, fn_sym, strings, scope, sink)
        }
        // `test ? cons : alt`: the value is EITHER branch (both flow in); the
        // test is a guard (walked, not edged).
        E::ConditionalExpression(cond) => {
            let _test = df_flow_expr(&cond.test, file, fn_sym, strings, scope, sink);
            let consequent = df_flow_expr(&cond.consequent, file, fn_sym, strings, scope, sink);
            let alternate = df_flow_expr(&cond.alternate, file, fn_sym, strings, scope, sink);
            let node = df_push(sink, strings, span, COND, None);
            df_edge(sink, consequent, node);
            df_edge(sink, alternate, node);
            node
        }
        // `&&` / `||` / `??`: for `||` / `??` the value is EITHER operand; for
        // `&&` the value is the right (left is a guard).
        E::LogicalExpression(logic) => {
            use ts::LogicalOperator as Op;
            let left = df_flow_expr(&logic.left, file, fn_sym, strings, scope, sink);
            let right = df_flow_expr(&logic.right, file, fn_sym, strings, scope, sink);
            let node = df_push(sink, strings, span, DfNodeKind::Logic, None);
            if matches!(logic.operator, Op::Or | Op::Coalesce) {
                df_edge(sink, left, node);
            }
            df_edge(sink, right, node);
            node
        }
        // `(a, b, c)`: the value is the LAST expression; earlier ones are effect.
        E::SequenceExpression(sequence) => {
            let mut last = df_push(sink, strings, span, DfNodeKind::Expr, None);
            for sub in &sequence.expressions {
                last = df_flow_expr(sub, file, fn_sym, strings, scope, sink);
            }
            last
        }
        // `` `hello ${name}` ``: each interpolation flows into a `template` node;
        // the raw source slice is the `df_lit` text.
        E::TemplateLiteral(template) => {
            let node = df_push(sink, strings, span, TEMPLATE, None);
            for sub in &template.expressions {
                let value = df_flow_expr(sub, file, fn_sym, strings, scope, sink);
                df_edge(sink, value, node);
            }
            sink.aux
                .lit_spans
                .push((node, template.span.start, template.span.end, "template"));
            node
        }
        E::TaggedTemplateExpression(tagged) => {
            let _tag = df_flow_expr(&tagged.tag, file, fn_sym, strings, scope, sink);
            let node = df_push(sink, strings, span, TEMPLATE, None);
            for sub in &tagged.quasi.expressions {
                let value = df_flow_expr(sub, file, fn_sym, strings, scope, sink);
                df_edge(sink, value, node);
            }
            // The quasi (the TemplateLiteral portion) is the string source; its
            // span excludes the tag prefix.
            sink.aux.lit_spans.push((
                node,
                tagged.quasi.span.start,
                tagged.quasi.span.end,
                "template",
            ));
            node
        }
        E::JSXElement(element) => df_jsx_element(element, file, fn_sym, strings, scope, sink),
        E::JSXFragment(fragment) => df_jsx_fragment(fragment, file, fn_sym, strings, scope, sink),
        // Remaining variants retain a generic expression node.
        _ => df_push(sink, strings, span, DfNodeKind::Expr, None),
    }
}
