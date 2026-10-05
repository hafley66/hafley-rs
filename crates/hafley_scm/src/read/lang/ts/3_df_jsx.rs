use super::*;

pub(super) fn df_jsx_element(
    el: &ts::JSXElement,
    file: &str,
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> NodeRef {
    use ts::JSXElementName as N;
    let name = match &el.opening_element.name {
        N::Identifier(id) => id.name.to_string(),
        N::IdentifierReference(id) => id.name.to_string(),
        N::MemberExpression(member) => member.property.name.to_string(),
        N::NamespacedName(name) => name.name.name.to_string(),
        N::ThisExpression(_) => String::new(),
    };
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
                        df_push(sink, strings, value.span, DfNodeKind::Lit, None)
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
                fields.push((name, value));
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
    element(el.span, &name, fields, strings, sink)
}

pub(super) fn df_jsx_fragment(
    fragment: &ts::JSXFragment,
    file: &str,
    fn_sym: &str,
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
    fn_sym: &str,
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
            ts::JSXChild::Text(_) => continue,
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
