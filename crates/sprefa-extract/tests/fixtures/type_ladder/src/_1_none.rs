pub struct NoField {
    pub n: u32,
}

pub enum NoVariant {
    Empty,
    Num(u8),
}

pub type NoAlias = u32;

pub fn no_sig(n: u32) -> u32 {
    n
}

pub fn no_bound<X: Clone>(x: X) -> X {
    x
}

impl Clone for NoField {
    fn clone(&self) -> Self {
        NoField { n: self.n }
    }
}
