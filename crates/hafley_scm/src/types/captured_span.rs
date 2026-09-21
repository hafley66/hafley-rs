use std::ops::Range;

pub struct CapturedSpan {
    pub name: u16,
    pub bytes: Range<u32>,
}
