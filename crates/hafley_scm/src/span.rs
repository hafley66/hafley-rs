use std::borrow::Borrow;

use serde::Serialize;

/// THE one coordinate. Byte offsets into the file; line/col derived, never stored.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Span {
    pub start: u32,
    pub len: u32,
}

impl Span {
    pub const fn empty() -> Self {
        Self { start: 0, len: 0 }
    }
    /// Synthetic identity for things with no real span (a whole-file module).
    pub const fn anchor(at: u32) -> Self {
        Self { start: at, len: 0 }
    }
    pub const fn contains(self, inner: Self) -> bool {
        self.start <= inner.start && inner.end() <= self.end()
    }
    pub const fn end(self) -> u32 {
        self.start + self.len
    }
}

/// The byte span of a tree-sitter node `[start_byte, end_byte)`.
pub fn node_span(node: tree_sitter::Node) -> Span {
    Span {
        start: node.start_byte() as u32,
        len: (node.end_byte() - node.start_byte()) as u32,
    }
}

/// Span from a declaration start through its body, or through the node when it has no body.
pub fn def_span(node: tree_sitter::Node) -> Span {
    let start = node.start_byte();
    let end = node.child_by_field_name("body").unwrap_or(node).end_byte();
    Span {
        start: start as u32,
        len: (end - start) as u32,
    }
}

/// UTF-8 text of a tree-sitter node, empty when its source slice is invalid.
pub fn node_text<'tree, 'src, N>(node: N, src: &'src [u8]) -> &'src str
where
    N: Borrow<tree_sitter::Node<'tree>>,
{
    node.borrow().utf8_text(src).unwrap_or("")
}
