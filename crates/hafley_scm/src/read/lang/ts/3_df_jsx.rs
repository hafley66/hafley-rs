use super::*;

pub(super) fn df_jsx_element(
    el: &ts::JSXElement,
    file: &str,
    fn_sym: &DfOwner,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> NodeRef {
    let eager_mark = sink.nodes.len();
    use ts::JSXElementName as N;
    let component = matches!(
        &el.opening_element.name,
        N::IdentifierReference(_) | N::MemberExpression(_)
    );
    let mut fields = Vec::new();
    for attribute in &el.opening_element.attributes {
        match attribute {
            ts::JSXAttributeItem::Attribute(attribute) => {
                let name = match &attribute.name {
                    ts::JSXAttributeName::Identifier(id) => id.name.to_string(),
                    ts::JSXAttributeName::NamespacedName(name) => name.name.name.to_string(),
                };
                let value = match &attribute.value {
                    None => df_push(sink, strings, attribute.span, DfNodeKind::Lit, None),
                    Some(ts::JSXAttributeValue::StringLiteral(value)) => {
                        let node = df_push(sink, strings, value.span, DfNodeKind::Lit, None);
                        sink.aux.lits.push(DfLit {
                            node,
                            kind: "lit",
                            text: value.value.to_string(),
                        });
                        node
                    }
                    Some(ts::JSXAttributeValue::ExpressionContainer(value)) => {
                        let Some(expression) = value.expression.as_expression() else {
                            continue;
                        };
                        df_flow_expr(expression, file, fn_sym, strings, scope, sink)
                    }
                    Some(ts::JSXAttributeValue::Element(element)) => {
                        df_jsx_element(element, file, fn_sym, strings, scope, sink)
                    }
                    Some(ts::JSXAttributeValue::Fragment(fragment)) => {
                        df_jsx_fragment(fragment, file, fn_sym, strings, scope, sink)
                    }
                };
                if !matches!(name.as_str(), "key" | "ref") {
                    fields.push((name, value));
                }
            }
            ts::JSXAttributeItem::SpreadAttribute(spread) => {
                fields.push((
                    "..".to_string(),
                    df_flow_expr(&spread.argument, file, fn_sym, strings, scope, sink),
                ));
            }
        }
    }
    children(
        &el.children,
        file,
        fn_sym,
        strings,
        scope,
        sink,
        &mut fields,
    );
    let span = if component {
        let start = el
            .opening_element
            .attributes
            .first()
            .map(|attribute| attribute.span().start)
            .or_else(|| el.children.first().map(|child| child.span().start))
            .unwrap_or(el.opening_element.name.span().end);
        let end = el
            .children
            .last()
            .map(|child| child.span().end)
            .or_else(|| {
                el.opening_element
                    .attributes
                    .last()
                    .map(|attribute| attribute.span().end)
            })
            .unwrap_or(start);
        oxc_span::Span::new(start, end)
    } else {
        el.span
    };
    let props = element(span, "", fields, strings, sink);
    if !component {
        return props;
    }
    df_owner(sink, strings, eager_mark, fn_sym);
    let body_mark = sink.nodes.len();
    let lam_sym = DfOwner {
        kind: fn_sym.kind,
        name: format!("{}::closure::{}", fn_sym.name, el.span.start),
    };
    let captured = df_push(
        sink,
        strings,
        el.opening_element.name.span(),
        DfNodeKind::VarRead,
        None,
    );
    df_edge(sink, props, captured);
    let call = df_push(
        sink,
        strings,
        el.opening_element.span,
        DfNodeKind::CallRes,
        None,
    );
    df_edge(sink, captured, call);
    sink.aux.args.push(DfArg {
        call,
        pos: 0,
        arg: captured,
    });
    let ret = df_push(
        sink,
        strings,
        el.opening_element.span,
        DfNodeKind::Ret,
        None,
    );
    df_edge(sink, call, ret);
    df_owner(sink, strings, body_mark, &lam_sym);
    df_push(
        sink,
        strings,
        el.span,
        DfNodeKind::Closure,
        Some(&format!("{file}::{}::{}", lam_sym.kind, lam_sym.name)),
    )
}

pub(super) fn df_jsx_fragment(
    fragment: &ts::JSXFragment,
    file: &str,
    fn_sym: &DfOwner,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> NodeRef {
    let mut fields = Vec::new();
    children(
        &fragment.children,
        file,
        fn_sym,
        strings,
        scope,
        sink,
        &mut fields,
    );
    element(fragment.span, "", fields, strings, sink)
}

fn children(
    children: &[ts::JSXChild],
    file: &str,
    fn_sym: &DfOwner,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
    fields: &mut Vec<(String, NodeRef)>,
) {
    for child in children {
        let (name, value) = match child {
            ts::JSXChild::Element(element) => (
                "children",
                df_jsx_element(element, file, fn_sym, strings, scope, sink),
            ),
            ts::JSXChild::Fragment(fragment) => (
                "children",
                df_jsx_fragment(fragment, file, fn_sym, strings, scope, sink),
            ),
            ts::JSXChild::ExpressionContainer(container) => {
                let Some(expression) = container.expression.as_expression() else {
                    continue;
                };
                (
                    "children",
                    df_flow_expr(expression, file, fn_sym, strings, scope, sink),
                )
            }
            ts::JSXChild::Spread(spread) => (
                "..",
                df_flow_expr(&spread.expression, file, fn_sym, strings, scope, sink),
            ),
            ts::JSXChild::Text(text) => {
                if text.value.trim().is_empty() {
                    continue;
                }
                let node = df_push(sink, strings, text.span, DfNodeKind::Lit, None);
                sink.aux.lits.push(DfLit {
                    node,
                    kind: "lit",
                    text: text.value.to_string(),
                });
                ("children", node)
            }
        };
        fields.push((name.to_string(), value));
    }
}

fn element(
    span: oxc_span::Span,
    name: &str,
    fields: Vec<(String, NodeRef)>,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) -> NodeRef {
    let node = df_push(sink, strings, span, DfNodeKind::New, Some(name));
    for (name, value) in fields {
        df_edge(sink, value, node);
        sink.aux.fields.push(DfField {
            owner: node,
            name,
            value,
        });
    }
    node
}
