use super::*;
use hafley_scm::lang::rust::{df_syntax_rows, DfSyntaxKind};

/// Materialize Rust syntax-flow rows into ryi's shared DfF graph.
pub(super) fn project_df(
    parsed: &syn::File,
    file: &str,
    src: &str,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    let rows = df_syntax_rows(parsed, file, src, line_starts);
    for row in rows.nodes {
        let kind = match row.kind {
            DfSyntaxKind::Param => DfNodeKind::Param,
            DfSyntaxKind::LetBind => DfNodeKind::LetBind,
            DfSyntaxKind::VarRead => DfNodeKind::VarRead,
            DfSyntaxKind::VarWrite => DfNodeKind::VarWrite,
            DfSyntaxKind::Lit => DfNodeKind::Lit,
            DfSyntaxKind::CallRes => DfNodeKind::CallRes,
            DfSyntaxKind::New => DfNodeKind::New,
            DfSyntaxKind::Member => DfNodeKind::Member,
            DfSyntaxKind::Ret => DfNodeKind::Ret,
            DfSyntaxKind::Binop => DfNodeKind::Binop,
            DfSyntaxKind::Unop => DfNodeKind::Unop,
            DfSyntaxKind::Loop => DfNodeKind::Loop,
            DfSyntaxKind::If => DfNodeKind::If,
            DfSyntaxKind::Closure => DfNodeKind::Closure,
            DfSyntaxKind::Expr => DfNodeKind::Expr,
            DfSyntaxKind::Borrow => BORROW,
            DfSyntaxKind::Break => BREAK,
            DfSyntaxKind::Match => MATCH,
            DfSyntaxKind::Block => BLOCK,
        };
        let mut node = Node::new(Span { start: row.span.start, len: row.span.len }, kind);
        if let Some(name) = row.name {
            node = node.with_name(strings.intern(&name));
        }
        sink.nodes.push(node);
    }
    for edge in rows.edges {
        sink.edges.push(Edge::new(
            NodeRef(edge.src.0),
            NodeRef(edge.dst.0),
            DfEdgeKind::Direct,
        ));
    }
    sink.aux.params = rows.aux.params.into_iter().map(|row| DfParam {
        node: NodeRef(row.node.0),
        pos: row.pos,
    }).collect();
    sink.aux.args = rows.aux.args.into_iter().map(|row| DfArg {
        call: NodeRef(row.call.0),
        pos: row.pos,
        arg: NodeRef(row.arg.0),
    }).collect();
    sink.aux.fields = rows.aux.fields.into_iter().map(|row| DfField {
        owner: NodeRef(row.owner.0),
        name: row.name,
        value: NodeRef(row.value.0),
    }).collect();
    sink.aux.lits = rows.aux.lits.into_iter().map(|row| DfLit {
        node: NodeRef(row.node.0),
        kind: row.kind,
        text: row.text,
    }).collect();
    sink.aux.loops = rows.aux.loops.into_iter().map(|row| crate::types::DfLoop {
        span: Span { start: row.span.start, len: row.span.len },
        var: row.var,
        collection: row.collection,
    }).collect();
    sink.aux.allocates = rows.aux.allocates.into_iter().map(|row| crate::types::DfAllocates {
        owner: Span { start: row.owner.start, len: row.owner.len },
    }).collect();
    sink.aux.allocator_hits = rows.aux.allocator_hits.into_iter().map(|span| Span {
        start: span.start,
        len: span.len,
    }).collect();
    sink.aux.nests = crate::types::compute_nests(&sink.nodes, &sink.aux.loops);
}
