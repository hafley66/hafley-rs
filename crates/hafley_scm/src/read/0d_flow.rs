use super::*;

#[path = "0c_arg_fields.rs"]
mod arguments;
#[path = "0c_closure_flow.rs"]
mod closures;

// ── VALUE-FLOW plane: FlowF  (inter-procedural value flow) ───────────────────

/// Cross-function value flow, a separate family from `DfF`. Phase-2 only: no
/// `FamilyMask` bit, no `RyiOutput` field; a pure join computes its edges.
#[derive(Default, Copy, Clone, Debug)]
pub struct FlowF;

/// Cross-function value edge kind. `DfDirect` is absent: that is DfF's plane.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FlowEdgeKind {
    /// A caller argument value flows into the callee's parameter at the same
    /// positional slot.
    ArgToParam,
    /// A callee return value reaches the caller's call-result node. The edge is
    /// caller-local, so the VALUE travels dst to src for this kind alone.
    RetToCallRes,
    /// A captured value flows into the closure's element slot.
    LambdaElem,
    /// A closure's return value flows out to the closure node.
    LambdaRet,
}

impl FlowEdgeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            FlowEdgeKind::ArgToParam => "arg_to_param",
            FlowEdgeKind::RetToCallRes => "ret_to_call_res",
            FlowEdgeKind::LambdaElem => "lambda_elem",
            FlowEdgeKind::LambdaRet => "lambda_ret",
        }
    }
}

impl Family for FlowF {
    type NodeKind = DfNodeKind;
    type EdgeKind = FlowEdgeKind;
    type Aux = ();
    const TAG: FamilyTag = FamilyTag::Flow;
}

/// One cross-function value-flow edge, BOTH endpoints (blob, span) because flow
/// crosses files. Emitted only by the `flow_edges` join.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowEdge {
    pub src_blob: ContentId,
    pub src_span: Span,
    pub dst_blob: ContentId,
    pub dst_span: Span,
    pub kind: FlowEdgeKind,
}

/// The pure inter-procedural value-flow join: `DfArg` x resolved call edge x
/// `DfParam` (ArgToParam) plus callee `Ret` nodes (RetToCallRes).
pub fn flow_edges(
    inputs: &[(ContentId, &RyiOutput)],
    resolved: &[(ContentId, Vec<ProjectEdge<CallF>>)],
) -> Vec<FlowEdge> {
    let by_blob: std::collections::HashMap<ContentId, &RyiOutput> = inputs
        .iter()
        .map(|(blob, out)| (blob.clone(), *out))
        .collect();
    let mut edges = Vec::new();
    for (blob, output) in inputs {
        if let Some(df) = &output.df {
            closures::emit(blob, output, df, &mut edges);
        }
    }
    for (caller_blob, call_edges) in resolved {
        let Some(caller) = by_blob.get(caller_blob) else {
            continue;
        };
        let Some(caller_df) = caller.df.as_ref() else {
            continue;
        };
        for call_edge in call_edges {
            let Some(site) = call_edge.call_site else {
                continue;
            };
            let Some(call_node) = call_node(caller_df, site) else {
                continue;
            };
            let Some(callee) = by_blob.get(&call_edge.dst_blob) else {
                continue;
            };
            let Some(callee_df) = callee.df.as_ref() else {
                continue;
            };
            for arg in &caller_df.aux.args {
                if arg.call != call_node || arg.pos < 0 {
                    continue;
                }
                for param in &callee_df.aux.params {
                    let param_span = callee_df.node(param.node).span;
                    let in_callee = call_edge.dst_span.start <= param_span.start
                        && param_span.end() <= call_edge.dst_span.end();
                    if !in_callee || param.pos as i64 != arg.pos {
                        continue;
                    }
                    let (whole, fields) = arguments::targets(
                        caller_df,
                        arg.arg,
                        callee_df,
                        param.node,
                        call_edge.dst_span,
                        &callee.strings,
                    );
                    if whole {
                        edges.push(FlowEdge {
                            src_blob: caller_blob.clone(),
                            src_span: caller_df.node(arg.arg).span,
                            dst_blob: call_edge.dst_blob.clone(),
                            dst_span: param_span,
                            kind: FlowEdgeKind::ArgToParam,
                        });
                    }
                    for (value, target) in fields {
                        edges.push(FlowEdge {
                            src_blob: caller_blob.clone(),
                            src_span: caller_df.node(value).span,
                            dst_blob: call_edge.dst_blob.clone(),
                            dst_span: callee_df.node(target).span,
                            kind: FlowEdgeKind::ArgToParam,
                        });
                    }
                }
            }
            let call_span = caller_df.node(call_node).span;
            for node in &callee_df.nodes {
                if node.kind != DfNodeKind::Ret {
                    continue;
                }
                let in_callee = call_edge.dst_span.start <= node.span.start
                    && node.span.end() <= call_edge.dst_span.end();
                if !in_callee {
                    continue;
                }
                edges.push(FlowEdge {
                    src_blob: caller_blob.clone(),
                    src_span: call_span,
                    dst_blob: call_edge.dst_blob.clone(),
                    dst_span: node.span,
                    kind: FlowEdgeKind::RetToCallRes,
                });
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    edges.retain(|edge| {
        seen.insert((
            edge.src_blob.clone(),
            edge.src_span,
            edge.dst_blob.clone(),
            edge.dst_span,
            edge.kind.as_str(),
        ))
    });
    edges
}

/// The caller's call node at `site`: the `CallRes`/`New` node whose span equals
/// the site, else the smallest such span containing it, else `None`.
fn call_node(bundle: &FamilyBundle<DfF>, site: Span) -> Option<NodeRef> {
    let is_call = |kind: DfNodeKind| matches!(kind, DfNodeKind::CallRes | DfNodeKind::New);
    for (index, node) in bundle.nodes.iter().enumerate() {
        if is_call(node.kind) && node.span == site {
            return Some(NodeRef(index as u32));
        }
    }
    let mut best: Option<(Span, NodeRef)> = None;
    for (index, node) in bundle.nodes.iter().enumerate() {
        if !is_call(node.kind) {
            continue;
        }
        let contains = node.span.start <= site.start && site.end() <= node.span.end();
        let tighter = best.map_or(true, |(span, _)| {
            node.span.end() - node.span.start < span.end() - span.start
        });
        if contains && tighter {
            best = Some((node.span, NodeRef(index as u32)));
        }
    }
    best.map(|(_, node)| node)
}
