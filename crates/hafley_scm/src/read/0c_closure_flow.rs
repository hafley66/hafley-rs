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
                closure.span.contains(df.node(edge.dst).span)
                    && !closure.span.contains(df.node(edge.src).span)
            }),
        };
        if capture {
            edges.push(FlowEdge::new(
                (blob, df.node(edge.src).span),
                (blob, df.node(edge.dst).span),
                FlowEdgeKind::LambdaElem,
            ));
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
            .filter(|closure| closure.span.contains(node.span))
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
        edges.push(FlowEdge::new(
            (blob, node.span),
            (blob, closure.span),
            FlowEdgeKind::LambdaRet,
        ));
    }
}
