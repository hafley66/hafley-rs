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
    pub const fn end(self) -> u32 {
        self.start + self.len
    }
}

/// UTF-8 text of a tree-sitter node, empty when its source slice is invalid.
pub fn node_text<'tree, 'src, N>(node: N, src: &'src [u8]) -> &'src str
where
    N: Borrow<tree_sitter::Node<'tree>>,
{
    node.borrow().utf8_text(src).unwrap_or("")
}
