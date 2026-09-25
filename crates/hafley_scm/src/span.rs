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
