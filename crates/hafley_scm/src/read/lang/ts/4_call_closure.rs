use super::*;

pub(super) struct LambdaDefs {
    pub(super) deferred_sites: Vec<(oxc_span::Span, oxc_span::Span)>,
    pub(super) out: Vec<oxc_span::Span>,
    pub(super) named: Vec<(oxc_span::Span, String)>,
}

impl<'a> OxcVisit<'a> for LambdaDefs {
    fn visit_jsx_element(&mut self, element: &ts::JSXElement<'a>) {
        if matches!(
            &element.opening_element.name,
            ts::JSXElementName::IdentifierReference(_) | ts::JSXElementName::MemberExpression(_)
        ) {
            self.out.push(element.span);
            self.deferred_sites
                .push((element.opening_element.span, element.span));
        }
        oxc_ast_visit::walk::walk_jsx_element(self, element);
    }

    fn visit_object_property(&mut self, prop: &ts::ObjectProperty<'a>) {
        use ts::PropertyKey as K;
        if let Some(name) = match &prop.key {
            K::StaticIdentifier(id) => Some(id.name.to_string()),
            _ => None,
        } {
            if let Some(body) = lambda_value_span(&prop.value) {
                self.named.push((body, name));
            }
        }
        oxc_ast_visit::walk::walk_object_property(self, prop);
    }

    fn visit_arrow_function_expression(&mut self, arrow: &ts::ArrowFunctionExpression<'a>) {
        self.out.push(arrow.span);
        oxc_ast_visit::walk::walk_arrow_function_expression(self, arrow);
    }
    fn visit_function(&mut self, func: &ts::Function<'a>, flags: oxc_syntax::scope::ScopeFlags) {
        if func.r#type == ts::FunctionType::FunctionExpression && func.body.is_some() {
            self.out.push(func.span);
            if let Some(id) = &func.id {
                self.named.push((func.span, id.name.to_string()));
            }
        }
        oxc_ast_visit::walk::walk_function(self, func, flags);
    }
    fn visit_variable_declarator(&mut self, declarator: &ts::VariableDeclarator<'a>) {
        if matches!(&declarator.id, ts::BindingPattern::BindingIdentifier(_)) {
            match &declarator.init {
                Some(ts::Expression::ArrowFunctionExpression(arrow)) => {
                    self.visit_arrow_function_body(&arrow.body);
                    return;
                }
                Some(ts::Expression::FunctionExpression(func)) => {
                    if let Some(body) = func.body.as_deref() {
                        self.visit_function_body(body);
                    }
                    return;
                }
                _ => {}
            }
        }
        oxc_ast_visit::walk::walk_variable_declarator(self, declarator);
    }
    fn visit_assignment_pattern(&mut self, _pattern: &ts::AssignmentPattern<'a>) {}
}
