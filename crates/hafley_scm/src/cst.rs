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
pub fn kind_name(language: &tree_sitter::Language, kind_id: u16) -> &'static str {
    language.node_kind_for_id(kind_id).unwrap_or("")
}

/// The grammar's kind id for a kind name, or `0` — the absent mark, the same
/// convention `Language::kind_to_id` uses. Lets a caller resolve its kind
/// tables once per file, never per node.
pub fn kind_id(language: &tree_sitter::Language, kind: &str) -> u16 {
    language.id_for_node_kind(kind, true)
}

/// Pre-order named-node walk with nearest-named-ancestor links. Iterative on an
/// explicit stack — recursion would die on deep trees. Children are pushed in
/// reverse so they pop in source order.
pub fn walk_named(tree: &Tree) -> Vec<CstRow> {
    let mut out = Vec::new();
    let root = tree.root_node();
    let mut cursor = tree.walk();
    let mut stack: Vec<(tree_sitter::Node<'_>, Option<u32>)> = vec![(root, None)];
    while let Some((node, nearest_named)) = stack.pop() {
        let my_ix = if node.is_named() {
            let ix = out.len() as u32;
            let name = node
                .child_by_field_name("name")
                .map(|field| (field.start_byte() as u32, field.end_byte() as u32));
            out.push(CstRow {
                kind_id: node.kind_id(),
                start: node.start_byte() as u32,
                end: node.end_byte() as u32,
                name,
                named_children: node.named_child_count() as u16,
                parent: nearest_named,
            });
            Some(ix)
        } else {
            nearest_named
        };
        let mark = stack.len();
        for child in node.children(&mut cursor) {
            stack.push((child, my_ix));
        }
        stack[mark..].reverse();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(&b"fn main() { spark(); }"[ns as usize..ne as usize], b"main");
        assert_eq!(main.parent, Some(0));
        assert_eq!(rows[0].parent, None);
        let call = rows
            .iter()
            .position(|row| kind_name(&rust, row.kind_id) == "call_expression")
            .unwrap();
        assert_eq!(rows[call].parent, Some(5), "call reparents to its NEAREST named ancestor (expression_statement), not the root");
        assert_eq!(rows[call].name, None, "call_expression has no name field");
    }
}
