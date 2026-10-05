use super::*;

/// Explicit deferred sites bind to their closure; eager sites select the
/// tightest covering definition, excluding deferred-only closure spans.
pub fn covering_def(defs: &FamilyBundle<CallF>, site: Span) -> Option<NodeRef> {
    if let Some((_, owner)) = defs
        .aux
        .deferred_sites
        .iter()
        .find(|(call, _)| *call == site)
    {
        return defs
            .nodes
            .iter()
            .position(|node| node.kind == CallKind::Lambda && node.span == *owner)
            .map(|index| NodeRef(index as u32));
    }
    // One linear pass for the tightest cover, no sort and no allocation. The
    // previous form sorted the whole bundle per call; ties break the same way
    // the sorted order did (min length, then min (start, end), then node order).
    let mut best: Option<(Span, NodeRef)> = None;
    for (ix, node) in defs.nodes.iter().enumerate() {
        let span = node.span;
        if defs
            .aux
            .deferred_sites
            .iter()
            .any(|(_, deferred)| *deferred == span)
        {
            continue;
        }
        if span.start > site.start || site.end() > span.end() {
            continue;
        }
        let key = (span.end() - span.start, span.start, span.end());
        let better = match best {
            None => true,
            Some((b, _)) => {
                let bkey = (b.end() - b.start, b.start, b.end());
                key < bkey
            }
        };
        if better {
            best = Some((span, NodeRef(ix as u32)));
        }
    }
    best.map(|(_, r)| r)
}
