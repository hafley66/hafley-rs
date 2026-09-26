pub struct Span4 {
    pub at: u32,
}

pub struct Req4 {
    pub span: Span4,
}

pub struct Stays;

mod helper {
    use super::*;

    pub fn width(span: &Span4) -> u32 {
        span.at
    }
}
