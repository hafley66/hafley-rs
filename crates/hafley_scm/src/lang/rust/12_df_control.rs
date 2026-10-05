//! Rust control-expression flow over the shared CST.

use super::*;

impl TreeDf<'_, '_> {
    pub(super) fn break_expr(
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

    pub(super) fn for_expr(
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

    pub(super) fn while_expr(
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

    pub(super) fn loop_expr(
        &mut self,
        expr: tree_sitter::Node<'_>,
        fn_sym: &str,
        scope: &mut Scope,
        loops: &mut LoopBreaks,
    ) -> NodeRef {
        self.loop_row(expr, None, None);
        let label = tree_children(expr)
            .into_iter()
            .find(|child| child.kind() == "label")
            .map(|label| {
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

    pub(super) fn loop_row(
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

    pub(super) fn if_expr(
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

    pub(super) fn match_expr(
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

    pub(super) fn block_expr(
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

    pub(super) fn closure_expr(
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
}
