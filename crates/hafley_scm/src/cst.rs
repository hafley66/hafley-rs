//! Whole-tree CST read: the named-node walk the source facts and guessed call
//! planes project from. tree-sitter owns the grammar and the parse; this module
//! is the reusable read primitive over the parsed tree — no grammar policy, no
//! string interning, no predicate evaluation. Those live with the caller.

use tree_sitter::Tree;

/// One named node of a parsed tree, in pre-order. `parent` indexes the nearest
/// NAMED ancestor (`None` at roots); unnamed nodes emit no row but hand their
/// named descendants to that ancestor — the reparent rule the source facts'
/// child edges encode. `name` is the byte span of the grammar's `name:` field
/// when the node kind declares one, and `named_children` counts direct named
/// children (zero = the leaf test a caller's name-leaf heuristic needs).
pub struct CstRow {
    pub kind_id: u16,
    pub start: u32,
    pub end: u32,
    pub name: Option<(u32, u32)>,
    pub named_children: u16,
    pub parent: Option<u32>,
}

/// Parse `src` under `language`. `None` when the parser cannot be configured or
/// reports no tree at all; a tree with ERROR nodes is still a tree.
pub fn parse(language: &tree_sitter::Language, src: &[u8]) -> Option<Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(language).ok()?;
    parser.parse(src, None)
}

/// The grammar's spelling of a kind id (`"identifier"`, `"call_expression"`,
/// ...), for callers that intern kind text.
pub fn kind_name(language: &tree_sitter::Language, kind_id: u16) -> &str {
    language.node_kind_for_id(kind_id).unwrap_or("")
}

/// The grammar's kind id for a kind name, or `0` — the absent mark, the same
/// convention `Language::kind_to_id` uses. Lets a caller resolve its kind
/// tables once per file, never per node.
pub fn kind_id(language: &tree_sitter::Language, kind: &str) -> u16 {
    language.id_for_node_kind(kind, true)
}

/// Where a node sits under its parent: grammar field id, child position, position among named children.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub field: Option<u16>,
    pub index: u32,
    pub named_index: Option<u32>,
}

/// Pre-order over every node, named and anonymous, on one `TreeCursor`. `visit` gets the node,
/// the value it returned for the parent (`None` at the root) and the node's slot.
pub fn walk_streaming<'t, T: Copy>(
    tree: &'t Tree,
    mut visit: impl FnMut(tree_sitter::Node<'t>, Option<T>, Slot) -> T,
) {
    let mut cursor = tree.walk();
    let root = visit(cursor.node(), None, Slot { field: None, index: 0, named_index: None });
    // (parent value, next child index, next named index), one frame per open ancestor.
    let mut stack = vec![(root, 0u32, 0u32)];
    if !cursor.goto_first_child() {
        return;
    }
    loop {
        let node = cursor.node();
        let frame = stack.last_mut().expect("an open parent");
        let index = frame.1;
        frame.1 += 1;
        let named_index = node.is_named().then(|| {
            frame.2 += 1;
            frame.2 - 1
        });
        let field = cursor.field_id().map(|id| id.get());
        let slot = Slot { field, index, named_index };
        let value = visit(node, Some(frame.0), slot);
        if cursor.goto_first_child() {
            stack.push((value, 0, 0));
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return;
            }
            stack.pop();
        }
    }
}

/// Named nodes only, handed to `sink` as `(ordinal, parent ordinal, kind id, start, end, name
/// span, named-children count)`; unnamed nodes reparent their named descendants upward.
pub fn walk_named_streaming(
    tree: &Tree,
    mut sink: impl FnMut(u32, Option<u32>, u16, u32, u32, Option<(u32, u32)>, u16),
) {
    let mut ordinal: u32 = 0;
    walk_streaming(tree, |node, parent: Option<Option<u32>>, _slot| {
        let nearest_named = parent.flatten();
        if !node.is_named() {
            return nearest_named;
        }
        let ix = ordinal;
        ordinal += 1;
        let name = node
            .child_by_field_name("name")
            .map(|field| (field.start_byte() as u32, field.end_byte() as u32));
        sink(
            ix,
            nearest_named,
            node.kind_id(),
            node.start_byte() as u32,
            node.end_byte() as u32,
            name,
            node.named_child_count() as u16,
        );
        Some(ix)
    });
}

/// The whole walk as a `Vec` — the convenience shape. Production callers
/// should prefer [`walk_named_streaming`], which never holds two copies of
/// the tree's rows at once.
pub fn walk_named(tree: &Tree) -> Vec<CstRow> {
    let mut out = Vec::new();
    walk_named_streaming(
        tree,
        |ix, parent, kind_id, start, end, name, named_children| {
            out.push(CstRow {
                kind_id,
                start,
                end,
                name,
                named_children,
                parent,
            });
            let _ = ix;
        },
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_streaming_visits_every_node_with_its_slot() {
        let rust: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let tree = parse(&rust, b"fn main() { spark(); }").expect("parses");
        let mut rows = Vec::new();
        walk_streaming(&tree, |node, depth: Option<usize>, slot| {
            let depth = depth.map_or(0, |depth| depth + 1);
            let field = slot.field.and_then(|id| rust.field_name_for_id(id)).unwrap_or("-");
            let named = slot.named_index.map_or("-".to_string(), |n| n.to_string());
            rows.push(format!("{}{} {field} {} {named}", "  ".repeat(depth), node.kind(), slot.index));
            depth
        });
        assert_eq!(
            rows.join("\n"),
            "source_file - 0 -\n  function_item - 0 0\n    fn - 0 -\n    identifier name 1 0\n    parameters parameters 2 1\n      ( - 0 -\n      ) - 1 -\n    block body 3 2\n      { - 0 -\n      expression_statement - 1 0\n        call_expression - 0 0\n          identifier function 0 0\n          arguments arguments 1 1\n            ( - 0 -\n            ) - 1 -\n        ; - 1 -\n      } - 2 -"
        );
    }

    #[test]
    fn walk_names_anchors_and_reparents_through_unnamed() {
        let rust: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let tree = parse(&rust, b"fn main() { spark(); }").expect("parses");
        let rows = walk_named(&tree);
        let kinds: Vec<&str> = rows
            .iter()
            .map(|row| kind_name(&rust, row.kind_id))
            .collect();
        assert_eq!(
            kinds,
            vec![
                "source_file",
                "function_item",
                "identifier",
                "parameters",
                "block",
                "expression_statement",
                "call_expression",
                "identifier",
                "arguments",
            ]
        );
        let main = &rows[1];
        assert_eq!(kind_name(&rust, main.kind_id), "function_item");
        let (ns, ne) = main.name.expect("function_item names its name field");
        assert_eq!(
            &b"fn main() { spark(); }"[ns as usize..ne as usize],
            b"main"
        );
        assert_eq!(main.parent, Some(0));
        assert_eq!(rows[0].parent, None);
        let call = rows
            .iter()
            .position(|row| kind_name(&rust, row.kind_id) == "call_expression")
            .unwrap();
        assert_eq!(
            rows[call].parent,
            Some(5),
            "call reparents to its NEAREST named ancestor (expression_statement), not the root"
        );
        assert_eq!(rows[call].name, None, "call_expression has no name field");
    }
}
