use super::*;

pub(super) struct LambdaDefs {
    pub(super) deferred_sites: Vec<(oxc_span::Span, oxc_span::Span)>,
    pub(super) out: Vec<oxc_span::Span>,
    /// Arrows / fn-exprs that are an object-literal property's value: the
    /// oracle names the enclosing callable by that property
    /// (`getAllCodeActions: context => codeFixAll(...)`), so the lambda def
    /// carries the property name and its sites name it as their caller.
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
        // A function EXPRESSION is an anonymous callable -> a Lambda def
        // (bodiless ones lift no body and mint no closure node in v5). Fn
        // declarations (CallWalker's nested Free defs) and method values
        // (Method defs) are not lambdas.
        if func.r#type == ts::FunctionType::FunctionExpression && func.body.is_some() {
            self.out.push(func.span);
            // A fn-expr with its own identifier (`[K.A]: function forEachChildInX
            // (...) {}`, `setTimeout(function handler() {})`) is named by that
            // identifier; the oracle calls sites inside it by this name.
            if let Some(id) = &func.id {
                self.named.push((func.span, id.name.to_string()));
            }
        }
        oxc_ast_visit::walk::walk_function(self, func, flags);
    }
    fn visit_variable_declarator(&mut self, declarator: &ts::VariableDeclarator<'a>) {
        // `const f = (...) => ...` / `const f = function () {}`: the bound
        // value is a named callable, not a lambda — skip its def, walk its
        // body for nested inline lambdas.
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
    fn visit_assignment_pattern(&mut self, _pattern: &ts::AssignmentPattern<'a>) {
        // Param/destructuring defaults hold no df-covered values in v5.
    }
}
