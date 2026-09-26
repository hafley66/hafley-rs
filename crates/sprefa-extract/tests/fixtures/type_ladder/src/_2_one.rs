use crate::_0_types::{A, T};

pub struct OneField {
    pub a: A,
}

pub enum OneVariant {
    Has(A),
}

pub type OneAlias = A;

pub fn one_param(_a: A) {}

pub fn one_return() -> A {
    A
}

pub fn one_bound<X: T>(_x: X) {}

pub struct OneBound<X: T>(pub X);

impl T for OneField {}

impl From<A> for OneField {
    fn from(a: A) -> Self {
        OneField { a }
    }
}
