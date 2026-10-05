use super::*;

pub(super) fn emit(
    blob: &ContentId,
    output: &RyiOutput,
    df: &FamilyBundle<DfF>,
    edges: &mut Vec<FlowEdge>,
) {
    let closures: Vec<_> = df
        .nodes
        .iter()
        .filter(|node| node.kind == DfNodeKind::Closure)
        .collect();
    for edge in &df.edges {
        if df.node(edge.dst).kind != DfNodeKind::VarRead {
            continue;
        }
        let owners = (
            df.aux.functions.get(edge.src.0 as usize).copied().flatten(),
            df.aux.functions.get(edge.dst.0 as usize).copied().flatten(),
        );
        let capture = match owners {
            (Some(from), Some(to)) => from != to,
            _ => closures.iter().any(|closure| {
                contains(closure.span, df.node(edge.dst).span)
                    && !contains(closure.span, df.node(edge.src).span)
            }),
        };
        if capture {
            edges.push(FlowEdge {
                src_blob: blob.clone(),
                src_span: df.node(edge.src).span,
                dst_blob: blob.clone(),
                dst_span: df.node(edge.dst).span,
                kind: FlowEdgeKind::LambdaElem,
            });
        }
    }
    for (index, node) in df
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.kind == DfNodeKind::Ret)
    {
        let Some(closure) = closures
            .iter()
            .filter(|closure| contains(closure.span, node.span))
            .min_by_key(|closure| closure.span.end() - closure.span.start)
        else {
            continue;
        };
        // JSX props are eager despite lying inside the element's source span.
        // Body ownership distinguishes those values from the deferred return.
        let owner = df
            .aux
            .functions
            .get(index)
            .copied()
            .flatten()
            .map(|id| output.strings.lookup(id));
        if let (Some(owner), Some(name)) = (owner, closure.name.map(|id| output.strings.lookup(id)))
        {
            if !name.ends_with(owner) {
                continue;
            }
        }
        edges.push(FlowEdge {
            src_blob: blob.clone(),
            src_span: node.span,
            dst_blob: blob.clone(),
            dst_span: closure.span,
            kind: FlowEdgeKind::LambdaRet,
        });
    }
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.start <= inner.start && inner.end() <= outer.end()
}
