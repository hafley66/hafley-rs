//! GDScript extraction over the tree-sitter-gdscript grammar.
//!
//! One plane: CstF, the same shape html/css take through `AstgrepSource`. Godot's
//! grammar is not in ast-grep's `SupportLang` roster, so this source owns the
//! parse: one `Node` per named node, one `Child` edge to the nearest named
//! ancestor, spans file-relative. No type/call/df/data plane exists for GDScript
//! here, and no phase-2 leg is claimed (no `Resolve`, `Rehome`, `Rename`, or cfg
//! role table): every such roster names this language by absence, never by a
//! wrong row.

use crate::family::{CstEdgeKind, CstF};
use crate::lang::extract_lang::RyiLang;
use crate::rows::{Edge, FamilyBundle, Node};
use crate::shape::{NodeRef, Span, Strings};
use crate::source::{RyiOutput, FamilyMask, Source};
use crate::trace;

#[derive(Default)]
pub struct GdscriptSource;

fn parse(content: &[u8]) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    let language = tree_sitter::Language::new(tree_sitter_gdscript::LANGUAGE);
    parser.set_language(&language).ok()?;
    parser.parse(content, None)
}

fn span(node: tree_sitter::Node) -> Span {
    Span {
        start: node.start_byte() as u32,
        len: (node.end_byte() - node.start_byte()) as u32,
    }
}

/// Pre-order, named nodes only: unnamed punctuation passes its nearest named
/// ancestor through, so every edge lands parent-adjacent. The same walk
/// `AstgrepSource`'s `CstProjector` runs over an ast-grep doc.
fn project_cst(root: tree_sitter::Node, strings: &mut Strings, sink: &mut FamilyBundle<CstF>) {
    let mut stack = vec![(root, None)];
    while let Some((node, parent)) = stack.pop() {
        let current = if node.is_named() {
            let node_ref = NodeRef(sink.nodes.len() as u32);
            sink.nodes
                .push(Node::new(span(node), strings.intern(node.kind())));
            if let Some(parent) = parent {
                sink.edges
                    .push(Edge::new(parent, node_ref, CstEdgeKind::Child));
            }
            Some(node_ref)
        } else {
            parent
        };
        let mut children: Vec<_> = node.children(&mut node.walk()).collect();
        children.reverse();
        for child in children {
            stack.push((child, current));
        }
    }
}

impl Source for GdscriptSource {
    fn name(&self) -> &'static str {
        "gdscript"
    }

    fn matches(&self, path: &str) -> bool {
        path.ends_with(".gd")
    }

    fn extract_lang(&self, _path: &str) -> Option<RyiLang> {
        Some(RyiLang::Gdscript)
    }

    fn extract(&self, _path: &str, content: &[u8], mask: FamilyMask) -> RyiOutput {
        let mut output = RyiOutput::default();
        if std::str::from_utf8(content).is_err() {
            return output;
        }
        let tree = {
            let span = trace::parse_span("gdscript", "tree-sitter");
            let _entered = span.enter();
            parse(content)
        };
        let Some(tree) = tree else {
            return output;
        };
        if mask.cst {
            let span = trace::family_span("gdscript", "cst");
            let _entered = span.enter();
            let mut bundle = FamilyBundle::<CstF>::default();
            project_cst(tree.root_node(), &mut output.strings, &mut bundle);
            trace::record_bundle(&span, &bundle, 0);
            output.cst = Some(bundle);
        }
        output
    }
}