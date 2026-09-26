use crate::_0_types::{A, B, C, T, U};

pub struct ManyFields {
    pub a: A,
    pub b: B,
    pub c: C,
}

pub enum ManyVariants {
    First(A),
    Second(B),
    Third { c: C },
}

pub type ManyAliasA = A;
pub type ManyAliasB = B;

pub fn many_params(_a: A, _b: B, _c: C) {}

pub fn many_returns() -> (A, B) {
    (A, B)
}

pub fn many_bounds<X: T + U, Y>(_x: X, _y: Y)
where
    Y: T,
{
}

pub struct ManyBounds<X: T + U, Y>(pub X, pub Y)
where
    Y: T;

impl T for ManyFields {}
impl U for ManyFields {}

impl ManyFields {
    pub fn method(&self, _b: B) -> C {
        C
    }
}
