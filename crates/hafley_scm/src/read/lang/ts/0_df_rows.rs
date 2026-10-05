use super::*;

pub(super) fn df_seed_params(
    params: &ts::FormalParameters,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    for (pos, param) in params.items.iter().enumerate() {
        match &param.pattern {
            ts::BindingPattern::BindingIdentifier(binding) => {
                let name = binding.name.to_string();
                let node = df_push(sink, strings, param.span, DfNodeKind::Param, Some(&name));
                sink.aux.params.push(DfParam {
                    node,
                    pos: pos as u32,
                });
                scope.insert(name, node);
            }
            ts::BindingPattern::ObjectPattern(object) => {
                for property in &object.properties {
                    if let ts::BindingPattern::BindingIdentifier(binding) = &property.value {
                        let key = match &property.key {
                            ts::PropertyKey::StaticIdentifier(ident) => ident.name.to_string(),
                            ts::PropertyKey::StringLiteral(string) => string.value.to_string(),
                            _ => binding.name.to_string(),
                        };
                        let node =
                            df_push(sink, strings, binding.span, DfNodeKind::Param, Some(&key));
                        sink.aux.params.push(DfParam {
                            node,
                            pos: pos as u32,
                        });
                        sink.aux.param_fields.push((node, key));
                        scope.insert(binding.name.to_string(), node);
                    }
                }
                if let Some(rest) = &object.rest {
                    if let ts::BindingPattern::BindingIdentifier(binding) = &rest.argument {
                        let name = binding.name.to_string();
                        let node =
                            df_push(sink, strings, binding.span, DfNodeKind::Param, Some(&name));
                        sink.aux.params.push(DfParam {
                            node,
                            pos: pos as u32,
                        });
                        scope.insert(name, node);
                    }
                }
            }
            _ => {}
        }
    }
}

pub(super) fn df_push(
    sink: &mut FamilyBundle<DfF>,
    strings: &mut Strings,
    node_span: oxc_span::Span,
    kind: DfNodeKind,
    var: Option<&str>,
) -> NodeRef {
    let node_ref = NodeRef(sink.nodes.len() as u32);
    let mut node = Node::new(to_span(node_span), kind);
    if let Some(name) = var.filter(|name| !name.is_empty()) {
        node = node.with_name(strings.intern(name));
    }
    sink.nodes.push(node);
    node_ref
}

pub(super) fn df_edge(sink: &mut FamilyBundle<DfF>, src: NodeRef, dst: NodeRef) {
    sink.edges.push(Edge::new(src, dst, DfEdgeKind::Direct));
}

pub(super) fn df_loop_row(
    sink: &mut FamilyBundle<DfF>,
    loop_span: oxc_span::Span,
    var: Option<String>,
    collection: Option<oxc_span::Span>,
) {
    let index = sink.aux.loops.len();
    sink.aux.loops.push(crate::read::types::DfLoop {
        span: to_span(loop_span),
        var,
        collection: None,
    });
    if let Some(collection) = collection {
        sink.aux
            .loop_collection_spans
            .push((index, collection.start, collection.end));
    }
}

pub(super) fn df_owner(
    sink: &mut FamilyBundle<DfF>,
    strings: &mut Strings,
    mark: usize,
    owner: &DfOwner,
) {
    let owner = strings.intern(&owner.name);
    sink.aux.functions.resize(sink.nodes.len(), None);
    for slot in &mut sink.aux.functions[mark..] {
        if slot.is_none() {
            *slot = Some(owner);
        }
    }
}
