use crate::_0_types as types;
use crate::_0_types::A;

macro_rules! make_a {
    () => {
        A
    };
}

pub fn via_macro() -> A {
    make_a!()
}

pub fn aliased() -> types::A {
    types::A
}
