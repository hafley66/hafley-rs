use super::*;

pub(super) fn targets(
    caller: &FamilyBundle<DfF>,
    arg: NodeRef,
    callee: &FamilyBundle<DfF>,
    param: NodeRef,
    definition: Span,
    strings: &Strings,
) -> (bool, Vec<(NodeRef, NodeRef)>) {
    let objects = objects(caller, arg);
    let field = callee
        .aux
        .param_fields
        .iter()
        .find(|(node, _)| *node == param)
        .map(|(_, field)| field.as_str());
    let mut targets = Vec::new();
    for object in &objects {
        for value in &caller.aux.fields {
            if value.owner != *object || value.name == ".." {
                continue;
            }
            if field == Some(value.name.as_str()) {
                targets.push((value.value, param));
            }
            if field.is_some() {
                continue;
            }
            for (index, node) in callee.nodes.iter().enumerate() {
                let target = NodeRef(index as u32);
                if node.kind != DfNodeKind::Member
                    || !field_span_in_definition(node.span, definition)
                    || node.name.map(|name| strings.lookup(name)) != Some(value.name.as_str())
                {
                    continue;
                }
                if callee
                    .edges
                    .iter()
                    .filter(|edge| edge.dst == target)
                    .any(|edge| parameter_object(callee, edge.src, param, definition))
                {
                    targets.push((value.value, target));
                }
            }
        }
    }
    (field.is_none() || objects.is_empty(), targets)
}

fn field_span_in_definition(span: Span, definition: Span) -> bool {
    definition.start <= span.start && span.end() <= definition.end()
}

// Resolve only identity reads/bindings and explicit object spreads.
fn objects(df: &FamilyBundle<DfF>, arg: NodeRef) -> Vec<NodeRef> {
    let mut pending = vec![arg];
    let mut seen = std::collections::HashSet::new();
    let mut objects = Vec::new();
    while let Some(node) = pending.pop() {
        if !seen.insert(node) {
            continue;
        }
        match df.node(node).kind {
            DfNodeKind::New => {
                objects.push(node);
                pending.extend(
                    df.aux
                        .fields
                        .iter()
                        .filter(|field| field.owner == node && field.name == "..")
                        .map(|field| field.value),
                );
            }
            DfNodeKind::VarRead | DfNodeKind::LetBind => pending.extend(
                df.edges
                    .iter()
                    .filter(|edge| edge.dst == node)
                    .map(|edge| edge.src),
            ),
            _ => {}
        }
    }
    objects
}

fn parameter_object(
    df: &FamilyBundle<DfF>,
    mut node: NodeRef,
    param: NodeRef,
    definition: Span,
) -> bool {
    let mut seen = std::collections::HashSet::new();
    while seen.insert(node) {
        if node == param {
            return true;
        }
        let value = df.node(node);
        if !field_span_in_definition(value.span, definition)
            || !matches!(value.kind, DfNodeKind::VarRead | DfNodeKind::LetBind)
        {
            return false;
        }
        let Some(edge) = df.edges.iter().find(|edge| edge.dst == node) else {
            return false;
        };
        node = edge.src;
    }
    false
}
